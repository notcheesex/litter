use crate::alleycat::{
    AgentCapabilities, AgentInfo, AgentPresentation, AgentTerminalCapability,
    AgentTerminalTransport, AgentWire, AlleycatError, ParsedPairPayload,
};
use crate::ffi::ClientError;

#[derive(uniffi::Object)]
pub struct AlleycatBridge;

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAlleycatPairPayload {
    pub v: u32,
    pub node_id: String,
    pub token: String,
    pub relay: Option<String>,
    pub host_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppAlleycatAgentWire {
    Websocket,
    Jsonl,
    Terminal,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAlleycatAgentInfo {
    pub name: String,
    pub display_name: String,
    pub runtime_kind: Option<crate::types::AgentRuntimeKind>,
    pub wire: AppAlleycatAgentWire,
    pub available: bool,
    /// Host-supplied unavailable reason. This is presentation text only:
    /// PTY availability and mode routing must come from typed wire and
    /// capability fields below, never labels or this string.
    pub unavailable_reason: Option<String>,
    /// UI hints sourced from the alleycat host: title, beta badge,
    /// sort order, aliases. Absent on legacy hosts — clients fall back
    /// to generic rendering keyed off `name` / `display_name`.
    pub presentation: Option<AppAgentPresentation>,
    /// Behavioral capability flags surfaced to platform UI so it can
    /// branch without hardcoding agent names. Absent on legacy hosts.
    pub capabilities: Option<AppAgentCapabilities>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAgentPresentation {
    pub title: Option<String>,
    pub is_beta: bool,
    pub sort_order: i32,
    pub description: Option<String>,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAgentCapabilities {
    pub locks_reasoning_effort_after_activity: bool,
    pub visible_modes: Option<Vec<String>>,
    pub supports_ssh_bridge: bool,
    pub uses_direct_codex_port: bool,
    pub supports_thread_permission_overrides: bool,
    pub reports_effective_thread_permissions: bool,
    pub terminal: Option<AppAgentTerminalCapability>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAgentTerminalCapability {
    pub transport: AppAgentTerminalTransport,
    pub protocol_version: u32,
    pub features: Vec<String>,
    pub launch_agent: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppAgentTerminalTransport {
    DroidPty,
    TerminalPty,
}

const DROID_PTY_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppDroidModeKind {
    JsonNative,
    PtyTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppDroidPtyUnavailableReason {
    HostDidNotAdvertiseTerminal,
    AgentUnavailable,
    MissingTerminalCapability,
    UnsupportedTransport,
    UnsupportedProtocol,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppDroidJsonNativeCapability {
    pub available: bool,
    pub agent_name: Option<String>,
    pub wire: Option<AppAlleycatAgentWire>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppDroidPtyCapability {
    pub available: bool,
    pub unavailable_reason_kind: Option<AppDroidPtyUnavailableReason>,
    pub unavailable_reason: Option<String>,
    pub agent_name: Option<String>,
    pub display_name: Option<String>,
    pub launch_agent: Option<String>,
    pub transport: Option<AppAgentTerminalTransport>,
    pub protocol_version: Option<u32>,
    pub features: Vec<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppDroidModeCapabilities {
    pub supported_modes: Vec<AppDroidModeKind>,
    pub json_native: AppDroidJsonNativeCapability,
    pub pty_terminal: AppDroidPtyCapability,
}

impl From<AgentPresentation> for AppAgentPresentation {
    fn from(value: AgentPresentation) -> Self {
        AppAgentPresentation {
            title: value.title,
            is_beta: value.is_beta,
            sort_order: value.sort_order,
            description: value.description,
            aliases: value.aliases,
        }
    }
}

impl From<AgentCapabilities> for AppAgentCapabilities {
    fn from(value: AgentCapabilities) -> Self {
        AppAgentCapabilities {
            locks_reasoning_effort_after_activity: value.locks_reasoning_effort_after_activity,
            visible_modes: value.visible_modes,
            supports_ssh_bridge: value.supports_ssh_bridge,
            uses_direct_codex_port: value.uses_direct_codex_port,
            supports_thread_permission_overrides: value.supports_thread_permission_overrides,
            reports_effective_thread_permissions: value.reports_effective_thread_permissions,
            terminal: value.terminal.map(Into::into),
        }
    }
}

impl From<AgentTerminalCapability> for AppAgentTerminalCapability {
    fn from(value: AgentTerminalCapability) -> Self {
        Self {
            transport: value.transport.into(),
            protocol_version: value.protocol_version,
            features: value.features,
            launch_agent: value.launch_agent,
            label: value.label,
        }
    }
}

impl From<AgentTerminalTransport> for AppAgentTerminalTransport {
    fn from(value: AgentTerminalTransport) -> Self {
        match value {
            AgentTerminalTransport::DroidPty => Self::DroidPty,
            AgentTerminalTransport::TerminalPty => Self::TerminalPty,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct AppAlleycatConnectResult {
    pub server_id: String,
    pub node_id: String,
    pub agent_name: String,
}

#[uniffi::export]
impl AlleycatBridge {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self
    }

    pub fn parse_pair_payload(&self, json: String) -> Result<AppAlleycatPairPayload, ClientError> {
        let parsed = crate::alleycat::parse_pair_payload(&json).map_err(map_alleycat_error)?;
        Ok(parsed.into())
    }

    pub fn droid_mode_capabilities(
        &self,
        agents: Vec<AppAlleycatAgentInfo>,
    ) -> AppDroidModeCapabilities {
        droid_mode_capabilities_from_agents(&agents)
    }
}

pub(crate) fn map_alleycat_error(error: AlleycatError) -> ClientError {
    match error {
        AlleycatError::InvalidPayload(message) => ClientError::InvalidParams(message),
        AlleycatError::ProtocolMismatch { payload, client } => ClientError::InvalidParams(format!(
            "alleycat protocol mismatch: payload={payload} client={client}"
        )),
        AlleycatError::Transport(message) => ClientError::Transport(message),
    }
}

impl From<AppAlleycatPairPayload> for ParsedPairPayload {
    fn from(value: AppAlleycatPairPayload) -> Self {
        ParsedPairPayload {
            version: value.v,
            node_id: value.node_id,
            token: value.token,
            relay: value.relay,
            host_name: value.host_name,
        }
    }
}

impl From<ParsedPairPayload> for AppAlleycatPairPayload {
    fn from(value: ParsedPairPayload) -> Self {
        AppAlleycatPairPayload {
            v: value.version,
            node_id: value.node_id,
            token: value.token,
            relay: value.relay,
            host_name: value.host_name,
        }
    }
}

impl From<AppAlleycatAgentWire> for AgentWire {
    fn from(value: AppAlleycatAgentWire) -> Self {
        match value {
            AppAlleycatAgentWire::Websocket => Self::Websocket,
            AppAlleycatAgentWire::Jsonl => Self::Jsonl,
            AppAlleycatAgentWire::Terminal => Self::Terminal,
        }
    }
}

impl From<AgentWire> for AppAlleycatAgentWire {
    fn from(value: AgentWire) -> Self {
        match value {
            AgentWire::Websocket => Self::Websocket,
            AgentWire::Jsonl => Self::Jsonl,
            AgentWire::Terminal => Self::Terminal,
        }
    }
}

impl From<AgentInfo> for AppAlleycatAgentInfo {
    fn from(value: AgentInfo) -> Self {
        let runtime_kind = crate::alleycat::agent_runtime_kind(&value.name, &value.display_name);
        AppAlleycatAgentInfo {
            name: value.name,
            display_name: value.display_name,
            runtime_kind,
            wire: value.wire.into(),
            available: value.available,
            unavailable_reason: value.unavailable_reason,
            presentation: value.presentation.map(Into::into),
            capabilities: value.capabilities.map(Into::into),
        }
    }
}

fn droid_mode_capabilities_from_agents(agents: &[AppAlleycatAgentInfo]) -> AppDroidModeCapabilities {
    let json_native = droid_json_native_capability(agents);
    let pty_terminal = droid_pty_capability(agents);
    let mut supported_modes = Vec::new();
    if json_native.available {
        supported_modes.push(AppDroidModeKind::JsonNative);
    }
    if pty_terminal.available {
        supported_modes.push(AppDroidModeKind::PtyTerminal);
    }
    AppDroidModeCapabilities {
        supported_modes,
        json_native,
        pty_terminal,
    }
}

fn droid_json_native_capability(agents: &[AppAlleycatAgentInfo]) -> AppDroidJsonNativeCapability {
    let candidate = agents.iter().find(|agent| {
        agent.runtime_kind.as_deref() == Some("droid")
            && !matches!(agent.wire, AppAlleycatAgentWire::Terminal)
    });
    let available = candidate.is_some_and(|agent| agent.available);
    AppDroidJsonNativeCapability {
        available,
        agent_name: candidate.map(|agent| agent.name.clone()),
        wire: candidate.map(|agent| agent.wire.clone()),
        unavailable_reason: candidate.and_then(|agent| agent.unavailable_reason.clone()),
    }
}

fn droid_pty_capability(agents: &[AppAlleycatAgentInfo]) -> AppDroidPtyCapability {
    let candidate = agents
        .iter()
        .find(|agent| {
            matches!(agent.wire, AppAlleycatAgentWire::Terminal)
                && terminal_capability(agent).is_some_and(|terminal| {
                    matches!(terminal.transport, AppAgentTerminalTransport::DroidPty)
                })
        })
        .or_else(|| {
            agents.iter().find(|agent| {
                matches!(agent.wire, AppAlleycatAgentWire::Terminal)
                    || agent.runtime_kind.as_deref() == Some("droid-terminal")
                    || terminal_capability(agent).is_some()
            })
        });

    let Some(agent) = candidate else {
        return unavailable_pty_capability(
            AppDroidPtyUnavailableReason::HostDidNotAdvertiseTerminal,
            "host did not advertise a Droid PTY terminal capability".to_string(),
            None,
            None,
        );
    };

    if !agent.available {
        return unavailable_pty_capability(
            AppDroidPtyUnavailableReason::AgentUnavailable,
            agent
                .unavailable_reason
                .clone()
                .unwrap_or_else(|| "Droid PTY terminal agent is unavailable".to_string()),
            Some(agent),
            terminal_capability(agent),
        );
    }

    let Some(terminal) = terminal_capability(agent) else {
        return unavailable_pty_capability(
            AppDroidPtyUnavailableReason::MissingTerminalCapability,
            "terminal agent did not include typed terminal metadata".to_string(),
            Some(agent),
            None,
        );
    };

    if !matches!(terminal.transport, AppAgentTerminalTransport::DroidPty) {
        return unavailable_pty_capability(
            AppDroidPtyUnavailableReason::UnsupportedTransport,
            "terminal agent is not a Droid PTY transport".to_string(),
            Some(agent),
            Some(terminal),
        );
    }

    if terminal.protocol_version != DROID_PTY_PROTOCOL_VERSION {
        return unavailable_pty_capability(
            AppDroidPtyUnavailableReason::UnsupportedProtocol,
            format!(
                "Droid PTY protocol version {} is unsupported",
                terminal.protocol_version
            ),
            Some(agent),
            Some(terminal),
        );
    }

    AppDroidPtyCapability {
        available: true,
        unavailable_reason_kind: None,
        unavailable_reason: None,
        agent_name: Some(agent.name.clone()),
        display_name: Some(agent.display_name.clone()),
        launch_agent: terminal
            .launch_agent
            .clone()
            .or_else(|| Some(agent.name.clone())),
        transport: Some(terminal.transport.clone()),
        protocol_version: Some(terminal.protocol_version),
        features: terminal.features.clone(),
        label: terminal.label.clone(),
    }
}

fn unavailable_pty_capability(
    reason: AppDroidPtyUnavailableReason,
    message: String,
    agent: Option<&AppAlleycatAgentInfo>,
    terminal: Option<&AppAgentTerminalCapability>,
) -> AppDroidPtyCapability {
    AppDroidPtyCapability {
        available: false,
        unavailable_reason_kind: Some(reason),
        unavailable_reason: Some(message),
        agent_name: agent.map(|agent| agent.name.clone()),
        display_name: agent.map(|agent| agent.display_name.clone()),
        launch_agent: terminal.and_then(|terminal| terminal.launch_agent.clone()),
        transport: terminal.map(|terminal| terminal.transport.clone()),
        protocol_version: terminal.map(|terminal| terminal.protocol_version),
        features: terminal
            .map(|terminal| terminal.features.clone())
            .unwrap_or_default(),
        label: terminal.and_then(|terminal| terminal.label.clone()),
    }
}

fn terminal_capability(agent: &AppAlleycatAgentInfo) -> Option<&AppAgentTerminalCapability> {
    agent
        .capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.terminal.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_agent(
        name: &str,
        runtime_kind: Option<&str>,
        wire: AppAlleycatAgentWire,
        available: bool,
        terminal: Option<AppAgentTerminalCapability>,
    ) -> AppAlleycatAgentInfo {
        AppAlleycatAgentInfo {
            name: name.to_string(),
            display_name: name.to_string(),
            runtime_kind: runtime_kind.map(ToOwned::to_owned),
            wire,
            available,
            unavailable_reason: (!available).then(|| "droid binary missing".to_string()),
            presentation: None,
            capabilities: terminal.map(|terminal| AppAgentCapabilities {
                locks_reasoning_effort_after_activity: false,
                visible_modes: Some(vec!["terminal".to_string()]),
                supports_ssh_bridge: false,
                uses_direct_codex_port: false,
                supports_thread_permission_overrides: true,
                reports_effective_thread_permissions: true,
                terminal: Some(terminal),
            }),
        }
    }

    fn droid_terminal_capability() -> AppAgentTerminalCapability {
        AppAgentTerminalCapability {
            transport: AppAgentTerminalTransport::DroidPty,
            protocol_version: DROID_PTY_PROTOCOL_VERSION,
            features: vec![
                "output".to_string(),
                "input".to_string(),
                "resize".to_string(),
                "close".to_string(),
            ],
            launch_agent: Some("droid-pty".to_string()),
            label: Some("Droid TUI".to_string()),
        }
    }

    #[test]
    fn droid_mode_capability_distinguishes_json_native_from_pty() {
        let capabilities = droid_mode_capabilities_from_agents(&[
            app_agent(
                "droid",
                Some("droid"),
                AppAlleycatAgentWire::Jsonl,
                true,
                None,
            ),
            app_agent(
                "droid-pty",
                Some("droid-terminal"),
                AppAlleycatAgentWire::Terminal,
                true,
                Some(droid_terminal_capability()),
            ),
        ]);

        assert_eq!(
            capabilities.supported_modes,
            vec![AppDroidModeKind::JsonNative, AppDroidModeKind::PtyTerminal]
        );
        assert!(capabilities.json_native.available);
        assert_eq!(
            capabilities.json_native.wire,
            Some(AppAlleycatAgentWire::Jsonl)
        );
        assert!(capabilities.pty_terminal.available);
        assert_eq!(
            capabilities.pty_terminal.transport,
            Some(AppAgentTerminalTransport::DroidPty)
        );
        assert_eq!(
            capabilities.pty_terminal.launch_agent.as_deref(),
            Some("droid-pty")
        );
        assert!(capabilities
            .pty_terminal
            .features
            .iter()
            .any(|feature| feature == "resize"));
    }

    #[test]
    fn json_only_droid_reports_typed_missing_pty_reason() {
        let capabilities = droid_mode_capabilities_from_agents(&[app_agent(
            "droid",
            Some("droid"),
            AppAlleycatAgentWire::Jsonl,
            true,
            None,
        )]);

        assert!(capabilities.json_native.available);
        assert!(!capabilities.pty_terminal.available);
        assert_eq!(
            capabilities.pty_terminal.unavailable_reason_kind,
            Some(AppDroidPtyUnavailableReason::HostDidNotAdvertiseTerminal)
        );
        assert_eq!(
            capabilities.supported_modes,
            vec![AppDroidModeKind::JsonNative]
        );
    }

    #[test]
    fn unavailable_droid_pty_keeps_host_reason_without_inferring_from_label() {
        let capabilities = droid_mode_capabilities_from_agents(&[app_agent(
            "anything",
            Some("droid-terminal"),
            AppAlleycatAgentWire::Terminal,
            false,
            Some(droid_terminal_capability()),
        )]);

        assert!(!capabilities.pty_terminal.available);
        assert_eq!(
            capabilities.pty_terminal.unavailable_reason_kind,
            Some(AppDroidPtyUnavailableReason::AgentUnavailable)
        );
        assert_eq!(
            capabilities.pty_terminal.unavailable_reason.as_deref(),
            Some("droid binary missing")
        );
        assert!(capabilities.supported_modes.is_empty());
    }
}
