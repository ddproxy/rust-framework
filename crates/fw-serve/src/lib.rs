//! HTTP(S) listener. The accept loop caps connections globally and per remote address, times out
//! a client that never finishes its headers, and drains in-flight connections on shutdown.

mod limit;
mod request_id;
mod shutdown;
pub mod tls;

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::Router;
use hyper_util::rt::{TokioIo, TokioTimer};
use limit::AddrLimit;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tower_service::Service;

pub use request_id::{with_request_ids, X_REQUEST_ID};
pub use shutdown::shutdown_signal;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServeOptions {
    pub max_connections: usize,
    pub max_connections_per_addr: usize,
    pub header_read_timeout: Duration,
}

/// The remote socket address of a connection. The listener puts it in every request's
/// extensions, so handlers can key rate limits on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerAddr(pub SocketAddr);

/// Serves `router` until `shutdown` resolves. TLS is used when `tls` is set.
///
/// Refused connections are dropped without a response. A connection that is still open when the
/// drain timeout expires is abandoned.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    options: ServeOptions,
    tls: Option<Arc<rustls::ServerConfig>>,
    shutdown: impl Future<Output = ()>,
) {
    let slots = Arc::new(Semaphore::new(options.max_connections));
    let per_addr = AddrLimit::new(options.max_connections_per_addr);
    tokio::pin!(shutdown);
    loop {
        let accepted = tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => accepted,
        };
        let (stream, peer) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::warn!(%error, "accept failed");
                tokio::time::sleep(ACCEPT_BACKOFF).await;
                continue;
            }
        };
        let Some(addr_slot) = per_addr.acquire(peer.ip()) else {
            tracing::debug!(%peer, "per-address connection limit reached");
            continue;
        };
        let Ok(slot) = Arc::clone(&slots).try_acquire_owned() else {
            tracing::debug!(%peer, "connection limit reached");
            continue;
        };
        let router = router.clone();
        let tls = tls.clone();
        let timeout = options.header_read_timeout;
        tokio::spawn(async move {
            let _slot = slot;
            let _addr_slot = addr_slot;
            if let Err(error) = handle(stream, peer, router, tls, timeout).await {
                tracing::debug!(%peer, %error, "connection ended with an error");
            }
        });
    }
    drop(listener);
    // Let in-flight requests finish, but not forever.
    let all = u32::try_from(options.max_connections).unwrap_or(u32::MAX);
    if tokio::time::timeout(DRAIN_TIMEOUT, slots.acquire_many(all))
        .await
        .is_err()
    {
        tracing::warn!("shutdown: connections still open after the drain timeout");
    }
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn handle(
    stream: TcpStream,
    peer: SocketAddr,
    router: Router,
    tls: Option<Arc<rustls::ServerConfig>>,
    header_read_timeout: Duration,
) -> Result<(), BoxError> {
    match tls {
        None => connection(stream, router, peer, header_read_timeout).await?,
        Some(config) => {
            let accept = tokio_rustls::TlsAcceptor::from(config).accept(stream);
            let stream = tokio::time::timeout(HANDSHAKE_TIMEOUT, accept)
                .await
                .map_err(|_| "TLS handshake timed out")??;
            connection(stream, router, peer, header_read_timeout).await?;
        }
    }
    Ok(())
}

async fn connection<IO>(
    io: IO,
    router: Router,
    peer: SocketAddr,
    header_read_timeout: Duration,
) -> Result<(), hyper::Error>
where
    IO: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
        let mut router = router.clone();
        let mut req = req.map(Body::new);
        req.extensions_mut().insert(PeerAddr(peer));
        async move { router.call(req).await }
    });
    hyper::server::conn::http1::Builder::new()
        .timer(TokioTimer::new())
        .header_read_timeout(header_read_timeout)
        .serve_connection(TokioIo::new(io), service)
        .await
}
