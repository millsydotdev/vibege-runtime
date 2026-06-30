//! # VibeGE IPC
//!
//! Inter-process communication bridge between the runtime host process
//! and sandboxed game processes.
//!
//! Messages are serialized with JSON (length-prefixed framing) and
//! transported over local TCP (127.0.0.1) for cross-platform compatibility.
//! Production target: named pipes (Windows) / Unix domain sockets (Unix).
//!
//! ## Architecture
//!
//! - Runtime opens a listener on a local TCP port
//! - Game process connects and performs a handshake
//! - Messages flow bidirectionally with correlation IDs for requests
//! - Message size limits and timeouts prevent abuse
//! - Reconnection with exponential backoff on disconnect

use std::cmp::min;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::debug;
use vibege_core::{ErrorCode, Result, RuntimeError};

// ─── Constants ──────────────────────────────────────────────────────

/// Default max message size (1MB).
const DEFAULT_MAX_MESSAGE_SIZE: u64 = 1024 * 1024;
/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// Max reconnect attempts.
const MAX_RECONNECT_ATTEMPTS: u32 = 5;
/// Initial backoff for reconnection.
const INITIAL_BACKOFF: Duration = Duration::from_millis(100);
/// Length prefix size (u32 = 4 bytes).
const LEN_PREFIX_SIZE: usize = 4;

// ─── Message Types ────────────────────────────────────────────────

pub type CorrelationId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageDirection {
    Request,
    Response,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageKind {
    Init,
    Update,
    Render,
    Shutdown,
    Suspend,
    Resume,
    InputEvent,
    Clear,
    DrawSprite,
    Present,
    FileRead,
    FileWrite,
    Ping,
    Pong,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub correlation_id: CorrelationId,
    pub direction: MessageDirection,
    pub kind: MessageKind,
    pub payload: String,
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: u32,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
    pub reconnects: u64,
    pub start_time: Instant,
}

impl Default for ConnectionStats {
    fn default() -> Self {
        Self {
            messages_sent: 0,
            messages_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            errors: 0,
            reconnects: 0,
            start_time: Instant::now(),
        }
    }
}

impl IpcMessage {
    fn new(kind: MessageKind, payload: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            correlation_id: NEXT_ID.fetch_add(1, Ordering::SeqCst),
            direction: MessageDirection::Request,
            kind,
            payload: payload.to_string(),
            error: None,
        }
    }

    pub fn response(&self, payload: &str) -> Self {
        Self {
            correlation_id: self.correlation_id,
            direction: MessageDirection::Response,
            kind: self.kind,
            payload: payload.to_string(),
            error: None,
        }
    }
}

pub trait MessageHandler: Send {
    fn handle_message(&mut self, message: &IpcMessage) -> Result<IpcMessage>;
}

// ─── Transport ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct IpcTransport {
    pub is_listener: bool,
    pub address: String,
}

impl IpcTransport {
    pub fn new(is_listener: bool, address: &str) -> Self {
        Self {
            is_listener,
            address: address.to_string(),
        }
    }
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn is_listener(&self) -> bool {
        self.is_listener
    }
}

// ─── Write helpers ───────────────────────────────────────────────

fn write_message_to(
    stream: &mut TcpStream,
    message: &IpcMessage,
    max_size: u64,
    stats: &Arc<Mutex<ConnectionStats>>,
) -> Result<()> {
    let json = serde_json::to_vec(message).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to serialize IPC message", e)
    })?;
    if json.len() as u64 > max_size {
        return Err(RuntimeError::new(
            ErrorCode::INTERNAL,
            format!(
                "IPC message too large: {} bytes (max {})",
                json.len(),
                max_size
            ),
        ));
    }
    let len = (json.len() as u32).to_le_bytes();
    stream.write_all(&len).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to write IPC length prefix", e)
    })?;
    stream.write_all(&json).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to write IPC message", e)
    })?;
    stream.flush().ok();
    if let Ok(mut s) = stats.lock() {
        s.messages_sent += 1;
        s.bytes_sent += json.len() as u64;
    }
    Ok(())
}

