import Foundation

struct TerminalOutputBuffer {
    private let maxCharacters: Int
    private let maxPendingCharacters: Int
    private(set) var text = ""
    private var pending = ""

    init(maxCharacters: Int = 64_000, maxPendingCharacters: Int = 64_000) {
        self.maxCharacters = maxCharacters
        self.maxPendingCharacters = maxPendingCharacters
    }

    var hasPendingOutput: Bool {
        !pending.isEmpty
    }

    mutating func append(_ data: Data) {
        guard !data.isEmpty else { return }
        pending += String(decoding: data, as: UTF8.self)
        trimPendingIfNeeded()
    }

    mutating func flush() -> Bool {
        guard !pending.isEmpty else { return false }
        text = String((text + pending).suffix(maxCharacters))
        pending.removeAll(keepingCapacity: true)
        return true
    }

    mutating func clear() {
        text.removeAll(keepingCapacity: true)
        pending.removeAll(keepingCapacity: true)
    }

    private mutating func trimPendingIfNeeded() {
        guard pending.count > maxPendingCharacters else { return }
        pending = String(pending.suffix(maxPendingCharacters))
    }
}
