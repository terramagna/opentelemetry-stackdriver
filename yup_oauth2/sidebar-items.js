initSidebarItems({"enum":[["Error","Encapsulates all possible results of the `token(...)` operation"],["InstalledFlowReturnMethod","cf. https://developers.google.com/identity/protocols/OAuth2InstalledApp#choosingredirecturi"]],"fn":[["parse_application_secret","Read an application secret from a JSON string."],["read_application_secret","Read an application secret from a file."],["read_service_account_key","Read a service account key from a JSON file. You can download the JSON keys from the Google Cloud Console or the respective console of your service provider."]],"mod":[["authenticator","Module contianing the core functionality for OAuth2 Authentication."],["authenticator_delegate","Module containing types related to delegates."],["error","Module containing various error types."]],"struct":[["AccessToken","Represents an access token returned by oauth2 servers. All access tokens are Bearer tokens. Other types of tokens are not supported."],["ApplicationSecret","Represents either 'installed' or 'web' applications in a json secrets file. See `ConsoleApplicationSecret` for more information"],["ConsoleApplicationSecret","A type to facilitate reading and writing the json secret file as returned by the google developer console"],["DeviceFlowAuthenticator","Create an authenticator that uses the device flow. `# async fn foo() { # let app_secret = yup_oauth2::read_application_secret(\"/tmp/foo\").await.unwrap();     let authenticator = yup_oauth2::DeviceFlowAuthenticator::builder(app_secret)         .build()         .await         .expect(\"failed to create authenticator\"); # }`"],["InstalledFlowAuthenticator","Create an authenticator that uses the installed flow. `# async fn foo() { # use yup_oauth2::InstalledFlowReturnMethod; # let custom_flow_delegate = yup_oauth2::authenticator_delegate::DefaultInstalledFlowDelegate; # let app_secret = yup_oauth2::read_application_secret(\"/tmp/foo\").await.unwrap();     let authenticator = yup_oauth2::InstalledFlowAuthenticator::builder(         app_secret,         InstalledFlowReturnMethod::HTTPRedirect,     )     .build()     .await     .expect(\"failed to create authenticator\"); # }`"],["ServiceAccountAuthenticator","Create an authenticator that uses a service account. `# async fn foo() { # let service_account_key = yup_oauth2::read_service_account_key(\"/tmp/foo\").await.unwrap();     let authenticator = yup_oauth2::ServiceAccountAuthenticator::builder(service_account_key)         .build()         .await         .expect(\"failed to create authenticator\"); # }`"],["ServiceAccountKey","JSON schema of secret service account key. You can obtain the key from the Cloud Console at https://console.cloud.google.com/."]]});