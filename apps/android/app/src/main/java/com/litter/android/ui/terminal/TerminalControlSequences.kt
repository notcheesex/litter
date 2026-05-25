package com.litter.android.ui.terminal

internal object TerminalControlSequences {
    const val ESCAPE = "\u001B"
    const val TAB = "\t"
    const val ENTER = "\r"
    const val BACKSPACE = "\u007F"
    const val CTRL_C = "\u0003"
    const val CTRL_D = "\u0004"
    const val CTRL_Z = "\u001A"
    const val ARROW_LEFT = "\u001B[D"
    const val ARROW_UP = "\u001B[A"
    const val ARROW_DOWN = "\u001B[B"
    const val ARROW_RIGHT = "\u001B[C"
    const val HOME = "\u001B[H"
    const val END = "\u001B[F"
    const val PAGE_UP = "\u001B[5~"
    const val PAGE_DOWN = "\u001B[6~"
    const val DROID_MISSIONS_COMMAND = "/missions\n"

    val terminalAccessoryLabels = listOf(
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
    )
}
