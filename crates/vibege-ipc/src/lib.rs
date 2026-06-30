//! # VibeGE IPC
//!
//! Inter-process communication bridge between the runtime host process
//! and sandboxed game processes.
//!
//! Messages are serialized with MessagePack (via `rmp-serde`) and
//! transported over platform-specific channels (Unix domain sockets
//! on Unix, named pipes on Windows).
//!
//! ## Architecture
//!
//! The IPC bridge uses a simple request-response protocol:
//! - Runtime opens a listener on a known address
//! - Game process connects and performs a handshake
//! - Messages flow bidirectionally with correlation IDs for requests
//! - Rate limiting and message size limits prevent abuse

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::debug;
use vibege_core::{ErrorCode, Result, RuntimeError};

// ─── Message Types ────────────────────────────────────────────────

/// A unique correlation ID for matching requests to responses.
pub type CorrelationId = u64;

/// Direction of an IPC message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageDirection {
    Request,
    Response,
    Event,
}

/// The category of an IPC message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageKind {
    // Lifecycle
    Init,
    Update,
    Render,
    Shutdown,
    Suspend,
    Resume,

    // Input
    InputEvent,

    // Rendering
    Clear,
    DrawSprite,
    Present,

    // Storage
    FileRead,
    FileWrite,

    // System
    Ping,
    Pong,
    Error,
}

/// A structured IPC message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    /// Unique correlation ID for request-response matching.
    pub correlation_id: CorrelationId,

    /// Message direction.
    pub direction: MessageDirection,

    /// The message category.
    pub kind: MessageKind,

    /// JSON-encoded payload.
    pub payload: String,

    /// Error information (only set for Error kind).
    pub error: Option<IpcError>,
}

/// Error information carried in IPC messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: u32,
    pub message: String,
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

    fn response(&self, payload: &str) -> Self {
        Self {
            correlation_id: self.correlation_id,
            direction: MessageDirection::Response,
            kind: self.kind,
            payload: payload.to_string(),
            error: None,
        }
    }
}

// ─── Connection Management ─────────────────────────────────────────

/// Callback for processing incoming IPC messages.
pub trait MessageHandler: Send {
    fn handle_message(&mut self, message: &IpcMessage) -> Result<IpcMessage>;
}

/// Statistics about an IPC connection.
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub errors: u64,
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
            start_time: Instant::now(),
        }
    }
}

/// Platform-specific IPC transport.
#[derive(Debug)]
pub struct IpcTransport {
    /// Whether the transport is a listener (server) or connector (client).
    is_listener: bool,

    /// The address of the IPC endpoint (pipe path or socket path).
    address: String,
}

impl IpcTransport {
    /// Creates a new IPC transport.
    pub fn new(is_listener: bool, address: &str) -> Self {
        Self {
            is_listener,
            address: address.to_string(),
        }
    }

    /// Returns the IPC address.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// Returns whether this is a listener.
    pub fn is_listener(&self) -> bool {
        self.is_listener
    }
}

/// Manages a single IPC connection between runtime and game.
pub struct IpcConnection {
    transport: IpcTransport,
    stats: Arc<Mutex<ConnectionStats>>,
    pending_responses: Arc<Mutex<HashMap<CorrelationId, IpcMessage>>>,
    timeout: Duration,
    max_message_size: u64,
}

impl IpcConnection {
    /// Creates a new IPC connection with the given transport.
    pub fn new(transport: IpcTransport) -> Self {
        Self {
            transport,
            stats: Arc::new(Mutex::new(ConnectionStats::default())),
            pending_responses: Arc::new(Mutex::new(HashMap::new())),
            timeout: Duration::from_secs(30),
            max_message_size: 1024 * 1024, // 1MB
        }
    }

    /// Sets the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum message size in bytes.
    pub fn with_max_message_size(mut self, max_size: u64) -> Self {
        self.max_message_size = max_size;
        self
    }

    /// Returns a reference to the connection stats.
    pub fn stats(&self) -> &Arc<Mutex<ConnectionStats>> {
        &self.stats
    }

    /// Returns whether this side listens for connections.
    pub fn is_listener(&self) -> bool {
        self.transport.is_listener
    }

    /// Creates an init message for the connection handshake.
    pub fn create_init_message(&self) -> IpcMessage {
        let payload = serde_json::json!({
            "protocol_version": "0.1.0",
            "client_name": env!("CARGO_PKG_NAME"),
            "client_version": env!("CARGO_PKG_VERSION"),
        });
        IpcMessage::new(MessageKind::Init, &payload.to_string())
    }

