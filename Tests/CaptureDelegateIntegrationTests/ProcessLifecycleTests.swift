import Foundation
import Testing

@testable import CaptureDelegateIPC

@Test("Swift process API receives cat output before one terminal event")
func processLifecycle() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else {
        throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1)
    }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-process-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let existing = directory.appending(path: "existing.txt")
    let missing = directory.appending(path: "missing.txt")
    try "present output\n".write(to: existing, atomically: true, encoding: .utf8)
    let socket = directory.appending(path: "backend.sock")
    let backend = try startBackend(binary: backendBinary, socket: socket)
    defer {
        if backend.isRunning { backend.terminate() }
        backend.waitUntilExit()
    }

    var events: [ProcessEvent] = []
    try IPCClient.startProcess(
        socketPath: socket.path(),
        runID: "swift-run",
        executable: "/bin/cat",
        arguments: [existing.path(), missing.path()],
        timeoutMilliseconds: 2_000
    ) { event in
        events.append(event)
    }

    let terminalIndexes = events.indices.filter {
        if case .exit = events[$0] { return true }
        return false
    }
    #expect(terminalIndexes == [events.count - 1])
    #expect(
        events.contains { event in
            if case .output(let runID, let stream, let output) = event {
                return runID == "swift-run" && stream == .stdout
                    && output.contains("present output")
            }
            return false
        })
    #expect(
        events.contains { event in
            if case .output(let runID, let stream, let output) = event {
                return runID == "swift-run" && stream == .stderr && output.contains("missing.txt")
            }
            return false
        })
    #expect(
        events.contains { event in
            guard case .exit(let runID, let exitCode, _) = event, let exitCode else { return false }
            return runID == "swift-run" && exitCode != 0
        })
}

@Test("nonexistent executable becomes a typed spawn failure event")
func nonexistentExecutable() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else {
        throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1)
    }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-missing-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let socket = directory.appending(path: "backend.sock")
    let backend = try startBackend(binary: backendBinary, socket: socket)
    defer {
        if backend.isRunning { backend.terminate() }
        backend.waitUntilExit()
    }

    var events: [ProcessEvent] = []
    try IPCClient.startProcess(
        socketPath: socket.path(),
        runID: "missing-run",
        executable: "/definitely/does/not/exist/capture-delegate",
        arguments: [],
        timeoutMilliseconds: 2_000
    ) { event in
        events.append(event)
    }

    #expect(
        events == [
            .exit(runID: "missing-run", exitCode: nil, errorCode: .spawnFailed)
        ])
}

@Test("sleeping process emits a typed timeout after preserving prior output")
func timedOutProcess() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else {
        throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1)
    }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-timeout-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let socket = directory.appending(path: "backend.sock")
    let backend = try startBackend(binary: backendBinary, socket: socket)
    defer {
        if backend.isRunning { backend.terminate() }
        backend.waitUntilExit()
    }

    let started = Date()
    var events: [ProcessEvent] = []
    try IPCClient.startProcess(
        socketPath: socket.path(),
        runID: "timeout-run",
        executable: "/bin/sh",
        arguments: ["-c", "printf before-timeout; sleep 2"],
        timeoutMilliseconds: 200
    ) { event in
        events.append(event)
    }

    #expect(Date().timeIntervalSince(started) < 1)
    #expect(
        events.contains { event in
            if case .output(_, .stdout, let output) = event {
                return output.contains("before-timeout")
            }
            return false
        })
    #expect(
        events == [
            .output(runID: "timeout-run", stream: .stdout, output: "before-timeout"),
            .exit(runID: "timeout-run", exitCode: nil, errorCode: .timedOut),
        ])
}

