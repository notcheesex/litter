package com.litter.android.ui.terminal

import com.litter.android.state.SavedServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.codex_mobile_client.AppAlleycatAgentInfo
import uniffi.codex_mobile_client.AppAlleycatAgentWire
import uniffi.codex_mobile_client.AppAgentTerminalTransport
import uniffi.codex_mobile_client.AppDroidJsonNativeCapability
import uniffi.codex_mobile_client.AppDroidModeCapabilities
import uniffi.codex_mobile_client.AppDroidModeKind
import uniffi.codex_mobile_client.AppDroidPtyCapability

class DroidTerminalSupportTests {
    @Test
    fun `labels droid json and tui modes separately from shared capabilities`() {
        val modes = modes()

        assertEquals(
            DroidTerminalSupport.JSON_MODE_LABEL,
            DroidTerminalSupport.modeLabelForAgent(agent("droid", AppAlleycatAgentWire.JSONL), modes),
        )
        assertEquals(
            DroidTerminalSupport.TERMINAL_MODE_LABEL,
            DroidTerminalSupport.modeLabelForAgent(agent("droid-pty", AppAlleycatAgentWire.TERMINAL), modes),
        )
        assertNull(
            DroidTerminalSupport.modeLabelForAgent(agent("codex", AppAlleycatAgentWire.WEBSOCKET), modes),
        )
    }

    @Test
    fun `builds droid terminal target only when pty capability is available`() {
        val saved = savedServer()
        val target = DroidTerminalSupport.targetForSavedServer(
            saved = saved,
            token = "pair-token",
            capabilities = modes(),
        )

        assertNotNull(target)
        assertEquals(saved.id, target!!.serverId)
        assertEquals("node-1", target.nodeId)
        assertEquals("droid-pty", target.agentName)
        assertEquals("Droid TUI", target.label)

        val unavailable = modes(ptyAvailable = false)
        assertNull(
            DroidTerminalSupport.targetForSavedServer(
                saved = saved,
                token = "pair-token",
                capabilities = unavailable,
            ),
        )
    }

    @Test
    fun `pairing defaults to Droid JSON anchor when Droid modes are advertised`() {
        val selected = DroidTerminalSupport.defaultSelectedAgentNames(
            agents = listOf(
                agent("codex", AppAlleycatAgentWire.WEBSOCKET),
                agent("droid", AppAlleycatAgentWire.JSONL),
                agent("droid-pty", AppAlleycatAgentWire.TERMINAL),
            ),
            capabilities = modes(),
        )

        assertEquals(setOf("droid"), selected)
    }

    @Test
    fun `pairing selection excludes terminal-only Droid PTY agents`() {
        assertEquals(
            false,
            DroidTerminalSupport.isPairingConnectableAgent(
                agent("droid-pty", AppAlleycatAgentWire.TERMINAL),
            ),
        )
        assertEquals(
            true,
            DroidTerminalSupport.isPairingConnectableAgent(
                agent("droid", AppAlleycatAgentWire.JSONL),
            ),
        )
    }

    private fun modes(ptyAvailable: Boolean = true): AppDroidModeCapabilities =
        AppDroidModeCapabilities(
            supportedModes = if (ptyAvailable) {
                listOf(AppDroidModeKind.JSON_NATIVE, AppDroidModeKind.PTY_TERMINAL)
            } else {
                listOf(AppDroidModeKind.JSON_NATIVE)
            },
            jsonNative = AppDroidJsonNativeCapability(
                available = true,
                agentName = "droid",
                wire = AppAlleycatAgentWire.JSONL,
                unavailableReason = null,
            ),
            ptyTerminal = AppDroidPtyCapability(
                available = ptyAvailable,
                unavailableReasonKind = null,
                unavailableReason = null,
                agentName = "droid-pty",
                displayName = "Droid Terminal",
                launchAgent = "droid-pty",
                transport = AppAgentTerminalTransport.DROID_PTY,
                protocolVersion = 1u,
                features = listOf("output", "input", "resize", "close"),
                label = "Droid TUI",
            ),
        )

    private fun agent(name: String, wire: AppAlleycatAgentWire): AppAlleycatAgentInfo =
        AppAlleycatAgentInfo(
            name = name,
            displayName = name,
            runtimeKind = if (name == "droid") "droid" else null,
            wire = wire,
            available = true,
            unavailableReason = null,
            presentation = null,
            capabilities = null,
        )

    private fun savedServer(): SavedServer =
        SavedServer(
            id = "alleycat:node-1",
            name = "Dev host",
            hostname = "node-1",
            port = 0,
            hasCodexServer = true,
            rememberedByUser = true,
            alleycatNodeId = "node-1",
            alleycatRelay = "https://relay.example",
        )
}
