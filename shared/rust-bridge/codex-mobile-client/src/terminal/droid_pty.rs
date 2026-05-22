use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{Mutex, mpsc};

use super::backend::{OpenBackendResult, TerminalBackend, TerminalBackendEvent};
use super::session::{TerminalError, TerminalSize};

const TERMINAL_PROTOCOL_VERSION: u16 = 1;
const MAGIC: &[u8; 4] = b"DPTY";
const MAX_FRAME_BYTES: usize = 1024 * 1024;

pub(crate) async fn open(
    node_id: String,
    token: String,
    relay: Option<String>,
    agent: Option<String>,
    cwd: Option<String>,
    size: TerminalSize,
) -> Result<OpenBackendResult, TerminalError> {
    let endpoint = crate::ffi::shared::shared_mobile_client()
        .alleycat_endpoint()
        .await
        .map_err(|error| TerminalError::Backend {
            detail: format!("binding alleycat endpoint: {error}"),
        })?;
    let params = crate::alleycat::ParsedPairPayload {
        version: crate::alleycat::ALLEYCAT_PROTOCOL_VERSION,
        node_id,
        token,
        relay,
        host_name: None,
    };
    let agent = agent.unwrap_or_else(|| "droid-pty".to_string());
    let (stream, session) =
        crate::alleycat::connect_terminal_agent_stream(&endpoint, params, agent)
            .await
            .map_err(|error| TerminalError::Unavailable {
                detail: sanitize_terminal_detail(format!(
                    "connecting Droid PTY terminal bridge: {error}"
                )),
            })?;
    let (backend, output_rx) = open_over_stream(stream, Some(session), cwd, size).await?;
    Ok((backend, output_rx))
}

async fn open_over_stream<S>(
    stream: S,
    session: Option<Arc<crate::alleycat::AlleycatSession>>,
    cwd: Option<String>,
    size: TerminalSize,
) -> Result<OpenBackendResult, TerminalError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    write_frame(
        &mut writer,
        &TerminalFrame {
            kind: TerminalFrameKind::Hello,
            payload: encode_json(&HelloPayload {
                min_version: TERMINAL_PROTOCOL_VERSION,
                max_version: TERMINAL_PROTOCOL_VERSION,
                features: terminal_features(),
            })?,
        },
    )
    .await?;
    let frame = read_frame(&mut reader).await?;
    if frame.kind != TerminalFrameKind::Hello {
        return Err(TerminalError::Protocol {
            detail: "Droid PTY terminal peer did not negotiate terminal protocol".to_string(),
        });
    }
    let hello: HelloPayload = decode_json(&frame.payload)?;
    if hello.min_version > TERMINAL_PROTOCOL_VERSION
        || hello.max_version < TERMINAL_PROTOCOL_VERSION
    {
        return Err(TerminalError::Protocol {
            detail: "Droid PTY terminal protocol version is unsupported".to_string(),
        });
    }
    write_frame(
        &mut writer,
        &TerminalFrame {
            kind: TerminalFrameKind::Start,
            payload: encode_json(&StartPayload {
                cols: size.cols,
                rows: size.rows,
                cwd,
            })?,
        },
    )
    .await?;

    let (output_tx, output_rx) = mpsc::channel(256);
    tokio::spawn(read_output_loop(reader, output_tx));
    let backend = Arc::new(RemoteDroidPtyBackend {
        writer: Mutex::new(writer),
        session,
        closed: AtomicBool::new(false),
    });
    Ok((backend, output_rx))
}

struct RemoteDroidPtyBackend<W> {
    writer: Mutex<W>,
    session: Option<Arc<crate::alleycat::AlleycatSession>>,
    closed: AtomicBool,
}

#[async_trait]
impl<W> TerminalBackend for RemoteDroidPtyBackend<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn write(&self, data: &[u8]) -> Result<(), TerminalError> {
        self.send(TerminalFrameKind::Input, data.to_vec()).await
    }

    async fn resize(&self, size: TerminalSize) -> Result<(), TerminalError> {
        let payload = resize_payload(size);
        self.send(TerminalFrameKind::Resize, payload.to_vec()).await
    }

    async fn close(&self) -> Result<(), TerminalError> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let result = self.send(TerminalFrameKind::Close, Vec::new()).await;
        if let Some(session) = &self.session {
            session.close();
        }
        result
    }
}

impl<W> RemoteDroidPtyBackend<W>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    async fn send(&self, kind: TerminalFrameKind, payload: Vec<u8>) -> Result<(), TerminalError> {
        if self.closed.load(Ordering::SeqCst) && kind != TerminalFrameKind::Close {
            return Err(TerminalError::Closed);
        }
        let mut writer = self.writer.lock().await;
        write_frame(&mut *writer, &TerminalFrame { kind, payload }).await
    }
}