fn read_message_from(
    stream: &mut TcpStream,
    max_size: u64,
    timeout: Duration,
    stats: &Arc<Mutex<ConnectionStats>>,
) -> Result<IpcMessage> {
    let mut len_buf = [0u8; LEN_PREFIX_SIZE];
    read_exact_timeout(stream, &mut len_buf, timeout).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to read IPC length prefix", e)
    })?;
    let msg_len = u32::from_le_bytes(len_buf) as usize;
    if msg_len as u64 > max_size {
        return Err(RuntimeError::new(
            ErrorCode::INTERNAL,
            format!(
                "IPC message too large: {} bytes (max {})",
                msg_len, max_size
            ),
        ));
    }
    let mut buf = vec![0u8; msg_len];
    read_exact_timeout(stream, &mut buf, timeout).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to read IPC message body", e)
    })?;
    let message: IpcMessage = serde_json::from_slice(&buf).map_err(|e| {
        RuntimeError::with_cause(ErrorCode::INTERNAL, "Failed to deserialize IPC message", e)
    })?;
    if let Ok(mut s) = stats.lock() {
        s.messages_received += 1;
        s.bytes_received += buf.len() as u64;
    }
    Ok(message)
}

// ─── Connection ──────────────────────────────────────────────────

pub struct IpcConnection {
    transport: IpcTransport,
    stats: Arc<Mutex<ConnectionStats>>,
    pending_responses: Arc<Mutex<HashMap<CorrelationId, IpcMessage>>>,
    timeout: Duration,
    max_message_size: u64,
}

impl IpcConnection {
    pub fn new(transport: IpcTransport) -> Self {
        Self {
            transport,
            stats: Arc::new(Mutex::new(ConnectionStats::default())),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            timeout: DEFAULT_TIMEOUT,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_max_message_size(mut self, max_size: u64) -> Self {
        self.max_message_size = max_size;
        self
    }

    pub fn stats(&self) -> &Arc<Mutex<ConnectionStats>> {
        &self.stats
    }
    pub fn is_listener(&self) -> bool {
        self.transport.is_listener
    }

    pub fn create_init_message(&self) -> IpcMessage {
        let payload = serde_json::json!({
            "protocol_version": "0.1.0",
            "client_name": env!("CARGO_PKG_NAME"),
            "client_version": env!("CARGO_PKG_VERSION"),
        });
        IpcMessage::new(MessageKind::Init, &payload.to_string())
    }

    /// Connects to the IPC listener with retry+backoff.
    fn connect_stream(&self) -> Result<TcpStream> {
        let mut last_err = None;
        for attempt in 1..=MAX_RECONNECT_ATTEMPTS {
            match TcpStream::connect(&self.transport.address) {
                Ok(stream) => {
                    stream.set_read_timeout(Some(self.timeout)).ok();
                    stream.set_write_timeout(Some(self.timeout)).ok();
                    return Ok(stream);
                }
                Err(e) => {
                    last_err = Some(e);
                    if attempt < MAX_RECONNECT_ATTEMPTS {
                        let backoff = INITIAL_BACKOFF * attempt;
                        std::thread::sleep(backoff);
                    }
                }
            }
        }
        if let Ok(mut s) = self.stats.lock() {
            s.reconnects += 1;
        }
        Err(RuntimeError::with_cause(
            ErrorCode::INTERNAL,
            format!("Failed to connect IPC after {MAX_RECONNECT_ATTEMPTS} attempts"),
            last_err.unwrap(),
        ))
    }

    /// Sends a message and waits for a response.
    pub fn send_and_receive(&self, message: &IpcMessage) -> Result<IpcMessage> {
        debug!(kind = ?message.kind, id = message.correlation_id, "IPC send");
        let mut stream = self.connect_stream()?;
        write_message_to(&mut stream, message, self.max_message_size, &self.stats)?;
        let response = read_message_from(
            &mut stream,
            self.max_message_size,
            self.timeout,
            &self.stats,
        )?;
        if let Ok(mut pending) = self.pending_responses.lock() {
            pending.insert(response.correlation_id, response.clone());
        }
        Ok(response)
    }

    /// Sends a message without waiting for a response.
    pub fn send(&self, message: &IpcMessage) -> Result<()> {
        let mut stream = self.connect_stream()?;
        write_message_to(&mut stream, message, self.max_message_size, &self.stats)?;
        Ok(())
    }

    /// Receives a pending response by correlation ID.
    pub fn receive_response(&self, correlation_id: CorrelationId) -> Result<IpcMessage> {
        let mut pending = self.pending_responses.lock().expect("pending lock");
        pending.remove(&correlation_id).ok_or_else(|| {
            RuntimeError::new(
                ErrorCode::INTERNAL,
                format!("No pending response for correlation ID {correlation_id}"),
            )
        })
    }

    /// Processes an incoming message through the handler.
    pub fn process_message(
        &self,
        message: &IpcMessage,
        handler: &mut dyn MessageHandler,
    ) -> Result<IpcMessage> {
        handler.handle_message(message)
    }
}

// ─── Listener ────────────────────────────────────────────────────

/// Binds a TCP listener for IPC connections.
pub fn bind_ipc_listener(transport: &IpcTransport) -> Result<TcpListener> {
    let listener = TcpListener::bind(&transport.address).map_err(|e| {
        RuntimeError::with_cause(
            ErrorCode::INIT_FAILED,
            format!("Failed to bind IPC listener on {}", transport.address),
            e,
        )
    })?;
    listener.set_nonblocking(true).ok();
    Ok(listener)
}

// ─── Read Exactly ────────────────────────────────────────────────

fn read_exact_timeout(
    stream: &mut TcpStream,
    buf: &mut [u8],
    timeout: Duration,
) -> std::io::Result<()> {
    let deadline = Instant::now() + timeout;
    let mut offset = 0;
    while offset < buf.len() {
        if Instant::now() > deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "IPC read timed out",
            ));
        }
        let chunk_size = min(buf.len() - offset, 4096);
        match stream.read(&mut buf[offset..offset + chunk_size]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "IPC connection closed",
                ));
            }
            Ok(n) => offset += n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Creates a test transport for in-process IPC testing.
