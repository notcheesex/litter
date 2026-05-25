import Foundation

enum TerminalControlSequences {
    static let escape = "\u{1B}"
    static let tab = "\t"
    static let enter = "\r"
    static let backspace = "\u{7F}"
    static let ctrlC = "\u{03}"
    static let ctrlD = "\u{04}"
    static let ctrlZ = "\u{1A}"
    static let arrowLeft = "\u{1B}[D"
    static let arrowUp = "\u{1B}[A"
    static let arrowDown = "\u{1B}[B"
    static let arrowRight = "\u{1B}[C"
    static let home = "\u{1B}[H"
    static let end = "\u{1B}[F"
    static let pageUp = "\u{1B}[5~"
    static let pageDown = "\u{1B}[6~"
    static let droidMissionsCommand = "/missions\n"

    static let terminalAccessoryLabels = [
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
        "Close"
    ]
}
