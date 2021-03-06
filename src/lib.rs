/*
   Copyright 2020 Vivint Smarthome

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
*/

#![cfg(not(doctest))]
// unfortunately the proto code includes comments from the google proto files
// that are interpreted as "doc tests" and will fail to build.
// When this PR is merged we should be able to remove this attribute:
// https://github.com/danburkert/prost/pull/291

use derivative::Derivative;
use futures::stream::StreamExt;
use opentelemetry::api::core::Value;
use opentelemetry::exporter::trace::{ExportResult, SpanData, SpanExporter};
use std::any::Any;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::convert::AsRef;
use std::time::{Duration, Instant};
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, ClientTlsConfig};

pub mod proto {
    pub mod google {
        pub mod api {
            tonic::include_proto!("google.api");
        }
        pub mod devtools {
            pub mod cloudtrace {
                pub mod v2 {
                    tonic::include_proto!("google.devtools.cloudtrace.v2");
                }
            }
        }
        pub mod protobuf {
            tonic::include_proto!("google.protobuf");
        }
        pub mod rpc {
            tonic::include_proto!("google.rpc");
        }
    }
}

use proto::google::devtools::cloudtrace::v2::span::time_event::Annotation;
use proto::google::devtools::cloudtrace::v2::span::TimeEvent;
use proto::google::devtools::cloudtrace::v2::trace_service_client::TraceServiceClient;
use proto::google::devtools::cloudtrace::v2::{AttributeValue, TruncatableString};

#[cfg(feature = "tokio_adapter")]
pub mod tokio_adapter;

/// Exports opentelemetry tracing spans to Google StackDriver.
///
/// As of the time of this writing, the opentelemetry crate exposes no link information
/// so this struct does not send link information.
#[derive(Derivative)]
#[derivative(Debug)]
pub struct StackDriverExporter {
    #[derivative(Debug = "ignore")]
    tx: futures::channel::mpsc::Sender<Vec<Arc<SpanData>>>,
    pending_count: Arc<AtomicUsize>,
    maximum_shutdown_duration: Duration,
}

