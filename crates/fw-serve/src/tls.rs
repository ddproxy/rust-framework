//! TLS for the listener: a PEM certificate chain and key, served as HTTP/1.1 only. The leaf
//! certificate's expiry is checked at load: expired certificates are refused, and ones expiring
//! within 30 days are warned about.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};

/// Warn when the leaf certificate has less than this much validity left.
pub const WARN_BEFORE: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// A loaded listener config together with the leaf certificate's expiry.
#[derive(Debug, Clone)]
pub struct TlsInfo {
    pub config: Arc<rustls::ServerConfig>,
    pub not_after: SystemTime,
}

/// Expiry state of a certificate at a given time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expiry {
    Expired,
    ExpiringSoon(Duration),
    Valid,
}

/// Classifies `not_after` relative to `now`, saturating when the deadline has passed.
pub fn expiry(not_after: SystemTime, now: SystemTime) -> Expiry {
    let left = crate::time::remaining(not_after, now);
    if left.is_zero() {
        Expiry::Expired
    } else if left < WARN_BEFORE {
        Expiry::ExpiringSoon(left)
    } else {
        Expiry::Valid
    }
}

/// Loads the chain, checks the leaf certificate's validity and key, and returns the config with
/// the leaf's `not_after`. The key file's permissions are the caller's to check, because only the
/// caller knows whether the file is a secret it must refuse.
pub fn load_info(cert_path: &Path, key_path: &Path) -> Result<TlsInfo, String> {
    let certs = CertificateDer::pem_file_iter(cert_path)
        .map_err(|e| format!("cannot read tls cert {}: {e}", cert_path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("bad tls cert {}: {e}", cert_path.display()))?;
    let first = certs
        .first()
        .ok_or_else(|| format!("tls cert {} holds no certificate", cert_path.display()))?;
    let not_after = leaf_not_after(first, cert_path)?;
    match expiry(not_after, SystemTime::now()) {
        Expiry::Expired => {
            return Err(format!(
                "tls cert {} expired at unix time {}",
                cert_path.display(),
                crate::time::unix_secs(not_after)
            ));
        }
        Expiry::ExpiringSoon(left) => {
            tracing::warn!(
                seconds_left = left.as_secs(),
                "tls certificate expires within 30 days"
            );
        }
        Expiry::Valid => {}
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
    Ok(TlsInfo {
        config: Arc::new(config),
        not_after,
    })
}

/// Loads the chain and key. Refuses an expired leaf certificate.
pub fn load(cert_path: &Path, key_path: &Path) -> Result<Arc<rustls::ServerConfig>, String> {
    load_info(cert_path, key_path).map(|info| info.config)
}

fn leaf_not_after(cert: &CertificateDer<'_>, path: &Path) -> Result<SystemTime, String> {
    let (_, parsed) = x509_parser::parse_x509_certificate(cert.as_ref())
        .map_err(|e| format!("cannot parse tls cert {}: {e}", path.display()))?;
    // Certificate times are persisted values: clamp instead of overflowing on an absurd timestamp.
    Ok(crate::time::from_unix_secs(
        parsed.validity().not_after.timestamp(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use time::{Duration as TimeDuration, OffsetDateTime};
    use tracing_subscriber::layer::SubscriberExt;

    fn scratch(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("fw-serve-tls-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Throwaway self-signed test certificate and key; neither is a real credential.
    fn write_cert(
        dir: &Path,
        not_after: OffsetDateTime,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let mut params = rcgen::CertificateParams::new(vec!["localhost".to_owned()]).unwrap();
        params.not_before = OffsetDateTime::now_utc() - TimeDuration::days(2);
        params.not_after = not_after;
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        let cert_path = dir.join("test-cert.pem");
        let key_path = dir.join("test-key.pem");
        std::fs::write(&cert_path, cert.pem()).unwrap();
        std::fs::write(&key_path, key.serialize_pem()).unwrap();
        (cert_path, key_path)
    }

    #[derive(Clone, Default)]
    struct Messages(Arc<Mutex<Vec<String>>>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Messages {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _: tracing_subscriber::layer::Context<'_, S>,
        ) {
            struct Visitor(String);
            impl tracing::field::Visit for Visitor {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut visitor = Visitor(String::new());
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor.0);
        }
    }

    fn load_with_capture(cert: &Path, key: &Path) -> (Result<TlsInfo, String>, Vec<String>) {
        let messages = Messages::default();
        let subscriber = tracing_subscriber::registry().with(messages.clone());
        let result = tracing::subscriber::with_default(subscriber, || load_info(cert, key));
        let seen = messages.0.lock().unwrap().clone();
        (result, seen)
    }

    #[test]
    fn an_expired_certificate_is_refused_at_load() {
        let dir = scratch("expired");
        let (cert, key) = write_cert(&dir, OffsetDateTime::now_utc() - TimeDuration::days(1));
        let error = load(&cert, &key).unwrap_err();
        assert!(error.contains("expired"), "{error}");
    }

    #[test]
    fn a_certificate_expiring_soon_loads_but_is_flagged() {
        let dir = scratch("soon");
        let (cert, key) = write_cert(&dir, OffsetDateTime::now_utc() + TimeDuration::days(10));
        let (loaded, messages) = load_with_capture(&cert, &key);
        let info = loaded.expect("a valid certificate loads");
        assert!(
            messages
                .iter()
                .any(|message| message.contains("expires within 30 days")),
            "{messages:?}"
        );
        match expiry(info.not_after, SystemTime::now()) {
            Expiry::ExpiringSoon(left) => {
                assert!(left > Duration::ZERO && left <= WARN_BEFORE, "{left:?}");
            }
            other => panic!("expected a soon expiry, got {other:?}"),
        }
    }

    #[test]
    fn a_far_future_certificate_loads_and_reports_not_after() {
        let dir = scratch("future");
        let not_after = OffsetDateTime::now_utc() + TimeDuration::days(365);
        let (cert, key) = write_cert(&dir, not_after);
        let (loaded, messages) = load_with_capture(&cert, &key);
        let info = loaded.expect("a valid certificate loads");
        assert!(
            messages
                .iter()
                .all(|message| !message.contains("expires within 30 days")),
            "{messages:?}"
        );
        assert_eq!(expiry(info.not_after, SystemTime::now()), Expiry::Valid);
        assert_eq!(
            crate::time::unix_secs(info.not_after),
            not_after.unix_timestamp()
        );
    }

    #[test]
    fn expiry_classification_saturates_for_edge_times() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(expiry(now - Duration::from_secs(1), now), Expiry::Expired);
        assert_eq!(expiry(now, now), Expiry::Expired);
        assert_eq!(
            expiry(now + WARN_BEFORE - Duration::from_secs(1), now),
            Expiry::ExpiringSoon(WARN_BEFORE - Duration::from_secs(1))
        );
        assert_eq!(expiry(now + WARN_BEFORE, now), Expiry::Valid);
        assert_eq!(
            expiry(crate::time::from_unix_secs(i64::MAX), now),
            Expiry::Valid
        );
        assert_eq!(
            expiry(crate::time::from_unix_secs(-1), now),
            Expiry::Expired
        );
    }
}
