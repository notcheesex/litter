package com.litter.android.state

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import uniffi.codex_mobile_client.AppStore
import uniffi.codex_mobile_client.TerminalBackendKind
import uniffi.codex_mobile_client.TerminalOutputListener
import uniffi.codex_mobile_client.TerminalSize
import uniffi.codex_mobile_client.TerminalSshTrustStore

class TerminalSessionController(
    private val scope: CoroutineScope,
    private val appStore: AppStore = AppModel.shared.store,
) {
    enum class Phase {
        IDLE,
        CONNECTING,
        RUNNING,
        EXITED,
        FAILED,
    }

    data class SshHostTrustChallenge(
        val host: String,
        val port: UShort,
        val fingerprint: String,
        val backend: TerminalBackendKind,
    )

    var phase by mutableStateOf(Phase.IDLE)
        private set
    var output by mutableStateOf("")
        private set
    var exitCode by mutableStateOf<Int?>(null)
        private set
    var errorMessage by mutableStateOf<String?>(null)
        private set
    var sshTrustChallenge by mutableStateOf<SshHostTrustChallenge?>(null)
        private set

    var sessionId: String? = null
        private set
    private var listener: TerminalOutputListener? = null
    private var outputByteSink: ((ByteArray) -> Unit)? = null
    private val outputBuffer = TerminalOutputBuffer()
    private var outputFlushScheduled = false
    private var eventGeneration: Int = 0
    private var terminalCols: UShort = 80u
    private var terminalRows: UShort = 24u
    private var lastBackend: TerminalBackendKind? = null

    val canSendInput: Boolean
        get() = phase == Phase.RUNNING

    fun openLocalProot(cwd: String? = null) {
        open(TerminalBackendKind.LocalProot(normalized(cwd)))
    }

    fun open(backend: TerminalBackendKind) {
        if (sessionId != null || phase == Phase.CONNECTING) return
        lastBackend = backend
        eventGeneration += 1
        val generation = eventGeneration
        phase = Phase.CONNECTING
        errorMessage = null
        exitCode = null
        sshTrustChallenge = null
        scope.launch {
            try {
                val size = TerminalSize(cols = terminalCols, rows = terminalRows)
                val id = if (backend is TerminalBackendKind.RemoteSsh) {
                    val backendImpl = SshTrustStore(AppModel.shared.appContext)
                    val trustStore = TerminalSshTrustStore(backendImpl)
                    appStore.openTerminalSessionWithTrustStore(backend, size, trustStore)
                } else {
                    appStore.openTerminalSession(backend, size)
                }
                if (generation != eventGeneration) {
                    runCatching { appStore.closeTerminalSession(id) }
                    return@launch
                }
                sessionId = id
                appStore.setActiveTerminalId(id)
                val opened = appStore.terminalSessionHandle(id) ?: run {
                    sessionId = null
                    errorMessage = "Session disappeared after open"
                    phase = Phase.FAILED
                    return@launch
                }
                val outputListener = object : TerminalOutputListener {
                    override fun onBytes(data: ByteArray) {
                        scope.launch(Dispatchers.Main.immediate) {
                            if (generation == eventGeneration) {
                                appendOutput(data)
                            }
                        }
                    }

                    override fun onExit(code: Int) {
                        scope.launch(Dispatchers.Main.immediate) {
                            if (generation == eventGeneration) {
                                flushPendingOutput()
                                exitCode = code
                                if (code < 0) {
                                    errorMessage = if (output.isBlank()) {
                                        streamClosedMessage()
                                    } else {
                                        streamEndedUnexpectedlyMessage()
                                    }
                                    phase = Phase.FAILED
                                } else {
                                    phase = Phase.EXITED
                                }
                            }
                        }
                    }
                }
                opened.subscribeOutput(outputListener)
                listener = outputListener
                phase = Phase.RUNNING
            } catch (error: Exception) {
                if (generation != eventGeneration) return@launch
                sessionId = null
                val challenge = sshHostTrustChallenge(error, backend)
                if (challenge != null) {
                    sshTrustChallenge = challenge
                    errorMessage = "Unknown SSH host key ${challenge.fingerprint}"
                } else {
                    errorMessage = error.message ?: "Unable to open terminal"
                }
                phase = Phase.FAILED
            }
        }
    }

    fun trustUnknownSshHostAndRetry() {
        val challenge = sshTrustChallenge ?: return
        SshTrustStore(AppModel.shared.appContext).write(
            host = challenge.host,
            port = challenge.port,
            fingerprint = challenge.fingerprint,
        )
        sshTrustChallenge = null
        errorMessage = null
        phase = Phase.IDLE
        open(challenge.backend)
    }

    fun switchBackend(backend: TerminalBackendKind) {
        close()
        clearOutput()
        open(backend)
    }

    fun retry() {
        val backend = lastBackend ?: return
        close()
        clearOutput()
        open(backend)
    }

    fun send(value: String) {
        sendBytes(value.toByteArray(Charsets.UTF_8))
    }

    fun sendBytes(bytes: ByteArray) {
        if (bytes.isEmpty()) return
        val id = sessionId ?: return
        if (!canSendInput) return
        scope.launch {
            try {
                appStore.writeToTerminalSession(id, bytes)
            } catch (error: Exception) {
                errorMessage = error.message ?: "Unable to write terminal input"
                phase = Phase.FAILED
            }
        }
    }

    fun sendLine(value: String) {
        send("$value\n")
    }

    fun interrupt() {
        val id = sessionId ?: return
        if (!canSendInput) return
        scope.launch {
            try {
                appStore.interruptTerminalSession(id)
            } catch (error: Exception) {
                errorMessage = error.message ?: "Unable to interrupt terminal session"
                phase = Phase.FAILED
            }
        }
    }

    fun clearOutput() {
        outputBuffer.clear()
        output = ""
    }

    fun setOutputByteSink(sink: ((ByteArray) -> Unit)?) {
        outputByteSink = sink
    }

    private fun sshHostTrustChallenge(
        error: Exception,
        backend: TerminalBackendKind,
    ): SshHostTrustChallenge? {
        val sshBackend = backend as? TerminalBackendKind.RemoteSsh ?: return null
        val fingerprint = unknownHostFingerprint(error.message.orEmpty()) ?: return null
        return SshHostTrustChallenge(
            host = sshBackend.host,
            port = sshBackend.port,
            fingerprint = fingerprint,
            backend = backend,
        )
    }

    private fun unknownHostFingerprint(message: String): String? {
        val marker = "unknown-host:"
        val start = message.indexOf(marker)
        if (start < 0) return null
        return message
            .substring(start + marker.length)
            .trim()
            .trim('"', '\'', '(', ')', '[', ']')
            .takeIf { it.isNotEmpty() }
    }

    fun resize(cols: Int, rows: Int, notifyBackend: Boolean = true) {
        if (cols <= 0 || rows <= 0) return
        terminalCols = cols.coerceIn(1, UShort.MAX_VALUE.toInt()).toUShort()
        terminalRows = rows.coerceIn(1, UShort.MAX_VALUE.toInt()).toUShort()
        val id = sessionId ?: return
        if (!notifyBackend || !canSendInput) return
        val size = TerminalSize(cols = terminalCols, rows = terminalRows)
        scope.launch {
            try {
                appStore.resizeTerminalSession(id, size)
            } catch (error: Exception) {
                errorMessage = error.message ?: "Unable to resize terminal"
                phase = Phase.FAILED
            }
        }
    }

    fun close() {
        eventGeneration += 1
        val id = sessionId
        sessionId = null
        listener = null
        outputFlushScheduled = false
        phase = Phase.IDLE
        if (id != null) {
            scope.launch {
                runCatching { appStore.closeTerminalSession(id) }
            }
        }
    }

    fun closeFromUser() {
        eventGeneration += 1
        val id = sessionId
        sessionId = null
        listener = null
        outputFlushScheduled = false
        errorMessage = null
        sshTrustChallenge = null
        flushPendingOutput()
        exitCode = exitCode ?: 0
        phase = Phase.EXITED
        if (id != null) {
            scope.launch {
                runCatching { appStore.closeTerminalSession(id) }
            }
        }
    }

    private fun appendOutput(data: ByteArray) {
        outputByteSink?.invoke(data.copyOf())
        outputBuffer.append(data)
        scheduleOutputFlush()
    }

    private fun scheduleOutputFlush() {
        if (outputFlushScheduled) return
        outputFlushScheduled = true
        scope.launch(Dispatchers.Main.immediate) {
            delay(32)
            outputFlushScheduled = false
            flushPendingOutput()
        }
    }

    private fun flushPendingOutput() {
        if (outputBuffer.flush()) {
            output = outputBuffer.text
        }
    }

    private fun normalized(value: String?): String? {
        val trimmed = value?.trim().orEmpty()
        return trimmed.ifEmpty { null }
    }

    private fun streamClosedMessage(): String =
        if (lastBackend is TerminalBackendKind.RemoteDroidPty) {
            "Droid TUI terminal stream closed. Retry the PTY session or go back."
        } else {
            "Terminal stream closed. Retry the session or go back."
        }

    private fun streamEndedUnexpectedlyMessage(): String =
        if (lastBackend is TerminalBackendKind.RemoteDroidPty) {
            "Droid TUI terminal ended unexpectedly. Review terminal output, then retry if needed."
        } else {
            "Terminal ended unexpectedly. Review terminal output, then retry if needed."
        }

}
