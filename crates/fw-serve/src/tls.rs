//! TLS for the listener: a PEM certificate chain and key, served as HTTP/1.1 only.

use std::path::Path;
use std::sync::Arc;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Loads the chain and key. The key file's permissions are the caller's to check, because only
/// the caller knows whether the file is a secret it must refuse.
pub fn load(cert_path: &Path, key_path: &Path) -> Result<Arc<rustls::ServerConfig>, String> {
    let certs = CertificateDer::pem_file_iter(cert_path)
        .map_err(|e| format!("cannot read tls cert {}: {e}", cert_path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("bad tls cert {}: {e}", cert_path.display()))?;
    if certs.is_empty() {
        return Err(format!(
            "tls cert {} holds no certificate",
            cert_path.display()
        ));
    }
    let key = PrivateKeyDer::from_pem_file(key_path)
        .map_err(|e| format!("cannot read tls key {}: {e}", key_path.display()))?;
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("cannot use the tls cert with the tls key: {e}"))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}