async fn read_output_loop<R>(mut reader: ReadHalf<R>, output_tx: mpsc::Sender<TerminalBackendEvent>)
where
    R: AsyncRead + Unpin,
{
    loop {
        let frame = match read_frame(&mut reader).await {
            Ok(frame) => frame,
            Err(_) => {
                let _ = output_tx.send(TerminalBackendEvent::Exit(-1)).await;
                break;
            }
        };
        match frame.kind {
            TerminalFrameKind::Output => {
                if output_tx
                    .send(TerminalBackendEvent::Bytes(frame.payload))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            TerminalFrameKind::Exit => {
                let code = decode_exit_payload(&frame.payload).unwrap_or(-1);
                let _ = output_tx.send(TerminalBackendEvent::Exit(code)).await;
                break;
            }
            TerminalFrameKind::Error => {
                let _ = output_tx.send(TerminalBackendEvent::Exit(-1)).await;
                break;
            }
            _ => {}
        }
    }
}

fn terminal_features() -> Vec<String> {
    ["hello", "start", "output", "input", "resize", "close", "exit", "error"]
        .iter()
        .map(|feature| (*feature).to_owned())
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum TerminalFrameKind {
    Hello = 1,
    Start = 2,
    Output = 3,
    Input = 4,
    Resize = 5,
    Close = 6,
    Exit = 7,
    Error = 8,
}

impl TerminalFrameKind {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Hello),
            2 => Some(Self::Start),
            3 => Some(Self::Output),
            4 => Some(Self::Input),
            5 => Some(Self::Resize),
            6 => Some(Self::Close),
            7 => Some(Self::Exit),
            8 => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalFrame {
    kind: TerminalFrameKind,
    payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct HelloPayload {
    min_version: u16,
    max_version: u16,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct StartPayload {
    cols: u16,
    rows: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ErrorPayload {
    code: String,
    message: String,
}

fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, TerminalError> {
    serde_json::to_vec(value).map_err(|error| TerminalError::Protocol {
        detail: format!("encoding Droid PTY terminal payload: {error}"),
    })
}

fn decode_json<T: serde::de::DeserializeOwned>(payload: &[u8]) -> Result<T, TerminalError> {
    serde_json::from_slice(payload).map_err(|error| TerminalError::Protocol {
        detail: format!("decoding Droid PTY terminal payload: {error}"),
    })
}

fn transport_error(action: &str, error: impl std::fmt::Display) -> TerminalError {
    TerminalError::TransportDisconnected {
        detail: sanitize_terminal_detail(format!("{action}: {error}")),
    }
}

fn protocol_error(detail: impl Into<String>) -> TerminalError {
    TerminalError::Protocol {
        detail: sanitize_terminal_detail(detail.into()),
    }
}

fn sanitize_terminal_detail(detail: String) -> String {
    let lower = detail.to_ascii_lowercase();
    let sensitive_markers = [
        "authorization",
        "bearer ",
        "token",
        "secret",
        "api_key",
        "apikey",
        "password",
        "credential",
    ];
    if sensitive_markers
        .iter()
        .any(|marker| lower.contains(marker))
    {
        "[redacted Droid PTY terminal error containing sensitive data]".to_string()
    } else {
        detail
    }
}

async fn read_frame<R>(reader: &mut R) -> Result<TerminalFrame, TerminalError>
where
    R: AsyncRead + Unpin,
{
    let mut magic = [0u8; 4];
    reader
        .read_exact(&mut magic)
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal magic", error))?;
    if &magic != MAGIC {
        return Err(protocol_error(format!(
            "unsupported Droid PTY terminal peer: invalid magic {:02x}{:02x}{:02x}{:02x}",
            magic[0], magic[1], magic[2], magic[3]
        )));
    }
    let version = reader
        .read_u16()
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal version", error))?;
    if version != TERMINAL_PROTOCOL_VERSION {
        return Err(protocol_error(format!(
            "unsupported Droid PTY terminal protocol version {version}"
        )));
    }
    let kind = reader
        .read_u8()
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal frame kind", error))?;
    let _flags = reader
        .read_u8()
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal frame flags", error))?;
    let len = reader
        .read_u32()
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal frame length", error))?;
    if len as usize > MAX_FRAME_BYTES {
        return Err(protocol_error(format!(
            "Droid PTY terminal frame too large: {len} bytes"
        )));
    }
    let kind = TerminalFrameKind::from_u8(kind)
        .ok_or_else(|| protocol_error("unknown Droid PTY terminal frame kind"))?;
    let mut payload = vec![0u8; len as usize];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|error| transport_error("reading Droid PTY terminal frame payload", error))?;
    Ok(TerminalFrame { kind, payload })
}

async fn write_frame<W>(writer: &mut W, frame: &TerminalFrame) -> Result<(), TerminalError>
where
    W: AsyncWrite + Unpin,
{
    if frame.payload.len() > MAX_FRAME_BYTES {
        return Err(protocol_error(format!(
            "Droid PTY terminal frame too large: {} bytes",
            frame.payload.len()
        )));
    }
    writer
        .write_all(MAGIC)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal magic", error))?;
    writer
        .write_u16(TERMINAL_PROTOCOL_VERSION)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal version", error))?;
    writer
        .write_u8(frame.kind as u8)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal frame kind", error))?;
    writer
        .write_u8(0)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal frame flags", error))?;
    writer
        .write_u32(frame.payload.len() as u32)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal frame length", error))?;
    writer
        .write_all(&frame.payload)
        .await
        .map_err(|error| transport_error("writing Droid PTY terminal frame payload", error))?;
    writer
        .flush()
        .await
        .map_err(|error| transport_error("flushing Droid PTY terminal frame", error))
}

fn resize_payload(size: TerminalSize) -> [u8; 4] {
    let mut payload = [0u8; 4];
    payload[..2].copy_from_slice(&size.cols.to_be_bytes());
    payload[2..].copy_from_slice(&size.rows.to_be_bytes());
    payload
}

fn decode_exit_payload(payload: &[u8]) -> Option<i32> {
    (payload.len() == 4)
        .then(|| i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn fake_peer_round_trips_binary_output_input_and_resize() {
        let (client, server) = duplex(64 * 1024);
        let server_task = tokio::spawn(fake_terminal_peer(server));
        let (backend, mut output_rx) = open_over_stream(
            client,
            None,
            Some("/tmp/droid".to_string()),
            TerminalSize { cols: 101, rows: 37 },
        )
        .await
        .expect("open fake terminal");

        let output = output_rx.recv().await.expect("output");
        assert_eq!(
            output,
            TerminalBackendEvent::Bytes(b"\x1b[?1049hhello-\xff\r\n".to_vec())
        );
        backend
            .write(b"/missions\x1b[A\t\x03\x04\n")
            .await
            .expect("write input");
        backend
            .resize(TerminalSize { cols: 132, rows: 43 })
            .await
            .expect("resize");
        backend.close().await.expect("close");

        let observed = server_task.await.expect("server task");
        assert_eq!(observed.input, b"/missions\x1b[A\t\x03\x04\n");
        assert_eq!(observed.resize, TerminalSize { cols: 132, rows: 43 });
        assert_eq!(observed.start.cols, 101);
        assert_eq!(observed.start.rows, 37);
        assert_eq!(observed.start.cwd.as_deref(), Some("/tmp/droid"));
    }

    #[tokio::test]
    async fn rejects_old_jsonl_peer_without_leaking_secret() {
        let (client, mut server) = duplex(1024);
        let server_task = tokio::spawn(async move {
            server
                .write_all(br#"{"jsonrpc":"2.0","token":"terminal-secret-fixture"}"#)
                .await
                .unwrap();
        });
        let error = match open_over_stream(client, None, None, TerminalSize { cols: 80, rows: 24 })
            .await
        {
            Ok(_) => panic!("old JSONL peer should be rejected"),
            Err(error) => error.to_string(),
        };
        server_task.await.unwrap();
        assert!(error.contains("unsupported Droid PTY terminal peer"));
        assert!(!error.contains("terminal-secret-fixture"));
    }

    #[derive(Debug)]
    struct FakeObserved {
        start: StartPayload,
        input: Vec<u8>,
        resize: TerminalSize,
    }

    async fn fake_terminal_peer<S>(stream: S) -> FakeObserved
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let (mut reader, mut writer) = tokio::io::split(stream);
        let hello = read_frame(&mut reader).await.unwrap();
        assert_eq!(hello.kind, TerminalFrameKind::Hello);
        write_frame(
            &mut writer,
            &TerminalFrame {
                kind: TerminalFrameKind::Hello,
                payload: encode_json(&HelloPayload {
                    min_version: TERMINAL_PROTOCOL_VERSION,
                    max_version: TERMINAL_PROTOCOL_VERSION,
                    features: terminal_features(),
                })
                .unwrap(),
            },
        )
        .await
        .unwrap();
        let start = read_frame(&mut reader).await.unwrap();
        assert_eq!(start.kind, TerminalFrameKind::Start);
        let start: StartPayload = decode_json(&start.payload).unwrap();
        write_frame(
            &mut writer,
            &TerminalFrame {
                kind: TerminalFrameKind::Output,
                payload: b"\x1b[?1049hhello-\xff\r\n".to_vec(),
            },
        )
        .await
        .unwrap();
        let input = read_frame(&mut reader).await.unwrap();
        assert_eq!(input.kind, TerminalFrameKind::Input);
        let resize = read_frame(&mut reader).await.unwrap();
        assert_eq!(resize.kind, TerminalFrameKind::Resize);
        let resize = decode_resize(&resize.payload);
        let close = read_frame(&mut reader).await.unwrap();
        assert_eq!(close.kind, TerminalFrameKind::Close);
        FakeObserved {
            start,
            input: input.payload,
            resize,
        }
    }

    fn decode_resize(payload: &[u8]) -> TerminalSize {
        TerminalSize {
            cols: u16::from_be_bytes([payload[0], payload[1]]),
            rows: u16::from_be_bytes([payload[2], payload[3]]),
        }
    }
}
