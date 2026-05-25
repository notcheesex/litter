import Foundation
import Observation

@MainActor
@Observable
final class TerminalSessionController {
    enum Phase: Equatable {
        case idle
        case connecting
        case running
        case exited(Int32)
        case failed(String)
    }

    struct SshHostTrustChallenge {
        let host: String
        let port: UInt16
        let fingerprint: String
        let backend: TerminalBackendKind
    }

    private(set) var phase: Phase = .idle
    private(set) var output = ""
    private(set) var sessionId: String?
    private(set) var sshTrustChallenge: SshHostTrustChallenge?

    @ObservationIgnored private let appStore: AppStore
    @ObservationIgnored private var outputListener: TerminalOutputRelay?
    @ObservationIgnored private var outputSink: ((Data) -> Void)?
    @ObservationIgnored private var outputBuffer = TerminalOutputBuffer()
    @ObservationIgnored private var outputFlushScheduled = false
    @ObservationIgnored private var eventGeneration = 0
    @ObservationIgnored private var terminalSize = TerminalSize(cols: 80, rows: 24)
    @ObservationIgnored private var lastBackend: TerminalBackendKind?

    init(appStore: AppStore = AppModel.shared.store) {
        self.appStore = appStore
    }

    var canSendInput: Bool {
        if case .running = phase { return true }
        return false
    }

    func openLocalIsh(cwd: String?) async {
        await open(backend: .localIsh(cwd: normalized(cwd)))
    }

    func open(backend: TerminalBackendKind) async {
        guard sessionId == nil else { return }
        lastBackend = backend
        eventGeneration &+= 1
        let generation = eventGeneration
        phase = .connecting
        sshTrustChallenge = nil
        do {
            let id: String
            if isSshBackend(backend) {
                let trustStore = TerminalSshTrustStore(backend: SwiftSshTrustBackend.shared)
                id = try await appStore.openTerminalSessionWithTrustStore(
                    kind: backend,
                    size: terminalSize,
                    trustStore: trustStore
                )
            } else {
                id = try await appStore.openTerminalSession(
                    kind: backend,
                    size: terminalSize
                )
            }
            guard generation == eventGeneration else {
                try? await appStore.closeTerminalSession(id: id)
                return
            }
            sessionId = id
            appStore.setActiveTerminalId(id: id)
            guard let session = appStore.terminalSessionHandle(id: id) else {
                phase = .failed("Session disappeared after open")
                sessionId = nil
                return
            }
            let listener = TerminalOutputRelay(owner: self, generation: generation)
            session.subscribeOutput(listener: listener)
            outputListener = listener
            phase = .running
        } catch {
            guard generation == eventGeneration else { return }
            sessionId = nil
            if let challenge = Self.sshHostTrustChallenge(from: error, backend: backend) {
                sshTrustChallenge = challenge
                phase = .failed("Unknown SSH host key \(challenge.fingerprint)")
            } else {
                phase = .failed(error.localizedDescription)
            }
        }
    }

    private func isSshBackend(_ backend: TerminalBackendKind) -> Bool {
        if case .remoteSsh = backend { return true }
        return false
    }

    func trustUnknownSshHostAndRetry() async {
        guard let challenge = sshTrustChallenge else { return }
        SwiftSshTrustBackend.shared.write(
            host: challenge.host,
            port: challenge.port,
            fingerprint: challenge.fingerprint
        )
        sshTrustChallenge = nil
        phase = .idle
        await open(backend: challenge.backend)
    }

    func switchBackend(_ backend: TerminalBackendKind) async {
        close()
        clearOutput()
        await open(backend: backend)
    }

    func retry() async {
        guard let backend = lastBackend else { return }
        close()
        clearOutput()
        await open(backend: backend)
    }

    func send(_ string: String) async {
        await send(Data(string.utf8))
    }

