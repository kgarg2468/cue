import Foundation
import Testing

@testable import CaptureDelegateIPC

@Test("Sessions created over the socket survive a backend restart")
func sessionsSurviveBackendRestart() throws {
    guard let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"]
    else {
        throw NSError(domain: "CaptureDelegateIntegrationTests", code: 1)
    }

    let directory = URL(filePath: "/private/tmp")
        .appending(
            path: "capture-delegate-sessions-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
    defer { try? FileManager.default.removeItem(at: directory) }
    let socket = directory.appending(path: "backend.sock")
    let store = directory.appending(path: "store.sqlite")

    let backend = try startBackend(binary: backendBinary, socket: socket, store: store)
    let created = try IPCClient.createSession(
        socketPath: socket.path(), title: "Durable session")
    #expect(!created.id.isEmpty)
    #expect(created.title == "Durable session")
    #expect(created.createdAtMilliseconds == created.updatedAtMilliseconds)
    #expect(
        try IPCClient.listSessions(socketPath: socket.path())
            == SessionListPage(sessions: [created], truncated: false))

    backend.terminate()
    backend.waitUntilExit()

    let restarted = try startBackend(binary: backendBinary, socket: socket, store: store)
    defer {
        if restarted.isRunning { restarted.terminate() }
        restarted.waitUntilExit()
    }

    #expect(
        try IPCClient.listSessions(socketPath: socket.path())
            == SessionListPage(sessions: [created], truncated: false))
}

private func startBackend(binary: String, socket: URL, store: URL) throws -> Process {
    let backend = Process()
    backend.executableURL = URL(filePath: binary)
    backend.arguments = ["--socket", socket.path(), "--store", store.path()]
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
