import XCTest
@testable import Litter

final class TerminalOutputBufferTests: XCTestCase {
    func testFlushBatchesOutputAndPreservesOrder() {
        var buffer = TerminalOutputBuffer(maxCharacters: 128, maxPendingCharacters: 128)

        buffer.append(Data("hello ".utf8))
        buffer.append(Data("world".utf8))

        XCTAssertTrue(buffer.hasPendingOutput)
        XCTAssertTrue(buffer.flush())
        XCTAssertEqual(buffer.text, "hello world")
        XCTAssertFalse(buffer.hasPendingOutput)
    }

    func testFlushKeepsBoundedTailForBurstOutput() {
        var buffer = TerminalOutputBuffer(maxCharacters: 10, maxPendingCharacters: 20)

        buffer.append(Data("0123456789".utf8))
        _ = buffer.flush()
        buffer.append(Data("abcdefghij".utf8))
        _ = buffer.flush()

        XCTAssertEqual(buffer.text, "abcdefghij")
    }

    func testPendingOutputIsBoundedBeforeScheduledFlush() {
        var buffer = TerminalOutputBuffer(maxCharacters: 64, maxPendingCharacters: 6)

        buffer.append(Data("abcdef".utf8))
        buffer.append(Data("ghijkl".utf8))
        _ = buffer.flush()

        XCTAssertEqual(buffer.text, "ghijkl")
    }
}