    func send(_ data: Data) async {
        guard let id = sessionId, canSendInput else { return }
        do {
            try await appStore.writeToTerminalSession(id: id, data: data)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func sendLine(_ string: String) async {
        await send(string + "\n")
    }

    func interrupt() async {
        guard let id = sessionId, canSendInput else { return }
        do {
            try await appStore.interruptTerminalSession(id: id)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func clearOutput() {
        outputBuffer.clear()
        output = ""
    }

    func setOutputSink(_ sink: ((Data) -> Void)?) {
        outputSink = sink
    }

    private static func sshHostTrustChallenge(
        from error: Error,
        backend: TerminalBackendKind
    ) -> SshHostTrustChallenge? {
        guard case let .remoteSsh(
            host: host,
            port: port,
            username: _,
            auth: _,
            shell: _,
            acceptUnknownHost: _,
            cwd: _
        ) = backend else {
            return nil
        }
        guard let fingerprint = unknownHostFingerprint(from: error.localizedDescription) else {
            return nil
        }
        return SshHostTrustChallenge(
            host: host,
            port: port,
            fingerprint: fingerprint,
            backend: backend
        )
    }

    private static func unknownHostFingerprint(from description: String) -> String? {
        guard let range = description.range(of: "unknown-host:") else { return nil }
        let raw = description[range.upperBound...]
        let fingerprint = raw
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: "\"'()[]"))
        return fingerprint.isEmpty ? nil : fingerprint
    }

    func resize(cols: UInt16, rows: UInt16, notifyBackend: Bool = true) async {
        guard cols > 0, rows > 0 else { return }
        let size = TerminalSize(cols: cols, rows: rows)
        terminalSize = size
        guard notifyBackend, let id = sessionId, canSendInput else { return }
        do {
            try await appStore.resizeTerminalSession(id: id, size: size)
        } catch {
            phase = .failed(error.localizedDescription)
        }
    }

    func close() {
        eventGeneration &+= 1
        guard let id = sessionId else { return }
        sessionId = nil
        outputListener = nil
        outputFlushScheduled = false
        phase = .idle
        Task {
            try? await appStore.closeTerminalSession(id: id)
        }
    }

    func closeFromUser() {
        eventGeneration &+= 1
        let id = sessionId
        sessionId = nil
        outputListener = nil
        outputFlushScheduled = false
        sshTrustChallenge = nil
        flushPendingOutput()
        phase = .exited(0)
        if let id {
            Task {
                try? await appStore.closeTerminalSession(id: id)
            }
        }
    }

    fileprivate func appendOutput(_ data: Data, generation: Int) {
        guard generation == eventGeneration else { return }
        outputSink?(data)
        outputBuffer.append(data)
        scheduleOutputFlush()
    }

    fileprivate func markExited(_ code: Int32, generation: Int) {
        guard generation == eventGeneration else { return }
        flushPendingOutput()
        if code < 0 {
            let message = output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                ? streamClosedMessage()
                : streamEndedUnexpectedlyMessage()
            phase = .failed(message)
        } else {
            phase = .exited(code)
        }
    }

    private func normalized(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }

    private func streamClosedMessage() -> String {
        if case .some(.remoteDroidPty) = lastBackend {
            return "Droid TUI terminal stream closed. Retry the PTY session or go back."
        }
        return "Terminal stream closed. Retry the session or go back."
    }

    private func streamEndedUnexpectedlyMessage() -> String {
        if case .some(.remoteDroidPty) = lastBackend {
            return "Droid TUI terminal ended unexpectedly. Review terminal output, then retry if needed."
        }
        return "Terminal ended unexpectedly. Review terminal output, then retry if needed."
    }

    private func scheduleOutputFlush() {
        guard !outputFlushScheduled else { return }
        outputFlushScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.032) { [weak self] in
            Task { @MainActor [weak self] in
                guard let self else { return }
                self.outputFlushScheduled = false
                self.flushPendingOutput()
            }
        }
    }

    private func flushPendingOutput() {
        if outputBuffer.flush() {
            output = outputBuffer.text
        }
    }
}

private final class TerminalOutputRelay: TerminalOutputListener, @unchecked Sendable {
    private weak var owner: TerminalSessionController?
    private let generation: Int

    init(owner: TerminalSessionController, generation: Int) {
        self.owner = owner
        self.generation = generation
    }

    func onBytes(data: Data) {
        Task { @MainActor [weak owner, generation] in
            owner?.appendOutput(data, generation: generation)
        }
    }

    func onExit(code: Int32) {
        Task { @MainActor [weak owner, generation] in
            owner?.markExited(code, generation: generation)
        }
    }
}
