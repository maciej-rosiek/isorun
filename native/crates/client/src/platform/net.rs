//! A WebTiles connection (the port of `connection.ts` `RemoteConnection`): a WebSocket that asks
//! for the `no-compression` subprotocol, unpacks the server's `{"msgs": [...]}` batches in order and
//! answers its pings. The socket runs on a thread natively and on the browser's own WebSocket on
//! the web; either way the frame polls what came (`poll`).

use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub enum ConnEvent {
    Open,
    Message(Value),
    Closed { code: u16, reason: String },
    Diagnostic(String),
}

/// What a connection carries: a server's socket, or the offline engine.
pub trait Transport {
    fn send(&mut self, text: String);
    fn close(&mut self);
    fn poll(&mut self, out: &mut Vec<ConnEvent>);
    fn reopen(&mut self);
}

type Wake = Arc<dyn Fn() + Send + Sync>;

/// The events a socket's thread or callbacks leave for the frame.
#[derive(Clone, Default)]
struct Inbox(Arc<Mutex<VecDeque<ConnEvent>>>);

impl Inbox {
    fn push(&self, e: ConnEvent) {
        if let Ok(mut q) = self.0.lock() {
            q.push_back(e);
        }
    }
    fn drain(&self, out: &mut Vec<ConnEvent>) {
        if let Ok(mut q) = self.0.lock() {
            out.extend(q.drain(..));
        }
    }
}

/// Unpack one text frame into its messages, in order; a ping is answered here and kept from the game.
fn dispatch(text: &str, inbox: &Inbox, pong: &mut dyn FnMut()) {
    match isorun_webtiles::unpack_frame(text) {
        Ok(msgs) => {
            for m in msgs {
                if m.get("msg").and_then(Value::as_str) == Some("ping") {
                    pong();
                    continue;
                }
                inbox.push(ConnEvent::Message(m));
            }
        }
        Err(e) => inbox.push(ConnEvent::Diagnostic(format!("{e:?}"))),
    }
}

/// A server's socket.
pub struct RemoteTransport {
    url: String,
    inbox: Inbox,
    wake: Wake,
    generation: u64,
    #[cfg(not(target_arch = "wasm32"))]
    outgoing: Option<std::sync::mpsc::Sender<Option<String>>>,
    #[cfg(target_arch = "wasm32")]
    socket: Option<web_sys::WebSocket>,
    #[cfg(target_arch = "wasm32")]
    queued: Arc<Mutex<Vec<String>>>,
}