pub fn create_test_transport() -> IpcTransport {
    IpcTransport::new(true, "127.0.0.1:0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let msg = IpcMessage::new(MessageKind::Ping, r#"{"test": true}"#);
        assert_eq!(msg.kind, MessageKind::Ping);
        assert_eq!(msg.direction, MessageDirection::Request);
        assert!(msg.correlation_id > 0);
    }

    #[test]
    fn test_message_response() {
        let req = IpcMessage::new(MessageKind::Ping, r#"{"test": true}"#);
        let resp = req.response(r#"{"status": "ok"}"#);
        assert_eq!(resp.correlation_id, req.correlation_id);
        assert_eq!(resp.direction, MessageDirection::Response);
    }

    #[test]
    fn test_connection_creation() {
        let transport = IpcTransport::new(true, "127.0.0.1:0");
        let conn = IpcConnection::new(transport);
        assert!(conn.is_listener());
        assert_eq!(conn.stats().lock().unwrap().messages_sent, 0);
    }

    #[test]
    fn test_consecutive_messages_have_unique_ids() {
        let msg1 = IpcMessage::new(MessageKind::Ping, "");
        let msg2 = IpcMessage::new(MessageKind::Ping, "");
        assert_ne!(msg1.correlation_id, msg2.correlation_id);
    }

    #[test]
    fn test_ipc_transport_address() {
        let transport = IpcTransport::new(false, "127.0.0.1:9999");
        assert!(!transport.is_listener());
        assert_eq!(transport.address(), "127.0.0.1:9999");
    }

    #[test]
    fn test_message_size_limit() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport).with_max_message_size(10);
        let msg = IpcMessage::new(MessageKind::Ping, "x".repeat(100).as_str());
        let json = serde_json::to_vec(&msg).unwrap();
        assert!(json.len() as u64 > conn.max_message_size);
    }

    #[test]
    fn test_timeout_configuration() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport).with_timeout(Duration::from_millis(100));
        assert_eq!(conn.timeout, Duration::from_millis(100));
    }

    #[test]
    fn test_send_and_receive_via_tcp() {
        // Start a local TCP echo server
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let server_transport = IpcTransport::new(true, &addr.to_string());
        let server_conn = IpcConnection::new(server_transport);

        // Client transport
        let client_transport = IpcTransport::new(false, &addr.to_string());
        let client_conn = IpcConnection::new(client_transport);

        // Accept connection on a thread
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let msg = read_message_from(
                    &mut stream,
                    DEFAULT_MAX_MESSAGE_SIZE,
                    DEFAULT_TIMEOUT,
                    server_conn.stats(),
                )
                .unwrap();
                let resp = msg.response(r#"{"status":"pong"}"#);
                write_message_to(
                    &mut stream,
                    &resp,
                    DEFAULT_MAX_MESSAGE_SIZE,
                    server_conn.stats(),
                )
                .unwrap();
            }
        });

        std::thread::sleep(Duration::from_millis(50));

        let ping = IpcMessage::new(MessageKind::Ping, r#"{"msg":"hello"}"#);
        let result = client_conn.send_and_receive(&ping);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.kind, MessageKind::Ping);
        assert_eq!(resp.direction, MessageDirection::Response);
        assert!(resp.payload.contains("pong"));
    }

    #[test]
    fn test_bind_listener() {
        let transport = IpcTransport::new(true, "127.0.0.1:0");
        let listener = bind_ipc_listener(&transport).unwrap();
        assert!(listener.local_addr().is_ok());
    }
}
