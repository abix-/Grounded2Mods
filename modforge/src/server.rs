//! HTTP listener that dispatches POSTs at a single endpoint to a
//! caller-supplied handler. Synchronous (`tiny_http`), one worker
//! thread, no async runtime.
//!
//! The handler is opaque: it receives the raw request body string and
//! returns the JSON bytes to ship back. This keeps `ueforge` neutral
//! about snapshot type, op set, and error shape. All of which live
//! in the embedding crate.

use std::io::{Cursor, Read};
use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;

use parking_lot::Mutex;
use socket2::{Domain, Protocol, Socket, Type};
use tiny_http::{Header, Method, Response, Server};

/// Cap on POST body size. Localhost-only API, but a misbehaving
/// client (or a stuck file upload) would otherwise read until OOM.
/// 1 MiB is ~1000x the largest legitimate op payload we ship.
const MAX_BODY_BYTES: u64 = 1 << 20;

/// Constant-time byte-string compare. Avoids the early-exit timing
/// signal in `==` for auth-token equality. Localhost weakens the
/// attacker model but the fix is ~5 LoC. No excuse to skip it.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Configuration for the embedded server.
pub struct Config {
    /// Bind address suffix; the server binds `127.0.0.1:<port>`.
    pub port: u16,
    /// URL path that accepts POSTs. Anything else returns 404.
    pub endpoint: &'static str,
    /// Name applied to the listener thread (visible in tools / logs).
    pub thread_name: &'static str,
    /// Optional per-launch auth token. When `Some(t)`, every
    /// request must carry `X-Ueforge-Auth: <t>` or it gets a
    /// 401. Stops cross-process localhost actors from hitting
    /// `write_bytes` / `call`. When `None`, no auth is enforced
    /// (the historical default; safe for closed-loop dev where
    /// tests are the only client).
    ///
    /// Callers typically generate via `format!("{:032x}",
    /// rand::random::<u128>())` at startup, write to a file
    /// the test client reads, and `Box::leak` to get
    /// `&'static str`.
    pub auth_token: Option<&'static str>,
}

/// Handle to a spawned listener. Drop or call [`stop`](Self::stop)
/// to break the listener's `incoming_requests` loop and join the
/// thread. Required for hot-reload-safe shutdown. The DLL can't
/// unload while the listener thread holds code on its stack.
pub struct SpawnHandle {
    server: Arc<Server>,
    join: Mutex<Option<thread::JoinHandle<()>>>,
}

impl SpawnHandle {
    /// Unblock the listener loop and join the thread. Idempotent.
    pub fn stop(&self) {
        // unblock() makes server.incoming_requests() return None
        // on its next iteration, breaking the for-loop in run().
        self.server.unblock();
        if let Some(j) = self.join.lock().take() {
            let _ = j.join();
        }
    }
}

impl Drop for SpawnHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Global registry of `SpawnHandle`s so `shutdown_all` can stop
/// listeners owned by call sites that don't hang on to the handle
/// (typical for `debug::spawn` callers that fire-and-forget).
static SERVER_REGISTRY: Mutex<Vec<SpawnHandle>> = Mutex::new(Vec::new());

/// Stop every server registered via [`spawn`]. Called from the
/// framework's hot-reload shutdown path.
pub fn shutdown_all() {
    let mut g = SERVER_REGISTRY.lock();
    let n = g.len();
    g.clear();
    drop(g);
    if n > 0 {
        crate::log!("server: shutdown_all stopped {n} listener(s)");
    }
}