impl StackDriverExporter {
    /// If `num_concurrent_requests` is set to `0` or `None` then no limit is enforced.
    pub async fn connect<S: futures::task::Spawn>(
        credentials_path: impl AsRef<std::path::Path>,
        spawn: &S,
        maximum_shutdown_duration: Option<Duration>,
        num_concurrent_requests: impl Into<Option<usize>>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let num_concurrent_requests = num_concurrent_requests.into();
        let uri = http::uri::Uri::from_static("https://cloudtrace.googleapis.com:443");

        let service_account_key = yup_oauth2::read_service_account_key(&credentials_path).await?;
        let project_name = service_account_key
            .project_id
            .as_ref()
            .ok_or("project_id is missing")?
            .clone();
        let authenticator = yup_oauth2::ServiceAccountAuthenticator::builder(service_account_key)
            .build()
            .await?;
        let scopes = &["https://www.googleapis.com/auth/trace.append"];
        let token = authenticator.token(scopes).await?;
        let bearer_token = format!("Bearer {}", token.as_str());

        let header_value = MetadataValue::from_str(&bearer_token)?;

        let tls_config = ClientTlsConfig::new().domain_name(uri.host().unwrap());

        let channel = Channel::builder(uri)
            .tls_config(tls_config)
            .connect()
            .await?;
        let (tx, rx) = futures::channel::mpsc::channel(64);
        let pending_count = Arc::new(AtomicUsize::new(0));
        spawn.spawn_obj(
            Box::new(Self::export_inner(
                TraceServiceClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
                    req.metadata_mut()
                        .insert("authorization", header_value.clone());
                    Ok(req)
                }),
                project_name,
                rx,
                pending_count.clone(),
                num_concurrent_requests,
            ))
            .into(),
        )?;

        Ok(Self {
            tx,
            pending_count,
            maximum_shutdown_duration: maximum_shutdown_duration.unwrap_or(Duration::from_secs(5)),
        })
    }

    pub fn pending_count(&self) -> usize {
        self.pending_count.load(Ordering::Relaxed)
    }

    async fn export_inner(
        client: TraceServiceClient<Channel>,
        project_name: String,
        rx: futures::channel::mpsc::Receiver<Vec<Arc<SpanData>>>,
        pending_count: Arc<AtomicUsize>,
        num_concurrent: impl Into<Option<usize>>,
    ) {
        rx.for_each_concurrent(num_concurrent, move |batch| {
            let mut client = client.clone(); // This clone is cheap and allows for concurrent requests (see https://github.com/hyperium/tonic/issues/285#issuecomment-595880400)
            let project_name = project_name.clone();
            let pending_count = pending_count.clone();
            async move {
                use proto::google::devtools::cloudtrace::v2::span::time_event::Value;
                use proto::google::devtools::cloudtrace::v2::span::{Attributes, TimeEvents};
                use proto::google::devtools::cloudtrace::v2::{BatchWriteSpansRequest, Span};

                let spans = batch
                    .into_iter()
                    .map(|span| {
                        let new_attributes = Attributes {
                            attribute_map: span
                                .attributes
                                .iter()
                                .map(|(key, value)| {
                                    let key = match key.inner().as_ref() {
                                        "httpRequest.requestUrl" => String::from("/http/url"),
                                        "httpRequest.status" => String::from("/http/status_code"),
                                        "httpRequest.method" => String::from("/http/method"),
                                        "httpRequest.responseSize" => String::from("/http/response/size"),
                                        key => key.to_string()
                                    };

                                    (
                                        key,
                                        attribute_value_conversion(value),
                                    )
                                })
                                .collect(),
                            ..Default::default()
                        };
                        let new_time_events = TimeEvents {
                            time_event: span
                                .message_events
                                .iter()
                                .map(|event| TimeEvent {
                                    time: Some(event.timestamp.into()),
                                    value: Some(Value::Annotation(Annotation {
                                        description: Some(to_truncate(event.name.clone())),
                                        ..Default::default()
                                    })),
                                })
                                .collect(),
                            ..Default::default()
                        };

                        Span {
                            name: format!(
                                "projects/{}/traces/{}/spans/{}",
                                project_name,
                                hex::encode(span.context.trace_id().to_u128().to_be_bytes()),
                                hex::encode(span.context.span_id().to_u64().to_be_bytes())
                            ),
                            display_name: Some(to_truncate(span.name.clone())),
                            span_id: hex::encode(span.context.span_id().to_u64().to_be_bytes()),
                            parent_span_id: hex::encode(span.parent_span_id.to_u64().to_be_bytes()),
                            start_time: Some(span.start_time.into()),
                            end_time: Some(span.end_time.into()),
                            attributes: Some(new_attributes),
                            time_events: Some(new_time_events),
                            ..Default::default()
                        }
                    })
                    .collect::<Vec<_>>();

                let req = BatchWriteSpansRequest {
                    name: format!("projects/{}", project_name),
                    spans,
                };
                client
                    .batch_write_spans(req)
                    .await
                    .map_err(|e| {
                        log::error!("StackDriver push failed {:?}", e);
                    })
                    .ok();
                pending_count.fetch_sub(1, Ordering::Relaxed);
            }
        })
        .await;
    }
}

impl SpanExporter for StackDriverExporter {
    fn export(&self, batch: Vec<Arc<SpanData>>) -> ExportResult {
        match self.tx.clone().try_send(batch) {
            Err(e) => {
                log::error!("Unable to send to export_inner {:?}", e);
                if e.is_disconnected() {
                    ExportResult::FailedNotRetryable
                } else {
                    ExportResult::FailedRetryable
                }
            }
            _ => {
                self.pending_count.fetch_add(1, Ordering::Relaxed);
                ExportResult::Success
            }
        }
    }

    fn shutdown(&self) {
        let start = Instant::now();
        while (Instant::now() - start) < self.maximum_shutdown_duration && self.pending_count() > 0
        {
            std::thread::yield_now();
            // Spin for a bit and give the inner export some time to upload, with a timeout.
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn attribute_value_conversion(v: &Value) -> AttributeValue {
    use proto::google::devtools::cloudtrace::v2::attribute_value;
    let new_value = match v {
        Value::Bool(v) => attribute_value::Value::BoolValue(*v),
        Value::Bytes(v) => attribute_value::Value::StringValue(to_truncate(hex::encode(v))),
        Value::F64(v) => attribute_value::Value::StringValue(to_truncate(v.to_string())),
        Value::I64(v) => attribute_value::Value::IntValue(*v),
        Value::String(v) => attribute_value::Value::StringValue(to_truncate(v.clone())),
        Value::U64(v) => attribute_value::Value::IntValue(*v as i64),
    };
    AttributeValue {
        value: Some(new_value),
    }
}

fn to_truncate(s: String) -> TruncatableString {
    TruncatableString {
        value: s,
        ..Default::default()
    }
}
