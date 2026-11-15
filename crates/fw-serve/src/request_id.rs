use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use tracing::Instrument;

pub const X_REQUEST_ID: &str = "x-request-id";

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Gives each request an id. A caller's `x-request-id` is kept when it is a short plain token, so
/// a value from the network can never inject text into the log. Otherwise an id is generated.
/// The id is set on the request's log span and echoed on the response.
pub fn with_request_ids(router: Router) -> Router {
    router.layer(middleware::from_fn(request_id))
}

async fn request_id(req: Request, next: Next) -> Response {
    let id = incoming_id(req.headers().get(X_REQUEST_ID)).unwrap_or_else(generate_id);
    let method = req.method().clone();
    // The path only: a query string may carry a credential.
    let path = req.uri().path().to_owned();
    let span = tracing::info_span!("request", id = %id, method = %method, path = %path);
    let started = Instant::now();
    let mut response = next.run(req).instrument(span.clone()).await;
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let status = response.status().as_u16();
    span.in_scope(|| tracing::info!(status, elapsed_ms, "request finished"));
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(X_REQUEST_ID, value);
    }
    response
}

fn incoming_id(value: Option<&HeaderValue>) -> Option<String> {
    let text = value?.to_str().ok()?;
    let plain = (1..=64).contains(&text.len())
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    plain.then(|| text.to_owned())
}

fn generate_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{sequence:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(value: &str) -> HeaderValue {
        HeaderValue::from_str(value).unwrap()
    }

    #[test]
    fn a_plain_incoming_id_is_kept() {
        assert_eq!(
            incoming_id(Some(&header("req-1.a_B"))).as_deref(),
            Some("req-1.a_B")
        );
    }

    #[test]
    fn hostile_incoming_ids_are_dropped_for_a_generated_one() {
        let hostile = [
            "",
            "has space",
            "log\u{1b}[2J",
            "line\u{7}bell",
            "{\"injected\":true}",
            "a/b",
        ];
        for value in hostile {
            let bytes = HeaderValue::from_bytes(value.as_bytes());
            if let Ok(bytes) = bytes {
                assert_eq!(incoming_id(Some(&bytes)), None, "{value:?}");
            }
        }
        let long = "x".repeat(65);
        assert_eq!(incoming_id(Some(&header(&long))), None);
        assert_eq!(incoming_id(None), None);
    }

    #[test]
    fn generated_ids_are_plain_and_distinct() {
        let a = generate_id();
        let b = generate_id();
        assert_ne!(a, b);
        assert!(
            incoming_id(Some(&header(&a))).is_some(),
            "a generated id passes the same check"
        );
    }
}
