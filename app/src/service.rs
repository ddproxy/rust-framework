//! The demo HTTP service: a health route and one bearer-authenticated route. Replace the routes
//! with your own; the listener, auth and request ids stay.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Router};
use fw_auth::{bearer_token, LoginThrottle, ThrottleSettings, TokenSet};
use fw_config::Config;
use fw_serve::{serve, shutdown_signal, with_request_ids, PeerAddr, ServeOptions};
use serde_json::json;
use tokio::net::TcpListener;

pub struct AppState {
    tokens: TokenSet,
    /// `None` when `[auth] throttle = false`.
    throttle: Option<LoginThrottle>,
}

impl AppState {
    pub fn from_config(config: &Config) -> Result<Self, String> {
        let tokens = TokenSet::new(
            config
                .auth
                .tokens
                .iter()
                .map(|t| (t.name.clone(), t.token_sha256.clone())),
        )
        .map_err(|e| e.to_string())?;
        let throttle = config.auth.throttle.then(|| {
            LoginThrottle::new(ThrottleSettings {
                max_failures: config.auth.max_failures,
                window: Duration::from_secs(config.auth.window_secs),
            })
        });
        Ok(Self { tokens, throttle })
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/whoami", get(whoami))
        .with_state(state);
    with_request_ids(app)
}

async fn healthz() -> Response {
    json_response(StatusCode::OK, &json!({ "status": "ok" }))
}

/// JSON without axum's `json` feature, which Reiki does not enable.
fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    let body = value.to_string();
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

async fn whoami(
    State(state): State<Arc<AppState>>,
    Extension(PeerAddr(peer)): Extension<PeerAddr>,
    headers: HeaderMap,
) -> Response {
    let ip = peer.ip();
    let now = Instant::now();
    if let Some(throttle) = &state.throttle {
        if let Some(wait) = throttle.blocked(ip, now) {
            let retry = wait.as_secs().max(1).to_string();
            tracing::warn!(%ip, "login throttled");
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(header::RETRY_AFTER, retry)],
                "too many failed attempts\n",
            )
                .into_response();
        }
    }
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(bearer_token);
    match presented.and_then(|token| state.tokens.verify(token)) {
        Some(name) => {
            if let Some(throttle) = &state.throttle {
                throttle.succeed(ip);
            }
            json_response(StatusCode::OK, &json!({ "name": name }))
        }
        None => {
            if let Some(throttle) = &state.throttle {
                throttle.fail(ip, now);
            }
            tracing::warn!(%ip, "bearer token rejected");
            (
                StatusCode::UNAUTHORIZED,
                [(header::WWW_AUTHENTICATE, "Bearer")],
                "unauthorized\n",
            )
                .into_response()
        }
    }
}

