use quinn::ServerConfig;
use rustls::{Certificate, Error, PrivateKey};

pub(crate) mod secure_authority {
    use std::fs::File;
    use std::io::BufReader;
    use rustls::{Certificate, PrivateKey};

    pub fn read_certs_from_file(
        cert_file: File,
        priv_key_file: File,
    ) -> anyhow::Result<(Vec<Certificate>, PrivateKey)> {
        let mut cert_chain_reader = BufReader::new(cert_file);
        let certs = rustls_pemfile::certs(&mut cert_chain_reader)?
            .into_iter()
            .map(Certificate)
            .collect();

        let mut key_reader = BufReader::new(priv_key_file);
        // if the file starts with "BEGIN RSA PRIVATE KEY"
        // let mut key_vec = rustls_pemfile::rsa_private_keys(&mut reader)?;
        // if the file starts with "BEGIN PRIVATE KEY"
        let mut keys = rustls_pemfile::pkcs8_private_keys(&mut key_reader)?;

        assert_eq!(keys.len(), 1);
        let key = PrivateKey(keys.remove(0));

        Ok((certs, key))
    }
}

pub(crate) mod insecure_local {
    use rustls::{Certificate, PrivateKey};

    pub fn generate_self_signed_cert(
    ) -> anyhow::Result<(Certificate, PrivateKey)> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let key = PrivateKey(cert.serialize_private_key_der());
        Ok((Certificate(cert.serialize_der()?), key))
    }
}

pub(crate) fn create_config(
    certs: Certificate,
    key: PrivateKey,
) -> Result<ServerConfig, Error> {
    ServerConfig::with_single_cert(vec![certs], key)
}