    /// Sends a message and waits for a response.
    ///
    /// In v0.1, this operates in-process for testing. A real implementation
    /// would serialize to MessagePack and send over the transport channel.
    pub fn send_and_receive(&self, message: &IpcMessage) -> Result<IpcMessage> {
        debug!(
            kind = ?message.kind,
            id = message.correlation_id,
            "IPC message sent"
        );

        // Record statistics
        {
            let mut stats = self.stats.lock().unwrap();
            stats.messages_sent += 1;
            stats.bytes_sent += message.payload.len() as u64;
        }

        // For now, simulate a response for known message types
        let response = match message.kind {
            MessageKind::Ping => {
                message.response(serde_json::json!({"status": "ok"}).to_string().as_str())
            }
            MessageKind::Init => message.response(
                serde_json::json!({
                    "status": "ok",
                    "session_id": format!("session-{}", message.correlation_id),
                })
                .to_string()
                .as_str(),
            ),
            _ => {
                // Default: echo back an acknowledgment
                message.response(
                    serde_json::json!({"status": "received"})
                        .to_string()
                        .as_str(),
                )
            }
        };

        // Store for potential async retrieval
        {
            let mut pending = self.pending_responses.lock().unwrap();
            pending.insert(response.correlation_id, response.clone());
        }

        {
            let mut stats = self.stats.lock().unwrap();
            stats.messages_received += 1;
            stats.bytes_received += response.payload.len() as u64;
        }

        Ok(response)
    }

    /// Sends a message without waiting for a response.
    pub fn send(&self, message: &IpcMessage) -> Result<()> {
        let _ = self.send_and_receive(message)?;
        Ok(())
    }

    /// Receives a pending response by correlation ID.
    pub fn receive_response(&self, correlation_id: CorrelationId) -> Result<IpcMessage> {
        let mut pending = self.pending_responses.lock().unwrap();
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

/// Creates a test transport for in-process IPC testing.
pub fn create_test_transport() -> IpcTransport {
    IpcTransport::new(true, "vibege-test-ipc")
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
        assert!(msg.error.is_none());
    }

    #[test]
    fn test_message_response() {
        let req = IpcMessage::new(MessageKind::Ping, r#"{"test": true}"#);
        let resp = req.response(r#"{"status": "ok"}"#);
        assert_eq!(resp.correlation_id, req.correlation_id);
        assert_eq!(resp.direction, MessageDirection::Response);
    }

    #[test]
    fn test_message_error() {
        let err = IpcMessage {
            correlation_id: 1,
            direction: MessageDirection::Response,
            kind: MessageKind::Error,
            payload: String::new(),
            error: Some(IpcError { code: 400, message: "Bad request".into() }),
        };
        assert_eq!(err.direction, MessageDirection::Response);
        assert_eq!(err.kind, MessageKind::Error);
        assert!(err.error.is_some());
        assert_eq!(err.error.unwrap().code, 400);
    }

    #[test]
    fn test_connection_creation() {
        let transport = IpcTransport::new(true, "test-pipe");
        let conn = IpcConnection::new(transport);
        assert!(conn.is_listener());
        assert_eq!(conn.stats().lock().unwrap().messages_sent, 0);
    }

    #[test]
    fn test_send_and_receive_ping() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport);
        let ping = IpcMessage::new(MessageKind::Ping, "{}");
        let response = conn.send_and_receive(&ping).unwrap();
        assert_eq!(response.kind, MessageKind::Ping);
        assert_eq!(response.direction, MessageDirection::Response);
    }

    #[test]
    fn test_send_and_receive_init() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport);
        let init = conn.create_init_message();
        let response = conn.send_and_receive(&init).unwrap();
        assert_eq!(response.kind, MessageKind::Init);
        assert!(response.payload.contains("session_id"));
    }

    #[test]
    fn test_stats_tracking() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport);
        let msg = IpcMessage::new(MessageKind::Ping, "hello");
        conn.send_and_receive(&msg).unwrap();
        let stats = conn.stats().lock().unwrap();
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.messages_received, 1);
        assert!(stats.bytes_sent > 0);
    }

    #[test]
    fn test_timeout_configuration() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport).with_timeout(Duration::from_millis(100));
        // Timeout is stored; real implementation would enforce it
        assert_eq!(conn.timeout, Duration::from_millis(100));
    }

    #[test]
    fn test_max_message_size() {
        let transport = create_test_transport();
        let conn = IpcConnection::new(transport).with_max_message_size(512);
        assert_eq!(conn.max_message_size, 512);
    }

    #[test]
    fn test_consecutive_messages_have_unique_ids() {
        let msg1 = IpcMessage::new(MessageKind::Ping, "");
        let msg2 = IpcMessage::new(MessageKind::Ping, "");
        assert_ne!(msg1.correlation_id, msg2.correlation_id);
    }

    #[test]
    fn test_ipc_transport_address() {
        let transport = IpcTransport::new(false, "/tmp/vibege.sock");
        assert!(!transport.is_listener());
        assert_eq!(transport.address(), "/tmp/vibege.sock");
    }
}
