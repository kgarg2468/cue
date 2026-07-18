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
        arguments: [existing.path(), missing.path()]
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
        arguments: []
    ) { event in
        events.append(event)
    }

    #expect(
        events == [
            .exit(runID: "missing-run", exitCode: nil, errorCode: .spawnFailed)
        ])
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
