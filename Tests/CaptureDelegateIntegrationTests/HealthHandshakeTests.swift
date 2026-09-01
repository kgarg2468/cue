import Foundation
import Testing

@Test("Rust backend and Swift health executable complete the health handshake")
func healthHandshake() throws {
    guard
        let backendBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_BACKEND_BINARY"],
        let healthBinary = ProcessInfo.processInfo.environment["CAPTURE_DELEGATE_HEALTH_BINARY"]
    else {
        throw NSError(
            domain: "CaptureDelegateIntegrationTests",
            code: 1,
            userInfo: [
                NSLocalizedDescriptionKey: "prebuilt backend and health binary paths are required"
            ]
        )
    }

    let fixtureName = "capture-delegate-health-\(UUID().uuidString)"
    let socketURL = URL(filePath: "/private/tmp").appending(path: "\(fixtureName).sock")
    let storeDirectoryURL = URL(filePath: "/private/tmp")
        .appending(path: "\(fixtureName).store", directoryHint: .isDirectory)
    let backend = Process()
    let backendError = Pipe()
    var backendLaunched = false
    defer {
        if backendLaunched {
            if backend.isRunning {
                backend.terminate()
            }
            backend.waitUntilExit()
        }
        try? FileManager.default.removeItem(at: socketURL)
        try? FileManager.default.removeItem(at: storeDirectoryURL)
    }

    backend.executableURL = URL(filePath: backendBinary)
    backend.arguments = [
        "--socket", socketURL.path(),
        "--store", storeDirectoryURL.appending(path: "store.sqlite").path(),
    ]
    backend.standardOutput = FileHandle.nullDevice
    backend.standardError = backendError
    try backend.run()
    backendLaunched = true

    let deadline = Date().addingTimeInterval(5)
    while Date() < deadline {
        let attributes = try? FileManager.default.attributesOfItem(atPath: socketURL.path())
        if attributes?[.type] as? FileAttributeType == .typeSocket {
            break
        }
        if !backend.isRunning {
            backend.waitUntilExit()
            backendLaunched = false
            let error = String(
                decoding: backendError.fileHandleForReading.readDataToEndOfFile(),
                as: UTF8.self
            )
            throw NSError(
                domain: "CaptureDelegateIntegrationTests",
                code: Int(backend.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey:
                        "backend exited before binding \(socketURL.path()): \(error)"
                ]
            )
        }
        Thread.sleep(forTimeInterval: 0.02)
    }

    let attributes = try? FileManager.default.attributesOfItem(atPath: socketURL.path())
    guard attributes?[.type] as? FileAttributeType == .typeSocket else {
        throw NSError(
            domain: "CaptureDelegateIntegrationTests",
            code: 2,
            userInfo: [
                NSLocalizedDescriptionKey:
                    "backend did not bind \(socketURL.path()) within 5 seconds"
            ]
        )
    }

    let health = Process()
    let output = Pipe()
    health.executableURL = URL(filePath: healthBinary)
    health.arguments = ["--socket", socketURL.path()]
    health.standardOutput = output
    health.standardError = output

    try health.run()
    let healthDeadline = Date().addingTimeInterval(5)
    while health.isRunning && Date() < healthDeadline {
        Thread.sleep(forTimeInterval: 0.02)
    }
    let healthTimedOut = health.isRunning
    if healthTimedOut {
        health.terminate()
    }
    health.waitUntilExit()

    let text = String(decoding: output.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    #expect(!healthTimedOut, "health executable did not exit within 5 seconds: \(text)")
    #expect(health.terminationStatus == 0, "health executable output: \(text)")
    #expect(text.contains("backend ok\n"))
}