/// Binds the listener and serves until a shutdown signal.
pub async fn run(config: &Config, config_path: &Path) -> Result<(), String> {
    let state = Arc::new(AppState::from_config(config)?);
    let tls = match &config.tls {
        Some(tls) => Some(fw_serve::tls::load(
            Path::new(&tls.cert),
            Path::new(&tls.key),
        )?),
        None => None,
    };
    let listener = TcpListener::bind(&config.server.listen)
        .await
        .map_err(|e| format!("cannot listen on {}: {e}", config.server.listen))?;
    let addr: SocketAddr = listener.local_addr().map_err(|e| e.to_string())?;
    tracing::info!(
        %addr,
        tls = tls.is_some(),
        config = %config_path.display(),
        "listening"
    );
    let options = ServeOptions {
        max_connections: config.server.max_connections,
        max_connections_per_addr: config.server.max_connections_per_addr,
        header_read_timeout: Duration::from_secs(config.server.header_read_timeout_secs),
    };
    serve(listener, router(state), options, tls, shutdown_signal()).await;
    tracing::info!("stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fw_config::{AuthConfig, TokenEntry};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    const TOKEN: &str = "a-long-demo-token-for-tests";

    async fn start(max_failures: u32) -> (SocketAddr, tokio::sync::oneshot::Sender<()>) {
        let config = Config {
            auth: AuthConfig {
                throttle: true,
                max_failures,
                window_secs: 60,
                tokens: vec![TokenEntry {
                    name: "peer-a".to_owned(),
                    token_sha256: fw_auth::sha256_hex(TOKEN),
                }],
            },
            ..Config::default()
        };
        let state = Arc::new(AppState::from_config(&config).unwrap());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
        let options = ServeOptions {
            max_connections: 16,
            max_connections_per_addr: 8,
            header_read_timeout: Duration::from_secs(5),
        };
        tokio::spawn(serve(listener, router(state), options, None, async move {
            let _ = stopped.await;
        }));
        (addr, stop)
    }

    async fn send(addr: SocketAddr, headers: &str) -> String {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request =
            format!("GET /v1/whoami HTTP/1.1\r\nHost: t\r\nConnection: close\r\n{headers}\r\n");
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        stream.read_to_end(&mut out).await.unwrap();
        String::from_utf8_lossy(&out).into_owned()
    }

    fn status(response: &str) -> u16 {
        response
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn a_correct_bearer_token_gets_the_token_name() {
        let (addr, stop) = start(10).await;
        let reply = send(addr, &format!("Authorization: Bearer {TOKEN}\r\n")).await;
        assert_eq!(status(&reply), 200, "{reply}");
        assert!(reply.ends_with(r#"{"name":"peer-a"}"#), "{reply}");
        let _ = stop.send(());
    }

    #[tokio::test]
    async fn a_missing_or_wrong_token_is_401_with_a_challenge() {
        let (addr, stop) = start(10).await;
        let none = send(addr, "").await;
        assert_eq!(status(&none), 401);
        assert!(
            none.to_ascii_lowercase()
                .contains("www-authenticate: bearer"),
            "{none}"
        );
        assert_eq!(
            status(&send(addr, "Authorization: Bearer nope-nope-nope\r\n").await),
            401
        );
        let _ = stop.send(());
    }

    #[tokio::test]
    async fn hostile_authorization_values_are_401_and_never_crash_the_listener() {
        let (addr, stop) = start(100).await;
        let hostile = [
            "Authorization: Bearer\r\n",
            "Authorization: Bearer a b c\r\n",
            "Authorization: Basic cGVlci1hOng=\r\n",
            "Authorization: Bearer \u{1b}[31m\r\n",
            "Authorization: Bearer t\u{e9}st\r\n",
        ];
        for header in hostile {
            // hyper answers 400 to raw control bytes before the handler runs; either refusal is fine.
            let reply = send(addr, header).await;
            assert!(matches!(status(&reply), 400 | 401), "{header:?} -> {reply}");
        }
        assert_eq!(
            status(&send(addr, &format!("Authorization: Bearer {TOKEN}\r\n")).await),
            200
        );
        let _ = stop.send(());
    }

    #[tokio::test]
    async fn repeated_failures_block_the_address_even_with_the_right_token() {
        let (addr, stop) = start(3).await;
        for _ in 0..3 {
            assert_eq!(
                status(&send(addr, "Authorization: Bearer wrong-wrong-wrong\r\n").await),
                401
            );
        }
        let blocked = send(addr, &format!("Authorization: Bearer {TOKEN}\r\n")).await;
        assert_eq!(status(&blocked), 429, "{blocked}");
        assert!(
            blocked.to_ascii_lowercase().contains("retry-after:"),
            "{blocked}"
        );
        let _ = stop.send(());
    }

    #[test]
    fn a_hash_of_the_wrong_length_is_refused_at_startup() {
        let config = Config {
            auth: AuthConfig {
                tokens: vec![TokenEntry {
                    name: "bad".to_owned(),
                    token_sha256: "abc".to_owned(),
                }],
                ..AuthConfig::default()
            },
            ..Config::default()
        };
        assert!(AppState::from_config(&config).is_err());
    }
}