@Test("Swift cancels a running process over a separate typed connection")
func cancelledProcess() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else { throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1) }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-cancel-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let socket = directory.appending(path: "backend.sock")
    let backend = try startBackend(binary: backendBinary, socket: socket)
    defer {
        if backend.isRunning { backend.terminate() }
        backend.waitUntilExit()
    }

    let outputReceived = DispatchSemaphore(value: 0)
    let finished = DispatchSemaphore(value: 0)
    let collector = IntegrationEventCollector()
    Thread {
        defer { finished.signal() }
        do {
            try IPCClient.startProcess(
                socketPath: socket.path(), runID: "swift-cancel-run", executable: "/bin/sh",
                arguments: ["-c", "printf ready; sleep 2"], timeoutMilliseconds: 2_000
            ) { event in
                collector.append(event)
                if case .output = event { outputReceived.signal() }
            }
        } catch { collector.record(error) }
    }.start()

    #expect(outputReceived.wait(timeout: .now() + 2) == .success)
    #expect(
        try IPCClient.cancelProcess(socketPath: socket.path(), runID: "swift-cancel-run")
            == .accepted)
    #expect(finished.wait(timeout: .now() + 2) == .success)
    #expect(collector.error == nil)
    #expect(
        collector.events.last
            == .exit(runID: "swift-cancel-run", exitCode: nil, errorCode: .cancelled))
}

@Test("Swift pauses, resumes, and cancels a running process over typed connections")
func pausedAndResumedProcess() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else { throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1) }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-pause-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let socket = directory.appending(path: "backend.sock")
    let backend = try startBackend(binary: backendBinary, socket: socket)
    defer {
        if backend.isRunning { backend.terminate() }
        backend.waitUntilExit()
    }

    let outputReceived = DispatchSemaphore(value: 0)
    let finished = DispatchSemaphore(value: 0)
    let collector = IntegrationEventCollector()
    Thread {
        defer { finished.signal() }
        do {
            try IPCClient.startProcess(
                socketPath: socket.path(), runID: "swift-pause-run", executable: "/bin/sh",
                arguments: ["-c", "printf ready; sleep 30"], timeoutMilliseconds: 30_000
            ) { event in
                collector.append(event)
                if case .output = event { outputReceived.signal() }
            }
        } catch { collector.record(error) }
    }.start()

    #expect(outputReceived.wait(timeout: .now() + 2) == .success)
    #expect(
        try IPCClient.pauseProcess(socketPath: socket.path(), runID: "swift-pause-run")
            == .accepted)
    #expect(
        try IPCClient.resumeProcess(socketPath: socket.path(), runID: "swift-pause-run")
            == .accepted)
    #expect(
        try IPCClient.cancelProcess(socketPath: socket.path(), runID: "swift-pause-run")
            == .accepted)
    #expect(finished.wait(timeout: .now() + 2) == .success)
    #expect(collector.error == nil)
    #expect(
        collector.events.last
            == .exit(runID: "swift-pause-run", exitCode: nil, errorCode: .cancelled))
}

private final class IntegrationEventCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var storedEvents: [ProcessEvent] = []
    private var storedError: Error?

    var events: [ProcessEvent] { lock.withLock { storedEvents } }
    var error: Error? { lock.withLock { storedError } }

    func append(_ event: ProcessEvent) { lock.withLock { storedEvents.append(event) } }
    func record(_ error: Error) { lock.withLock { storedError = error } }
}

private func startBackend(binary: String, socket: URL) throws -> Process {
    let backend = Process()
    backend.executableURL = URL(filePath: binary)
    backend.arguments = ["--socket", socket.path()]
    backend.standardOutput = FileHandle.nullDevice
    backend.standardError = FileHandle.nullDevice
    try backend.run()

    let deadline = Date().addingTimeInterval(5)
    while Date() < deadline {
        if (try? IPCClient.checkHealth(socketPath: socket.path())) == true { return backend }
        if !backend.isRunning {
            backend.waitUntilExit()
            throw NSError(
                domain: "CaptureDelegateIntegrationTests", code: Int(backend.terminationStatus))
        }
        Thread.sleep(forTimeInterval: 0.02)
    }
    backend.terminate()
    backend.waitUntilExit()
    throw NSError(domain: "CaptureDelegateIntegrationTests", code: 2)
}
