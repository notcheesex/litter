import XCTest
@testable import Litter

@MainActor
final class DroidTerminalSupportTests: XCTestCase {
    func testLabelsDroidJsonAndTuiModesSeparatelyFromSharedCapabilities() {
        let capabilities = makeCapabilities()

        XCTAssertEqual(
            DroidTerminalSupport.modeLabel(
                for: makeAgent(name: "droid", wire: .jsonl),
                capabilities: capabilities
            ),
            DroidTerminalSupport.jsonModeLabel
        )
        XCTAssertEqual(
            DroidTerminalSupport.modeLabel(
                for: makeAgent(name: "droid-pty", wire: .terminal),
                capabilities: capabilities
            ),
            DroidTerminalSupport.terminalModeLabel
        )
        XCTAssertNil(
            DroidTerminalSupport.modeLabel(
                for: makeAgent(name: "codex", wire: .websocket),
                capabilities: capabilities
            )
        )
    }

    func testBuildsDroidTerminalTargetOnlyWhenPtyCapabilityIsAvailable() {
        let saved = makeSavedServer()
        let target = DroidTerminalSupport.target(
            for: saved,
            token: "pair-token",
            capabilities: makeCapabilities()
        )

        XCTAssertEqual(target?.serverId, saved.id)
        XCTAssertEqual(target?.nodeId, "node-1")
        XCTAssertEqual(target?.agentName, "droid-pty")
        XCTAssertEqual(target?.label, "Droid TUI")

        XCTAssertNil(
            DroidTerminalSupport.target(
                for: saved,
                token: "pair-token",
                capabilities: makeCapabilities(ptyAvailable: false)
            )
        )
    }

    private func makeCapabilities(ptyAvailable: Bool = true) -> AppDroidModeCapabilities {
        AppDroidModeCapabilities(
            supportedModes: ptyAvailable ? [.jsonNative, .ptyTerminal] : [.jsonNative],
            jsonNative: AppDroidJsonNativeCapability(
                available: true,
                agentName: "droid",
                wire: .jsonl,
                unavailableReason: nil
            ),
            ptyTerminal: AppDroidPtyCapability(
                available: ptyAvailable,
                unavailableReasonKind: nil,
                unavailableReason: nil,
                agentName: "droid-pty",
                displayName: "Droid Terminal",
                launchAgent: "droid-pty",
                transport: .droidPty,
                protocolVersion: 1,
                features: ["output", "input", "resize", "close"],
                label: "Droid TUI"
            )
        )
    }

    private func makeAgent(
        name: String,
        wire: AppAlleycatAgentWire
    ) -> AppAlleycatAgentInfo {
        AppAlleycatAgentInfo(
            name: name,
            displayName: name,
            runtimeKind: name == "droid" ? "droid" : nil,
            wire: wire,
            available: true,
            unavailableReason: nil,
            presentation: nil,
            capabilities: nil
        )
    }

    private func makeSavedServer() -> SavedServer {
        SavedServer(
            id: "alleycat:node-1",
            name: "Dev host",
            hostname: "node-1",
            port: 0,
            codexPorts: [],
            sshPort: nil,
            source: .manual,
            hasCodexServer: true,
            wakeMAC: nil,
            preferredConnectionMode: nil,
            preferredCodexPort: nil,
            sshPortForwardingEnabled: nil,
            websocketURL: nil,
            rememberedByUser: true,
            alleycatNodeId: "node-1",
            alleycatRelay: "https://relay.example"
        )
    }
}
