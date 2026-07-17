import Darwin
import Foundation
import Testing

@testable import CaptureDelegateIPC

@Test("health request JSON is newline delimited")
func healthRequestJSON() {
    #expect(IPCClient.healthRequest == "{\"version\":1,\"type\":\"health\"}\n")
}

@Test("closed peer returns an error without SIGPIPE termination")
func closedPeerDoesNotRaiseSIGPIPE() throws {
    var descriptors = [Int32](repeating: -1, count: 2)
    #expect(socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0)
    _ = Darwin.close(descriptors[0])
    defer { _ = Darwin.close(descriptors[1]) }

    var receivedError = false
    do {
        try IPCClient.writeHealthRequest(to: descriptors[1])
    } catch {
        receivedError = true
    }
    #expect(receivedError)
}

@Test("stalled response fails within the client read deadline")
func stalledResponseIsBounded() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let started = Date()
    var receivedError = false
    do {
        _ = try IPCClient.readResponseLine(from: descriptors.reader)
    } catch {
        receivedError = true
    }

    #expect(receivedError)
    #expect(Date().timeIntervalSince(started) < 2)
}

@Test("byte-dribbled responses honor one total monotonic deadline")
func byteDribbledResponseIsBoundedByTotalDeadline() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let writerFinished = DispatchSemaphore(value: 0)
    Thread {
        defer { writerFinished.signal() }
        for byte in Array("dribble".utf8) {
            _ = Darwin.write(descriptors.writer, [byte], 1)
            usleep(100_000)
        }
    }.start()

    let started = Date()
    var receivedError = false
    do {
        _ = try IPCClient.readResponseLine(from: descriptors.reader)
    } catch {
        receivedError = true
    }

    #expect(receivedError)
    #expect(Date().timeIntervalSince(started) < 0.9)
    #expect(writerFinished.wait(timeout: .now() + 2) == .success)
}

@Test("oversized response fails within the frame bound")
func oversizedResponseIsRejected() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }
    var sendBufferBytes: Int32 = 32 * 1024
    #expect(
        setsockopt(
            descriptors.writer,
            SOL_SOCKET,
            SO_SNDBUF,
            &sendBufferBytes,
            socklen_t(MemoryLayout.size(ofValue: sendBufferBytes))
        ) == 0
    )
    let oversized = [UInt8](repeating: 120, count: 8 * 1024 + 1) + [10]
    try writeAll(oversized, to: descriptors.writer)

    do {
        _ = try IPCClient.readResponseLine(from: descriptors.reader)
        Issue.record("oversized response should be rejected")
    } catch let error as IPCClientError {
        #expect(error == .responseTooLarge)
    } catch {
        Issue.record("unexpected error: \(error)")
    }
}

@Test("valid response remains readable over a local socket")
func validResponseRemainsReadable() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }
    let response = "{\"version\":1,\"type\":\"health_response\",\"status\":\"ok\"}\n"
    try writeAll(Array(response.utf8), to: descriptors.writer)

    #expect(try IPCClient.readResponseLine(from: descriptors.reader) == response)
}

private func localSocketPair() throws -> (reader: Int32, writer: Int32) {
    var descriptors = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
        throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }
    return (descriptors[0], descriptors[1])
}

private func writeAll(_ bytes: [UInt8], to descriptor: Int32) throws {
    var offset = 0
    while offset < bytes.count {
        let count = bytes.withUnsafeBytes { buffer in
            Darwin.write(descriptor, buffer.baseAddress!.advanced(by: offset), bytes.count - offset)
        }
        guard count > 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        offset += count
    }
}