impl RemoteTransport {
    pub fn new(url: &str, wake: Wake) -> Self {
        let mut t = RemoteTransport {
            url: url.to_string(),
            inbox: Inbox::default(),
            wake,
            generation: 0,
            #[cfg(not(target_arch = "wasm32"))]
            outgoing: None,
            #[cfg(target_arch = "wasm32")]
            socket: None,
            #[cfg(target_arch = "wasm32")]
            queued: Arc::new(Mutex::new(Vec::new())),
        };
        t.connect();
        t
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn connect(&mut self) {
        use std::sync::mpsc::{channel, TryRecvError};
        use tungstenite::client::IntoClientRequest;
        self.generation += 1;
        let (tx, rx) = channel::<Option<String>>();
        self.outgoing = Some(tx);
        let inbox = self.inbox.clone();
        let wake = self.wake.clone();
        let url = self.url.clone();
        let _ = std::thread::Builder::new().name("websocket".into()).spawn(move || {
            let closed = |code: u16, reason: String| {
                inbox.push(ConnEvent::Closed { code, reason });
                wake();
            };
            let mut request = match url.as_str().into_client_request() {
                Ok(r) => r,
                Err(e) => return closed(1006, e.to_string()),
            };
            request.headers_mut().insert("Sec-WebSocket-Protocol", "no-compression".parse().unwrap());
            let (mut socket, _) = match tungstenite::connect(request) {
                Ok(s) => s,
                Err(e) => return closed(1006, e.to_string()),
            };
            // a short read timeout, so the same thread sends what the frame queues
            let stream = socket.get_mut();
            let timeout = Some(std::time::Duration::from_millis(15));
            match stream {
                tungstenite::stream::MaybeTlsStream::Plain(s) => {
                    let _ = s.set_read_timeout(timeout);
                    let _ = s.set_nodelay(true);
                }
                tungstenite::stream::MaybeTlsStream::Rustls(s) => {
                    let _ = s.get_ref().set_read_timeout(timeout);
                    let _ = s.get_ref().set_nodelay(true);
                }
                _ => {}
            }
            inbox.push(ConnEvent::Open);
            wake();
            loop {
                loop {
                    match rx.try_recv() {
                        Ok(Some(text)) => {
                            if let Err(e) = socket.send(tungstenite::Message::text(text)) {
                                return closed(1006, e.to_string());
                            }
                        }
                        Ok(None) | Err(TryRecvError::Disconnected) => {
                            let _ = socket.close(None);
                            let _ = socket.flush();
                            return closed(1000, String::new());
                        }
                        Err(TryRecvError::Empty) => break,
                    }
                }
                match socket.read() {
                    Ok(tungstenite::Message::Text(text)) => {
                        let mut pongs = 0;
                        dispatch(text.as_str(), &inbox, &mut || pongs += 1);
                        for _ in 0..pongs {
                            let _ = socket.send(tungstenite::Message::text(r#"{"msg":"pong"}"#));
                        }
                        wake();
                    }
                    Ok(tungstenite::Message::Binary(_)) => inbox.push(ConnEvent::Diagnostic("binary frame received; compression is not supported".into())),
                    Ok(tungstenite::Message::Close(frame)) => {
                        let (code, reason) = frame.map(|f| (u16::from(f.code), f.reason.to_string())).unwrap_or((1005, String::new()));
                        return closed(code, reason);
                    }
                    Ok(_) => {}
                    Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => return closed(1006, String::new()),
                    Err(e) => return closed(1006, e.to_string()),
                }
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    fn connect(&mut self) {
        use wasm_bindgen::closure::Closure;
        use wasm_bindgen::JsCast;
        self.generation += 1;
        let protocols = js_sys::Array::of1(&"no-compression".into());
        let socket = match web_sys::WebSocket::new_with_str_sequence(&self.url, &protocols) {
            Ok(s) => s,
            Err(e) => {
                self.inbox.push(ConnEvent::Closed { code: 1006, reason: format!("{e:?}") });
                return;
            }
        };
        socket.set_binary_type(web_sys::BinaryType::Arraybuffer);
        let (inbox, wake) = (self.inbox.clone(), self.wake.clone());
        let queued = self.queued.clone();
        let opened = socket.clone();
        let on_open = Closure::<dyn FnMut()>::new(move || {
            for text in queued.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default() {
                let _ = opened.send_with_str(&text);
            }
            inbox.push(ConnEvent::Open);
            wake();
        });
        socket.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();
        let (inbox, wake) = (self.inbox.clone(), self.wake.clone());
        let ponger = socket.clone();
        let on_message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            let text = if let Some(t) = e.data().as_string() {
                t
            } else if let Ok(buffer) = e.data().dyn_into::<js_sys::ArrayBuffer>() {
                String::from_utf8_lossy(&js_sys::Uint8Array::new(&buffer).to_vec()).into_owned()
            } else {
                return;
            };
            dispatch(&text, &inbox, &mut || {
                let _ = ponger.send_with_str(r#"{"msg":"pong"}"#);
            });
            wake();
        });
        socket.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();
        let (inbox, wake) = (self.inbox.clone(), self.wake.clone());
        let on_close = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(move |e: web_sys::CloseEvent| {
            inbox.push(ConnEvent::Closed { code: e.code(), reason: e.reason() });
            wake();
        });
        socket.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
        self.socket = Some(socket);
    }
}

impl Transport for RemoteTransport {
    fn send(&mut self, text: String) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(tx) = &self.outgoing {
            let _ = tx.send(Some(text));
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = &self.socket {
            if s.ready_state() == web_sys::WebSocket::OPEN {
                let _ = s.send_with_str(&text);
            } else if let Ok(mut q) = self.queued.lock() {
                q.push(text);
            }
        }
    }

    fn close(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(tx) = self.outgoing.take() {
            let _ = tx.send(None);
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(s) = self.socket.take() {
            let _ = s.close();
        }
    }

    fn poll(&mut self, out: &mut Vec<ConnEvent>) {
        self.inbox.drain(out);
    }

    fn reopen(&mut self) {
        self.close();
        self.inbox = Inbox::default();
        self.connect();
    }
}

/// A connection with its open/closed state, as the session sees it.
pub struct Connection {
    transport: Box<dyn Transport>,
    pub open: bool,
    pub closed: bool,
    pub gamedata_base: String,
    /// What was sent before the socket opened, to go out when it does.
    pending: Vec<String>,
}

impl Connection {
    pub fn new(transport: Box<dyn Transport>, gamedata_base: String) -> Self {
        Connection { transport, open: false, closed: false, gamedata_base, pending: Vec::new() }
    }

    pub fn send(&mut self, msg: &Value) {
        let text = msg.to_string();
        if self.open {
            self.transport.send(text);
        } else if !self.closed {
            self.pending.push(text);
        }
    }

    pub fn close(&mut self) {
        self.transport.close();
    }

    pub fn reopen(&mut self) {
        self.open = false;
        self.closed = false;
        self.transport.reopen();
    }

    pub fn poll(&mut self) -> Vec<ConnEvent> {
        let mut out = Vec::new();
        self.transport.poll(&mut out);
        for e in &out {
            match e {
                ConnEvent::Open => {
                    self.open = true;
                    for text in std::mem::take(&mut self.pending) {
                        self.transport.send(text);
                    }
                }
                ConnEvent::Closed { .. } => {
                    self.open = false;
                    self.closed = true;
                    self.pending.clear();
                }
                _ => {}
            }
        }
        out
    }
}
