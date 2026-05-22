package com.litter.android.ui.terminal

import android.content.Context
import com.litter.android.core.bridge.UniffiInit
import com.litter.android.state.AlleycatCredentialStore
import com.litter.android.state.AppModel
import com.litter.android.state.SavedServer
import com.litter.android.state.SavedServerStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import uniffi.codex_mobile_client.AlleycatBridge
import uniffi.codex_mobile_client.AppAlleycatAgentInfo
import uniffi.codex_mobile_client.AppAlleycatPairPayload
import uniffi.codex_mobile_client.AppDroidModeCapabilities

data class DroidTerminalTarget(
    val serverId: String,
    val displayName: String,
    val nodeId: String,
    val relay: String?,
    val token: String,
    val agentName: String?,
    val label: String,
)

object DroidTerminalSupport {
    const val DEFAULT_LABEL = "Droid TUI"
    const val JSON_MODE_LABEL = "Droid native chat"
    const val TERMINAL_MODE_LABEL = "Droid TUI terminal"

    fun modeLabelForAgent(
        agent: AppAlleycatAgentInfo,
        capabilities: AppDroidModeCapabilities,
    ): String? {
        val name = normalized(agent.name) ?: return null
        val pty = capabilities.ptyTerminal
        val json = capabilities.jsonNative
        return when {
            pty.agentName?.trim() == name || pty.launchAgent?.trim() == name ->
                TERMINAL_MODE_LABEL
            json.agentName?.trim() == name ->
                JSON_MODE_LABEL
            else -> null
        }
    }

    fun targetForSavedServer(
        saved: SavedServer,
        token: String?,
        capabilities: AppDroidModeCapabilities,
    ): DroidTerminalTarget? {
        val nodeId = normalized(saved.alleycatNodeId) ?: return null
        val resolvedToken = normalized(token) ?: return null
        val pty = capabilities.ptyTerminal
        if (!pty.available) return null
        val label = normalized(pty.label) ?: DEFAULT_LABEL
        return DroidTerminalTarget(
            serverId = saved.id,
            displayName = saved.name.trim().ifEmpty { "Alleycat ${shortNodeId(nodeId)}" },
            nodeId = nodeId,
            relay = normalized(saved.alleycatRelay),
            token = resolvedToken,
            agentName = normalized(pty.launchAgent) ?: normalized(pty.agentName),
            label = label,
        )
    }

    suspend fun discoverTargets(
        context: Context,
        appModel: AppModel = AppModel.shared,
        alleycatBridge: AlleycatBridge = AlleycatBridge(),
    ): List<DroidTerminalTarget> = withContext(Dispatchers.IO) {
        val appContext = context.applicationContext
        UniffiInit.ensure(appContext)
        val credentialStore = AlleycatCredentialStore(appContext)
        SavedServerStore.remembered(appContext)
            .mapNotNull { saved ->
                val nodeId = normalized(saved.alleycatNodeId) ?: return@mapNotNull null
                val token = normalized(credentialStore.loadToken(nodeId)) ?: return@mapNotNull null
                val params = AppAlleycatPairPayload(
                    v = 1u,
                    nodeId = nodeId,
                    token = token,
                    relay = normalized(saved.alleycatRelay),
                    hostName = normalized(saved.name),
                )
                runCatching {
                    val agents = appModel.serverBridge.listAlleycatAgents(params)
                    val capabilities = alleycatBridge.droidModeCapabilities(agents)
                    targetForSavedServer(saved, token, capabilities)
                }.getOrNull()
            }
    }

    fun preferredTarget(
        targets: List<DroidTerminalTarget>,
        preferredServerId: String?,
    ): DroidTerminalTarget? {
        val preferred = normalized(preferredServerId)
        return preferred
            ?.let { serverId -> targets.firstOrNull { it.serverId == serverId } }
            ?: targets.firstOrNull()
    }

    fun backendId(nodeId: String, agentName: String?): String {
        val agent = normalized(agentName) ?: "default"
        return "droid-pty-$nodeId-$agent"
    }

    private fun normalized(value: String?): String? =
        value?.trim()?.takeIf { it.isNotEmpty() }

    private fun shortNodeId(raw: String): String =
        if (raw.length <= 16) raw else raw.take(8) + "..." + raw.takeLast(8)
}
