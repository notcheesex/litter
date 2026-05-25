package com.litter.android.ui.terminal

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class TerminalControlSequencesTests {
    @Test
    fun `accessory controls include soft-keyboard terminal essentials`() {
        val labels = TerminalControlSequences.terminalAccessoryLabels

        listOf(
            "Missions",
            "Esc",
            "Tab",
            "Enter",
            "⌫",
            "Home",
            "End",
            "PgUp",
            "PgDn",
            "Ctrl-C",
            "Ctrl-D",
            "Ctrl-Z",
            "←",
            "↑",
            "↓",
            "→",
            "Paste",
            "Interrupt",
            "Close",
        ).forEach { label ->
            assertTrue("missing terminal accessory label $label", labels.contains(label))
        }
    }

    @Test
    fun `missions shortcut is terminal text not a native command token`() {
        assertEquals("/missions\n", TerminalControlSequences.DROID_MISSIONS_COMMAND)
    }

    @Test
    fun `navigation and control sequences match terminal byte conventions`() {
        assertEquals("\u001B", TerminalControlSequences.ESCAPE)
        assertEquals("\r", TerminalControlSequences.ENTER)
        assertEquals("\u007F", TerminalControlSequences.BACKSPACE)
        assertEquals("\u0003", TerminalControlSequences.CTRL_C)
        assertEquals("\u0004", TerminalControlSequences.CTRL_D)
        assertEquals("\u001B[5~", TerminalControlSequences.PAGE_UP)
        assertEquals("\u001B[6~", TerminalControlSequences.PAGE_DOWN)
    }
}
