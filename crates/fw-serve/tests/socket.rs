//! Runs the listener on real loopback sockets. Requests are written as raw HTTP/1.1 so the test
//! sees exactly what the server sent.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::routing::get;
use axum::{Extension, Router};
use fw_serve::{serve, with_request_ids, PeerAddr, ServeOptions};
use rustls::pki_types::{CertificateDer, ServerName};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

fn router() -> Router {
    let app = Router::new()
        .route("/ping", get(|| async { "pong" }))
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                "late"
            }),
        )
        .route(
            "/peer",
            get(|Extension(PeerAddr(peer)): Extension<PeerAddr>| async move { peer.to_string() }),
        );
    with_request_ids(app)
}

fn options(max_connections: usize, per_addr: usize) -> ServeOptions {
    ServeOptions {
        max_connections,
        max_connections_per_addr: per_addr,
        header_read_timeout: Duration::from_secs(5),
        request_timeout: Duration::from_secs(5),
    }
}

struct Running {
    addr: SocketAddr,
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
}

async fn start(options: ServeOptions, tls: Option<Arc<rustls::ServerConfig>>) -> Running {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (stop, stopped) = oneshot::channel::<()>();
    let task = tokio::spawn(serve(listener, router(), options, tls, async move {
        let _ = stopped.await;
    }));
    Running { addr, stop, task }
}

fn get_request(path: &str, extra: &str) -> String {
    format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n{extra}\r\n")
}

/// Sends one request and reads the whole response.
async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(mut stream: S, request: &str) -> String {
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    stream.read_to_end(&mut out).await.unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

async fn plain(addr: SocketAddr, request: &str) -> String {
    exchange(TcpStream::connect(addr).await.unwrap(), request).await
}

/// What a refused connection looks like: no bytes back, the close may arrive as a reset.
async fn refused(addr: SocketAddr) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let _ = stream.write_all(get_request("/ping", "").as_bytes()).await;
    let mut out = Vec::new();
    let _ = stream.read_to_end(&mut out).await;
    String::from_utf8_lossy(&out).into_owned()
}

fn status_line(response: &str) -> &str {
    response.lines().next().unwrap_or("")
}

fn header_value<'a>(response: &'a str, name: &str) -> Option<&'a str> {
    response.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

#[tokio::test]
async fn a_request_gets_an_id_and_a_hostile_incoming_id_is_replaced() {
    let server = start(options(8, 4), None).await;

    let kept = plain(
        server.addr,
        &get_request("/ping", "x-request-id: req-42\r\n"),
    )
    .await;
    assert!(status_line(&kept).contains("200"), "{kept}");
    assert_eq!(header_value(&kept, "x-request-id"), Some("req-42"));

    let long = "y".repeat(200);
    let replaced = plain(
        server.addr,
        &get_request("/ping", &format!("x-request-id: {long}\r\n")),
    )
    .await;
    let id = header_value(&replaced, "x-request-id").expect("an id is always set");
    assert_ne!(id, long, "an over-long id is not echoed");
    assert!(
        id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "generated: {id}"
    );

    let spaced = plain(
        server.addr,
        &get_request("/ping", "x-request-id: a b c\r\n"),
    )
    .await;
    assert!(!header_value(&spaced, "x-request-id").unwrap().contains(' '));

    let _ = server.stop.send(());
    server.task.await.unwrap();
}

#[tokio::test]
async fn the_handler_sees_the_peer_address() {
    let server = start(options(8, 4), None).await;
    let stream = TcpStream::connect(server.addr).await.unwrap();
    let local = stream.local_addr().unwrap();
    let response = exchange(stream, &get_request("/peer", "")).await;
    assert!(response.ends_with(&local.to_string()), "{response}");
    let _ = server.stop.send(());
    server.task.await.unwrap();
}

#[tokio::test]
async fn the_per_address_limit_refuses_extra_connections_and_frees_slots() {
    let server = start(options(8, 2), None).await;
    let idle_a = TcpStream::connect(server.addr).await.unwrap();
    let idle_b = TcpStream::connect(server.addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let reply = refused(server.addr).await;
    assert!(
        reply.is_empty(),
        "the third connection from one address is dropped: {reply:?}"
    );

    drop((idle_a, idle_b));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let served = plain(server.addr, &get_request("/ping", "")).await;
    assert!(
        status_line(&served).contains("200"),
        "slots were freed: {served}"
    );

    let _ = server.stop.send(());
    server.task.await.unwrap();
}

#[tokio::test]
async fn the_global_limit_refuses_connections_beyond_it() {
    let server = start(options(2, 10), None).await;
    let idle_a = TcpStream::connect(server.addr).await.unwrap();
    let idle_b = TcpStream::connect(server.addr).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let reply = refused(server.addr).await;
    assert!(reply.is_empty(), "{reply:?}");

    drop((idle_a, idle_b));
    let _ = server.stop.send(());
    server.task.await.unwrap();
}

#[tokio::test]
async fn shutdown_stops_the_listener() {
    let server = start(options(8, 4), None).await;
    let addr = server.addr;
    assert!(status_line(&plain(addr, &get_request("/ping", "")).await).contains("200"));
    let _ = server.stop.send(());
    tokio::time::timeout(Duration::from_secs(5), server.task)
        .await
        .expect("serve returns after shutdown")
        .unwrap();
    assert!(
        TcpStream::connect(addr).await.is_err(),
        "the port is closed"
    );
}

#[tokio::test]
async fn tls_serves_https_with_a_pem_cert_and_key() {
    let dir = std::env::temp_dir().join(format!("fw-serve-tls-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let key =
        rcgen::generate_simple_self_signed(vec!["localhost".to_owned(), "127.0.0.1".to_owned()])
            .unwrap();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, key.cert.pem()).unwrap();
    std::fs::write(&key_path, key.signing_key.serialize_pem()).unwrap();
    let server_config = fw_serve::tls::load(&cert_path, &key_path).expect("loads");

    let server = start(options(8, 4), Some(server_config)).await;

    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(CertificateDer::from(key.cert.der().to_vec()))
        .unwrap();
    let client_config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(roots)
    .with_no_client_auth();
    let connector = tokio_rustls::TlsConnector::from(Arc::new(client_config));
    let tcp = TcpStream::connect(server.addr).await.unwrap();
    let name = ServerName::try_from("localhost").unwrap();
    let tls = connector.connect(name, tcp).await.expect("handshake");
    let response = exchange(tls, &get_request("/ping", "")).await;
    assert!(response.ends_with("pong"), "{response}");

    let _ = server.stop.send(());
    server.task.await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tls_load_names_a_missing_file() {
    let missing = std::env::temp_dir().join("fw-serve-no-such-cert.pem");
    let error = fw_serve::tls::load(&missing, &missing).unwrap_err();
    assert!(error.contains("fw-serve-no-such-cert.pem"), "{error}");
}

#[tokio::test]
async fn a_handler_that_overruns_the_request_timeout_gets_a_408() {
    let mut short = options(8, 4);
    short.request_timeout = Duration::from_millis(200);
    let server = start(short, None).await;
    let started = std::time::Instant::now();
    let response = plain(server.addr, &get_request("/slow", "")).await;
    assert!(status_line(&response).contains("408"), "{response}");
    assert!(started.elapsed() < Duration::from_secs(5));
    let ok = plain(server.addr, &get_request("/ping", "")).await;
    assert!(
        status_line(&ok).contains("200"),
        "other requests still work: {ok}"
    );
    server.stop.send(()).unwrap();
    let _ = server.task.await;
}
