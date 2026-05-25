use super::backend::{TerminalBackend, TerminalBackendEvent, open_backend, validate_size};
use super::ssh::TerminalSshAuth;
use super::ssh_known_hosts::TerminalSshTrustStore;
use crate::ffi::shared::shared_runtime;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

const OUTPUT_CHANNEL_CAPACITY: usize = 1024;
const SESSION_REPLAY_LIMIT_BYTES: usize = 64 * 1024;
const TERMINAL_SESSION_ID_PREFIX: &str = "terminal-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TerminalSessionId {
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DroidPtySessionRequest {
    pub node_id: String,
    pub token: String,
    pub relay: Option<String>,
    pub agent: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DroidPtySessionHandle {
    pub session_id: TerminalSessionId,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TerminalBackendKind {
    LocalIsh {
        cwd: Option<String>,
    },
    LocalProot {
        cwd: Option<String>,
    },
    RemoteAlleycat {
        node_id: String,
        token: String,
        relay: Option<String>,
        shell: Option<String>,
    },
    RemoteDroidPty {
        node_id: String,
        token: String,
        relay: Option<String>,
        agent: Option<String>,
        cwd: Option<String>,
    },
    RemoteSsh {
        host: String,
        port: u16,
        username: String,
        auth: TerminalSshAuth,
        shell: Option<String>,
        accept_unknown_host: bool,
        cwd: Option<String>,
    },
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TerminalError {
    #[error("Unsupported: {detail}")]
    Unsupported { detail: String },
    #[error("Invalid size: {detail}")]
    InvalidSize { detail: String },
    #[error("Invalid input: {detail}")]
    InvalidInput { detail: String },
    #[error("Unknown terminal session: {session_id}")]
    UnknownSession { session_id: String },
    #[error("Unavailable: {detail}")]
    Unavailable { detail: String },
    #[error("Protocol: {detail}")]
    Protocol { detail: String },
    #[error("Transport disconnected: {detail}")]
    TransportDisconnected { detail: String },
    #[error("Permission denied: {detail}")]
    Permission { detail: String },
    #[error("Backend: {detail}")]
    Backend { detail: String },
    #[error("Terminal session is closed")]
    Closed,
}

#[uniffi::export(callback_interface)]
pub trait TerminalOutputListener: Send + Sync {
    fn on_bytes(&self, data: Vec<u8>);
    fn on_exit(&self, code: i32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TerminalEventKind {
    Output,
    Exit,
    Overflow,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TerminalEvent {
    pub session_id: String,
    pub sequence: u64,
    pub kind: TerminalEventKind,
    pub data: Vec<u8>,
    pub exit_code: Option<i32>,
    pub dropped_event_count: u64,
}

#[uniffi::export(callback_interface)]
pub trait TerminalEventListener: Send + Sync {
    fn on_event(&self, event: TerminalEvent);
}

#[derive(Debug, Clone)]
enum TerminalOutputEvent {
    Bytes(Vec<u8>),
    Exit(i32),
}

#[derive(Debug, Clone)]
struct TerminalOutputEnvelope {
    sequence: u64,
    event: TerminalOutputEvent,
}

#[derive(Debug, Default)]
struct TerminalOutputHistory {
    next_sequence: u64,
    events: VecDeque<TerminalOutputEnvelope>,
    byte_count: usize,
}

#[derive(Debug)]
struct TerminalOutputReplay {
    replayed_through: Option<u64>,
    events: Vec<TerminalOutputEnvelope>,
}

impl TerminalOutputHistory {
    fn record(&mut self, event: &TerminalOutputEvent) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);

        let stored_event = match event {
            TerminalOutputEvent::Bytes(data) if data.len() > SESSION_REPLAY_LIMIT_BYTES => {
                TerminalOutputEvent::Bytes(data[data.len() - SESSION_REPLAY_LIMIT_BYTES..].to_vec())
            }
            TerminalOutputEvent::Bytes(data) => TerminalOutputEvent::Bytes(data.clone()),
            TerminalOutputEvent::Exit(code) => TerminalOutputEvent::Exit(*code),
        };
        if let TerminalOutputEvent::Bytes(data) = &stored_event {
            self.byte_count = self.byte_count.saturating_add(data.len());
        }
        self.events.push_back(TerminalOutputEnvelope {
            sequence,
            event: stored_event,
        });
        self.trim();
        sequence
    }

    fn snapshot(&self) -> TerminalOutputReplay {
        TerminalOutputReplay {
            replayed_through: self.next_sequence.checked_sub(1),
            events: self.events.iter().cloned().collect(),
        }
    }

    fn trim(&mut self) {
        while self.byte_count > SESSION_REPLAY_LIMIT_BYTES {
            let Some(removed) = self.events.pop_front() else {
                self.byte_count = 0;
                break;
            };
            if let TerminalOutputEvent::Bytes(data) = removed.event {
                self.byte_count = self.byte_count.saturating_sub(data.len());
            }
        }
    }
}

#[derive(uniffi::Object)]
pub struct TerminalSession {
    backend: Arc<dyn TerminalBackend>,
    output_tx: broadcast::Sender<TerminalOutputEnvelope>,
    output_history: Arc<Mutex<TerminalOutputHistory>>,
    session_id: Arc<Mutex<Option<String>>>,
    closed: Arc<AtomicBool>,
    rt: Arc<tokio::runtime::Runtime>,
}

#[uniffi::export(async_runtime = "tokio")]
impl TerminalSession {
    #[uniffi::constructor]
    pub async fn open(
        backend: TerminalBackendKind,
        size: TerminalSize,
    ) -> Result<Self, TerminalError> {
        Self::open_managed(backend, size, None, Some(new_terminal_session_id())).await
    }

    /// Same as [`Self::open`] but consults `trust_store` for the SSH backend
    /// host-key pin policy. Local backends ignore `trust_store`.
    #[uniffi::constructor]
    pub async fn open_with_trust_store(
        backend: TerminalBackendKind,
        size: TerminalSize,
        trust_store: Arc<TerminalSshTrustStore>,
    ) -> Result<Self, TerminalError> {
        Self::open_managed(
            backend,
            size,
            Some(trust_store),
            Some(new_terminal_session_id()),
        )
        .await
    }

    pub async fn write_input(&self, data: Vec<u8>) -> Result<(), TerminalError> {
        self.ensure_open()?;
        self.backend.write(&data).await
    }

    pub async fn resize(&self, size: TerminalSize) -> Result<(), TerminalError> {
        self.ensure_open()?;
        validate_size(size)?;
        self.backend.resize(size).await
    }

    pub fn subscribe_output(&self, listener: Box<dyn TerminalOutputListener>) {
        let mut rx = self.output_tx.subscribe();
        let replay = self.output_history.lock().unwrap().snapshot();
        let listener: Arc<dyn TerminalOutputListener> = Arc::from(listener);
        self.rt.spawn(async move {
            let mut replayed_through = replay.replayed_through;
            for envelope in replay.events {
                deliver_terminal_output(&listener, envelope.event.clone());
                replayed_through = Some(envelope.sequence);
                if matches!(envelope.event, TerminalOutputEvent::Exit(_)) {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(envelope)
                        if replayed_through.is_some_and(|seen| envelope.sequence <= seen) => {}
                    Ok(envelope) => {
                        let is_exit = matches!(envelope.event, TerminalOutputEvent::Exit(_));
                        deliver_terminal_output(&listener, envelope.event);
                        if is_exit {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // Keep consuming fresh bytes. Late subscribers get a
                        // bounded replay window, but a slow live listener can
                        // still fall behind if it stops draining entirely.
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn subscribe_events(&self, listener: Box<dyn TerminalEventListener>) {
        let mut rx = self.output_tx.subscribe();
        let replay = self.output_history.lock().unwrap().snapshot();
        let session_id = self.session_id.clone();
        let listener: Arc<dyn TerminalEventListener> = Arc::from(listener);
        self.rt.spawn(async move {
            let mut last_delivered = replay.replayed_through;
            for envelope in replay.events {
                deliver_terminal_event(
                    &listener,
                    &session_id,
                    envelope.sequence,
                    envelope.event.clone(),
                );
                last_delivered = Some(envelope.sequence);
                if matches!(envelope.event, TerminalOutputEvent::Exit(_)) {
                    return;
                }
            }
            loop {
                match rx.recv().await {
                    Ok(envelope)
                        if last_delivered.is_some_and(|seen| envelope.sequence <= seen) => {}
                    Ok(envelope) => {
                        let is_exit = matches!(envelope.event, TerminalOutputEvent::Exit(_));
                        deliver_terminal_event(
                            &listener,
                            &session_id,
                            envelope.sequence,
                            envelope.event,
                        );
                        last_delivered = Some(envelope.sequence);
                        if is_exit {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(dropped)) => {
                        let sequence = last_delivered
                            .map(|seen| seen.saturating_add(1))
                            .unwrap_or(0);
                        deliver_terminal_overflow_event(
                            &listener,
                            &session_id,
                            sequence,
                            dropped as u64,
                        );
                        last_delivered = Some(sequence);
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    pub fn session_id(&self) -> Option<TerminalSessionId> {
        self.session_id
            .lock()
            .unwrap()
            .clone()
            .map(|value| TerminalSessionId { value })
    }

    pub async fn close_session(&self) -> Result<(), TerminalError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        self.backend.close().await
    }
}

impl TerminalSession {
    pub(crate) async fn open_managed(
        backend: TerminalBackendKind,
        size: TerminalSize,
        trust_store: Option<Arc<TerminalSshTrustStore>>,
        session_id: Option<String>,
    ) -> Result<Self, TerminalError> {
        let rt = shared_runtime();
        let (backend, output_rx) = open_backend(backend, size, trust_store).await?;
        Ok(Self::from_open_backend_with_session_id(
            backend, output_rx, rt, session_id,
        ))
    }

    fn from_open_backend(
        backend: Arc<dyn TerminalBackend>,
        output_rx: tokio::sync::mpsc::Receiver<TerminalBackendEvent>,
        rt: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self::from_open_backend_with_session_id(
            backend,
            output_rx,
            rt,
            Some(new_terminal_session_id()),
        )
    }

    fn from_open_backend_with_session_id(
        backend: Arc<dyn TerminalBackend>,
        mut output_rx: tokio::sync::mpsc::Receiver<TerminalBackendEvent>,
        rt: Arc<tokio::runtime::Runtime>,
        session_id: Option<String>,
    ) -> Self {
        let (output_tx, _) = broadcast::channel(OUTPUT_CHANNEL_CAPACITY);
        let output_history = Arc::new(Mutex::new(TerminalOutputHistory::default()));
        let forward_tx = output_tx.clone();
        let forward_history = output_history.clone();
        rt.spawn(async move {
            while let Some(event) = output_rx.recv().await {
                let output = match event {
                    TerminalBackendEvent::Bytes(data) => TerminalOutputEvent::Bytes(data),
                    TerminalBackendEvent::Exit(code) => TerminalOutputEvent::Exit(code),
                };
                let is_exit = matches!(output, TerminalOutputEvent::Exit(_));
                let sequence = forward_history.lock().unwrap().record(&output);
                let _ = forward_tx.send(TerminalOutputEnvelope {
                    sequence,
                    event: output,
                });
                if is_exit {
                    break;
                }
            }
        });
        Self {
            backend,
            output_tx,
            output_history,
            session_id: Arc::new(Mutex::new(session_id)),
            closed: Arc::new(AtomicBool::new(false)),
            rt,
        }
    }

    fn ensure_open(&self) -> Result<(), TerminalError> {
        if self.closed.load(Ordering::SeqCst) {
            Err(TerminalError::Closed)
        } else {
            Ok(())
        }
    }
}

fn deliver_terminal_output(listener: &Arc<dyn TerminalOutputListener>, event: TerminalOutputEvent) {
    match event {
        TerminalOutputEvent::Bytes(data) => listener.on_bytes(data),
        TerminalOutputEvent::Exit(code) => listener.on_exit(code),
    }
}

fn deliver_terminal_event(
    listener: &Arc<dyn TerminalEventListener>,
    session_id: &Arc<Mutex<Option<String>>>,
    sequence: u64,
    event: TerminalOutputEvent,
) {
    let session_id = session_id.lock().unwrap().clone().unwrap_or_default();
    let event = match event {
        TerminalOutputEvent::Bytes(data) => TerminalEvent {
            session_id,
            sequence,
            kind: TerminalEventKind::Output,
            data,
            exit_code: None,
            dropped_event_count: 0,
        },
        TerminalOutputEvent::Exit(code) => TerminalEvent {
            session_id,
            sequence,
            kind: TerminalEventKind::Exit,
            data: Vec::new(),
            exit_code: Some(code),
            dropped_event_count: 0,
        },
    };
    listener.on_event(event);
}

fn deliver_terminal_overflow_event(
    listener: &Arc<dyn TerminalEventListener>,
    session_id: &Arc<Mutex<Option<String>>>,
    sequence: u64,
    dropped_event_count: u64,
) {
    listener.on_event(TerminalEvent {
        session_id: session_id.lock().unwrap().clone().unwrap_or_default(),
        sequence,
        kind: TerminalEventKind::Overflow,
        data: Vec::new(),
        exit_code: None,
        dropped_event_count,
    });
}

pub(crate) fn new_terminal_session_id() -> String {
    format!("{TERMINAL_SESSION_ID_PREFIX}{}", uuid::Uuid::new_v4())
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    #[derive(Default)]
    struct FakeBackend {
        writes: Mutex<Vec<Vec<u8>>>,
        resizes: Mutex<Vec<TerminalSize>>,
        closed: AtomicBool,
    }

    #[async_trait]
    impl TerminalBackend for FakeBackend {
        async fn write(&self, data: &[u8]) -> Result<(), TerminalError> {
            self.writes.lock().unwrap().push(data.to_vec());
            Ok(())
        }

        async fn resize(&self, size: TerminalSize) -> Result<(), TerminalError> {
            self.resizes.lock().unwrap().push(size);
            Ok(())
        }

        async fn close(&self) -> Result<(), TerminalError> {
            self.closed.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    struct CapturingListener {
        bytes: Arc<Mutex<Vec<Vec<u8>>>>,
        exits: Arc<Mutex<Vec<i32>>>,
    }

    impl TerminalOutputListener for CapturingListener {
        fn on_bytes(&self, data: Vec<u8>) {
            self.bytes.lock().unwrap().push(data);
        }

        fn on_exit(&self, code: i32) {
            self.exits.lock().unwrap().push(code);
        }
    }

    struct CapturingEventListener {
        events: Arc<Mutex<Vec<TerminalEvent>>>,
    }

    impl TerminalEventListener for CapturingEventListener {
        fn on_event(&self, event: TerminalEvent) {
            self.events.lock().unwrap().push(event);
        }
    }

    #[tokio::test]
    async fn session_forwards_io_and_lifecycle_to_backend() {
        let backend = Arc::new(FakeBackend::default());
        let (tx, rx) = mpsc::channel(8);
        let rt = shared_runtime();
        let session = TerminalSession::from_open_backend(backend.clone(), rx, rt);
        assert!(session
            .session_id()
            .expect("session id")
            .value
            .starts_with("terminal-"));

        let bytes = Arc::new(Mutex::new(Vec::new()));
        let exits = Arc::new(Mutex::new(Vec::new()));
        session.subscribe_output(Box::new(CapturingListener {
            bytes: bytes.clone(),
            exits: exits.clone(),
        }));

        session.write_input(b"echo hi\n".to_vec()).await.unwrap();
        session
            .resize(TerminalSize {
                cols: 100,
                rows: 40,
            })
            .await
            .unwrap();

        tx.send(TerminalBackendEvent::Bytes(b"hi\n".to_vec()))
            .await
            .unwrap();
        tx.send(TerminalBackendEvent::Exit(0)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        assert_eq!(
            backend.writes.lock().unwrap().as_slice(),
            &[b"echo hi\n".to_vec()]
        );
        assert_eq!(
            backend.resizes.lock().unwrap().as_slice(),
            &[TerminalSize {
                cols: 100,
                rows: 40
            }]
        );
        assert_eq!(bytes.lock().unwrap().as_slice(), &[b"hi\n".to_vec()]);
        assert_eq!(exits.lock().unwrap().as_slice(), &[0]);

        session.close_session().await.unwrap();
        assert!(backend.closed.load(Ordering::SeqCst));
        assert!(matches!(
            session.write_input(Vec::new()).await,
            Err(TerminalError::Closed)
        ));
    }

    #[tokio::test]
    async fn session_replays_output_emitted_before_listener_subscribes() {
        let backend = Arc::new(FakeBackend::default());
        let (tx, rx) = mpsc::channel(8);
        let rt = shared_runtime();
        let session = TerminalSession::from_open_backend(backend, rx, rt);

        tx.send(TerminalBackendEvent::Bytes(b"early\n".to_vec()))
            .await
            .unwrap();
        tx.send(TerminalBackendEvent::Exit(7)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        let bytes = Arc::new(Mutex::new(Vec::new()));
        let exits = Arc::new(Mutex::new(Vec::new()));
        session.subscribe_output(Box::new(CapturingListener {
            bytes: bytes.clone(),
            exits: exits.clone(),
        }));
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        assert_eq!(bytes.lock().unwrap().as_slice(), &[b"early\n".to_vec()]);
        assert_eq!(exits.lock().unwrap().as_slice(), &[7]);
    }

    #[tokio::test]
    async fn session_events_are_binary_ordered_and_session_scoped() {
        let backend = Arc::new(FakeBackend::default());
        let (tx, rx) = mpsc::channel(8);
        let rt = shared_runtime();
        let session = TerminalSession::from_open_backend_with_session_id(
            backend,
            rx,
            rt,
            Some("terminal-session-test".to_string()),
        );

        let events = Arc::new(Mutex::new(Vec::new()));
        session.subscribe_events(Box::new(CapturingEventListener {
            events: events.clone(),
        }));

        tx.send(TerminalBackendEvent::Bytes(b"\x1b[?1049h".to_vec()))
            .await
            .unwrap();
        tx.send(TerminalBackendEvent::Bytes(vec![0xff, 0x00, b'A']))
            .await
            .unwrap();
        tx.send(TerminalBackendEvent::Exit(23)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;

        let events = events.lock().unwrap().clone();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].session_id, "terminal-session-test");
        assert_eq!(events[0].sequence, 0);
        assert_eq!(events[0].kind, TerminalEventKind::Output);
        assert_eq!(events[0].data, b"\x1b[?1049h".to_vec());
        assert_eq!(events[1].session_id, "terminal-session-test");
        assert_eq!(events[1].sequence, 1);
        assert_eq!(events[1].kind, TerminalEventKind::Output);
        assert_eq!(events[1].data, vec![0xff, 0x00, b'A']);
        assert_eq!(events[2].session_id, "terminal-session-test");
        assert_eq!(events[2].sequence, 2);
        assert_eq!(events[2].kind, TerminalEventKind::Exit);
        assert_eq!(events[2].exit_code, Some(23));
    }

    #[test]
    fn rejects_zero_sized_terminal() {
        let error = validate_size(TerminalSize { cols: 0, rows: 24 }).unwrap_err();
        assert!(matches!(error, TerminalError::InvalidSize { .. }));
    }

    #[test]
    fn generated_terminal_session_ids_are_namespaced_from_chat_threads() {
        let id = new_terminal_session_id();
        assert!(id.starts_with("terminal-"));
        assert_ne!(id, "thread-1");
    }

    #[tokio::test]
    #[ignore = "requires a live alleycat daemon; set LITTER_TERMINAL_LIVE_ALLEYCAT_PAIR"]
    async fn live_remote_alleycat_terminal_round_trips_shell_io() {
        let pair_json = match std::env::var("LITTER_TERMINAL_LIVE_ALLEYCAT_PAIR") {
            Ok(value) if !value.trim().is_empty() => value,
            _ => {
                eprintln!("skipping: LITTER_TERMINAL_LIVE_ALLEYCAT_PAIR is not set");
                return;
            }
        };
        let pair = crate::alleycat::parse_pair_payload(&pair_json).expect("parse pair payload");
        let session = TerminalSession::open(
            TerminalBackendKind::RemoteAlleycat {
                node_id: pair.node_id,
                token: pair.token,
                relay: pair.relay,
                shell: Some("/bin/sh".to_string()),
            },
            TerminalSize { cols: 77, rows: 31 },
        )
        .await
        .expect("open remote shell terminal");

        enum LiveEvent {
            Bytes(Vec<u8>),
            Exit(i32),
        }

        struct LiveListener {
            tx: tokio::sync::mpsc::UnboundedSender<LiveEvent>,
        }

        impl TerminalOutputListener for LiveListener {
            fn on_bytes(&self, data: Vec<u8>) {
                let _ = self.tx.send(LiveEvent::Bytes(data));
            }

            fn on_exit(&self, code: i32) {
                let _ = self.tx.send(LiveEvent::Exit(code));
            }
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        session.subscribe_output(Box::new(LiveListener { tx }));
        session
            .write_input(b"printf 'remote-mobile-ready\n'; stty size; exit 0\n".to_vec())
            .await
            .expect("write shell input");

        let mut output = Vec::new();
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        let exit_code = loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                panic!(
                    "timed out waiting for remote shell output; got {:?}",
                    String::from_utf8_lossy(&output)
                );
            }
            match tokio::time::timeout(remaining, rx.recv()).await {
                Ok(Some(LiveEvent::Bytes(bytes))) => output.extend(bytes),
                Ok(Some(LiveEvent::Exit(code))) => {
                    break code;
                }
                Ok(None) => panic!("remote terminal listener closed"),
                Err(_) => panic!(
                    "timed out waiting for remote shell output; got {:?}",
                    String::from_utf8_lossy(&output)
                ),
            }
            if String::from_utf8_lossy(&output).contains("remote-mobile-ready")
                && String::from_utf8_lossy(&output).contains("31 77")
            {
                // Keep waiting for the shell/exit notification so the test
                // proves the full lifecycle, not just stdout delivery.
            }
        };

        let output = String::from_utf8_lossy(&output);
        assert!(
            output.contains("remote-mobile-ready"),
            "expected command output, got {output:?}"
        );
        assert!(
            output.contains("31 77"),
            "expected stty size from remote PTY, got {output:?}"
        );
        assert_eq!(exit_code, 0);
        session.close_session().await.ok();
    }
}
