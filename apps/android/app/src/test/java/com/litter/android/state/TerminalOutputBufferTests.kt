package com.litter.android.state

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TerminalOutputBufferTests {
    @Test
    fun `flush batches output and preserves order`() {
        val buffer = TerminalOutputBuffer(maxChars = 128, maxPendingChars = 128)

        buffer.append("hello ".toByteArray())
        buffer.append("world".toByteArray())

        assertTrue(buffer.hasPending)
        assertTrue(buffer.flush())
        assertEquals("hello world", buffer.text)
        assertFalse(buffer.hasPending)
    }

    @Test
    fun `flush keeps bounded tail for burst output`() {
        val buffer = TerminalOutputBuffer(maxChars = 10, maxPendingChars = 20)

        buffer.append("0123456789".toByteArray())
        buffer.flush()
        buffer.append("abcdefghij".toByteArray())
        buffer.flush()

        assertEquals("abcdefghij", buffer.text)
    }

    @Test
    fun `pending output is bounded before scheduled flush`() {
        val buffer = TerminalOutputBuffer(maxChars = 64, maxPendingChars = 6)

        buffer.append("abcdef".toByteArray())
        buffer.append("ghijkl".toByteArray())
        buffer.flush()

        assertEquals("ghijkl", buffer.text)
    }
}