/// Spawn the listener thread. Returns immediately. If bind fails the
/// error is reported via `on_log` and the thread is not started.
/// On success the handle is registered into the framework's
/// shutdown registry; pass `register: false` to opt out (e.g. tests
/// that own the handle directly).
pub fn spawn<H, L>(cfg: Config, handler: H, on_log: L)
where
    H: Fn(&str) -> Vec<u8> + Send + Sync + 'static,
    L: Fn(&str) + Send + 'static,
{
    let addr_str = format!("127.0.0.1:{}", cfg.port);
    // Hot reload friendliness: a prior generation's listener may
    // have closed its socket microseconds ago, but Windows holds
    // the port in TIME_WAIT for several minutes by default. Set
    // SO_REUSEADDR on our socket so the bind succeeds anyway.
    // Without this, every hot-reload that touches the listener
    // crate would fail to rebind for ~4 minutes on Windows.
    let server = {
        let addr: SocketAddr = match addr_str.parse() {
            Ok(a) => a,
            Err(e) => {
                on_log(&format!("ueforge: bad addr {addr_str}: {e}"));
                return;
            }
        };
        let sock = match Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP)) {
            Ok(s) => s,
            Err(e) => {
                on_log(&format!("ueforge: socket() failed: {e}"));
                return;
            }
        };
        if let Err(e) = sock.set_reuse_address(true) {
            on_log(&format!("ueforge: SO_REUSEADDR failed: {e}"));
            // continue anyway; bind may still succeed
        }
        if let Err(e) = sock.bind(&addr.into()) {
            on_log(&format!("ueforge: bind {addr_str} failed: {e}"));
            return;
        }
        if let Err(e) = sock.listen(128) {
            on_log(&format!("ueforge: listen {addr_str} failed: {e}"));
            return;
        }
        let listener: std::net::TcpListener = sock.into();
        match Server::from_listener(listener, None) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                on_log(&format!("ueforge: tiny_http from_listener failed: {e}"));
                return;
            }
        }
    };
    on_log(&format!("ueforge: listening on {addr_str}{}", cfg.endpoint));

    let handler = Arc::new(handler);
    let server_for_thread = server.clone();
    let endpoint = cfg.endpoint;
    let auth_token = cfg.auth_token;
    let join = thread::Builder::new()
        .name(cfg.thread_name.into())
        .spawn(move || run(server_for_thread, endpoint, handler, auth_token))
        .expect("spawn listener thread");

    SERVER_REGISTRY.lock().push(SpawnHandle {
        server,
        join: Mutex::new(Some(join)),
    });
}

fn run<H>(
    server: Arc<Server>,
    endpoint: &'static str,
    handler: Arc<H>,
    auth_token: Option<&'static str>,
) where
    H: Fn(&str) -> Vec<u8> + Send + Sync + 'static,
{
    for mut req in server.incoming_requests() {
        let url = req.url().to_string();
        let method = req.method().clone();

        if method != Method::Post || url != endpoint {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
            continue;
        }

        if let Some(expected) = auth_token {
            let provided = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("X-Ueforge-Auth"))
                .map(|h| h.value.as_str());
            let ok = match provided {
                Some(p) => ct_eq(p.as_bytes(), expected.as_bytes()),
                None => false,
            };
            if !ok {
                let _ = req.respond(Response::from_string("unauthorized").with_status_code(401));
                continue;
            }
        }

        // Cap the body read at MAX_BODY_BYTES + 1: if the reader
        // produces more than the cap, we reject as 413.
        let mut buf = Vec::new();
        let read_result = req
            .as_reader()
            .take(MAX_BODY_BYTES + 1)
            .read_to_end(&mut buf);
        if read_result.is_err() {
            let _ = req.respond(Response::from_string("bad body").with_status_code(400));
            continue;
        }
        if buf.len() as u64 > MAX_BODY_BYTES {
            let _ = req.respond(Response::from_string("payload too large").with_status_code(413));
            continue;
        }
        let body = match String::from_utf8(buf) {
            Ok(s) => s,
            Err(_) => {
                let _ = req.respond(Response::from_string("bad body").with_status_code(400));
                continue;
            }
        };

        let payload = handler(&body);
        let len = payload.len();
        let resp = Response::new(
            200.into(),
            vec![
                Header::from_bytes(&b"content-type"[..], &b"application/json"[..])
                    .expect("static header parse"),
            ],
            Cursor::new(payload),
            Some(len),
            None,
        );
        let _ = req.respond(resp);
    }
}
