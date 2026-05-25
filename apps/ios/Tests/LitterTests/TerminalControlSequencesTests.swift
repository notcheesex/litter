import XCTest
@testable import Litter

final class TerminalControlSequencesTests: XCTestCase {
    func testAccessoryControlsIncludeSoftKeyboardTerminalEssentials() {
        let labels = TerminalControlSequences.terminalAccessoryLabels

        [
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
        ].forEach { label in
            XCTAssertTrue(labels.contains(label), "missing terminal accessory label \(label)")
        }
    }

    func testMissionsShortcutIsTerminalTextNotNativeCommandToken() {
        XCTAssertEqual(TerminalControlSequences.droidMissionsCommand, "/missions\n")
    }

    func testNavigationAndControlSequencesMatchTerminalByteConventions() {
        XCTAssertEqual(TerminalControlSequences.escape, "\u{1B}")
        XCTAssertEqual(TerminalControlSequences.enter, "\r")
        XCTAssertEqual(TerminalControlSequences.backspace, "\u{7F}")
        XCTAssertEqual(TerminalControlSequences.ctrlC, "\u{03}")
        XCTAssertEqual(TerminalControlSequences.ctrlD, "\u{04}")
        XCTAssertEqual(TerminalControlSequences.pageUp, "\u{1B}[5~")
        XCTAssertEqual(TerminalControlSequences.pageDown, "\u{1B}[6~")
    }
}
