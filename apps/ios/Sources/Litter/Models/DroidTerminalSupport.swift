import Foundation

struct DroidTerminalTarget: Equatable, Identifiable {
    var id: String { "\(nodeId):\(agentName ?? "default")" }

    let serverId: String
    let displayName: String
    let nodeId: String
    let relay: String?
    let token: String
    let agentName: String?
    let label: String
}

enum DroidTerminalSupport {
    static let defaultLabel = "Droid TUI"
    static let jsonModeLabel = "Droid native chat"
    static let terminalModeLabel = "Droid TUI terminal"

    static func modeLabel(
        for agent: AppAlleycatAgentInfo,
        capabilities: AppDroidModeCapabilities
    ) -> String? {
        let name = normalized(agent.name)
        guard !name.isEmpty else { return nil }
        let pty = capabilities.ptyTerminal
        let json = capabilities.jsonNative
        if normalized(pty.agentName) == name || normalized(pty.launchAgent) == name {
            return terminalModeLabel
        }
        if normalized(json.agentName) == name {
            return jsonModeLabel
        }
        return nil
    }

    static func target(
        for saved: SavedServer,
        token: String?,
        capabilities: AppDroidModeCapabilities
    ) -> DroidTerminalTarget? {
        let nodeId = normalized(saved.alleycatNodeId)
        let resolvedToken = normalized(token)
        guard !nodeId.isEmpty, !resolvedToken.isEmpty else { return nil }
        let pty = capabilities.ptyTerminal
        guard pty.available else { return nil }
        let label = normalized(pty.label).isEmpty ? defaultLabel : normalized(pty.label)
        let displayName = normalized(saved.name).isEmpty
            ? "Alleycat \(shortNodeId(nodeId))"
            : normalized(saved.name)
        return DroidTerminalTarget(
            serverId: saved.id,
            displayName: displayName,
            nodeId: nodeId,
            relay: normalizedOptional(saved.alleycatRelay),
            token: resolvedToken,
            agentName: normalizedOptional(pty.launchAgent) ?? normalizedOptional(pty.agentName),
            label: label
        )
    }

    @MainActor
    static func discoverTargets(
        appModel: AppModel = AppModel.shared,
        alleycat: RustAlleycatBridge = .shared
    ) async -> [DroidTerminalTarget] {
        var targets: [DroidTerminalTarget] = []
        for saved in SavedServerStore.rememberedServers() {
            let nodeId = normalized(saved.alleycatNodeId)
            guard !nodeId.isEmpty else { continue }
            guard let token = try? AlleycatCredentialStore.shared.loadToken(nodeId: nodeId),
                  !normalized(token).isEmpty
            else {
                continue
            }
            let params = AppAlleycatPairPayload(
                v: 1,
                nodeId: nodeId,
                token: token,
                relay: normalizedOptional(saved.alleycatRelay),
                hostName: normalizedOptional(saved.name)
            )
            do {
                let agents = try await appModel.serverBridge.listAlleycatAgents(params: params)
                let capabilities = alleycat.droidModeCapabilities(agents: agents)
                if let target = target(for: saved, token: token, capabilities: capabilities) {
                    targets.append(target)
                }
            } catch {
                continue
            }
        }
        return targets
    }

    static func preferredTarget(
        in targets: [DroidTerminalTarget],
        preferredServerId: String?
    ) -> DroidTerminalTarget? {
        if let preferred = normalizedOptional(preferredServerId),
           let match = targets.first(where: { $0.serverId == preferred }) {
            return match
        }
        return targets.first
    }

    static func backendId(nodeId: String, agentName: String?) -> String {
        let agent = normalizedOptional(agentName) ?? "default"
        return "droid-pty-\(nodeId)-\(agent)"
    }

    private static func normalized(_ value: String?) -> String {
        value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    private static func normalizedOptional(_ value: String?) -> String? {
        let trimmed = normalized(value)
        return trimmed.isEmpty ? nil : trimmed
    }

    private static func shortNodeId(_ raw: String) -> String {
        raw.count <= 16 ? raw : "\(raw.prefix(8))...\(raw.suffix(8))"
    }
}
