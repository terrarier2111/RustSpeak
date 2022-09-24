use quinn::ClientConfig;

pub(crate) mod secure_authority {
    use quinn::ClientConfig;

    pub fn config() -> ClientConfig {
        ClientConfig::with_native_roots()
    }
}

// FIXME: we should probably remove this once we are out of the testing stage
pub(crate) mod insecure_local {
    use quinn::ClientConfig;
    use rustls::client::{ServerCertVerified, ServerCertVerifier};
    use rustls::{Certificate, ServerName};
    use std::sync::Arc;

    pub fn config() -> ClientConfig {
        let crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_no_client_auth();

        ClientConfig::new(Arc::new(crypto))
    }

    // Implementation of `ServerCertVerifier` that verifies everything as trustworthy.
    struct SkipServerVerification;

    impl SkipServerVerification {
        fn new() -> Arc<Self> {
            Arc::new(Self)
        }
    }

    impl ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &Certificate,
            _intermediates: &[Certificate],
            _server_name: &ServerName,
            _scts: &mut dyn Iterator<Item=&[u8]>,
            _ocsp_response: &[u8],
            _now: std::time::SystemTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }
    }
}
