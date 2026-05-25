package com.litter.android.state

internal class TerminalOutputBuffer(
    private val maxChars: Int = 64_000,
    private val maxPendingChars: Int = 64_000,
) {
    var text: String = ""
        private set

    private val pending = StringBuilder()

    val hasPending: Boolean
        get() = pending.isNotEmpty()

    fun append(data: ByteArray) {
        if (data.isEmpty()) return
        pending.append(data.toString(Charsets.UTF_8))
        trimPendingIfNeeded()
    }

    fun flush(): Boolean {
        if (pending.isEmpty()) return false
        text = (text + pending.toString()).takeLast(maxChars)
        pending.clear()
        return true
    }

    fun clear() {
        text = ""
        pending.clear()
    }

    private fun trimPendingIfNeeded() {
        if (pending.length <= maxPendingChars) return
        val trimmed = pending.takeLast(maxPendingChars).toString()
        pending.clear()
        pending.append(trimmed)
    }
}
