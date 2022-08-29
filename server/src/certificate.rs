use quinn::crypto::rustls;
use quinn::ServerConfig;

mod secure_authority {
    use std::error::Error;
    use std::fs::File;
    use std::io::BufReader;
    use quinn::crypto::rustls;

    pub fn read_certs_from_file(cert_file: File, priv_key_file: File,
    ) -> Result<(Vec<rustls::Certificate>, rustls::PrivateKey), Box<dyn Error>> {
        let mut cert_chain_reader = BufReader::new(cert_file);
        let certs = rustls_pemfile::certs(&mut cert_chain_reader)?
            .into_iter()
            .map(rustls::Certificate)
            .collect();

        let mut key_reader = BufReader::new(priv_key_file);
        // if the file starts with "BEGIN RSA PRIVATE KEY"
        // let mut key_vec = rustls_pemfile::rsa_private_keys(&mut reader)?;
        // if the file starts with "BEGIN PRIVATE KEY"
        let mut keys = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?;

        assert_eq!(key_vec.len(), 1);
        let key = rustls::PrivateKey(keys.remove(0));

        Ok((certs, key))
    }
}

mod insecure_local {
    use std::error::Error;
    use quinn::crypto::rustls;

    pub fn generate_self_signed_cert() -> Result<(rustls::Certificate, rustls::PrivateKey), Box<dyn Error>> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let key = rustls::PrivateKey(cert.serialize_private_key_der());
        Ok((rustls::Certificate(cert.serialize_der()?), key))
    }
}

pub fn create_config(certs: rustls::Certificate, key: rustls::PrivateKey) -> Result<ServerConfig, rustls::Error> {
    ServerConfig::with_single_cert(certs, key)
}