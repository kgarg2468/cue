import AVFAudio
import Foundation
import Testing

@testable import CaptureDelegateCore

@MainActor
private final class FakeAudioRecorder: AudioRecording {
    var isMeteringEnabled = false
    var shouldRecord = true
    var averagePower: Float = -20
    private(set) var recordCallCount = 0
    private(set) var pauseCallCount = 0
    private(set) var stopCallCount = 0

    func record() -> Bool {
        recordCallCount += 1
        return shouldRecord
    }

    func pause() {
        pauseCallCount += 1
    }

    func stop() {
        stopCallCount += 1
    }

    func updateMeters() {}

    func averagePower(forChannel channelNumber: Int) -> Float {
        averagePower
    }
}

@MainActor
private final class TestClock {
    var wallDate = Date(timeIntervalSince1970: 1_700_000_000)
    var monotonicTime: TimeInterval = 100
}

private enum FactoryError: Error {
    case injected
}

@Test("capture state machine excludes paused time and returns the original start date")
@MainActor
func capturePauseResumeAndStopAccounting() throws {
    let recorder = FakeAudioRecorder()
    let clock = TestClock()
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { _ in recorder },
        wallClock: { clock.wallDate },
        monotonicClock: { clock.monotonicTime },
        schedulesUpdates: false
    )

    try engine.start()
    #expect(engine.state == .recording(startedAt: clock.wallDate))
    #expect(recorder.isMeteringEnabled)

    clock.monotonicTime += 2
    try engine.pause()
    #expect(engine.state == .paused)
    #expect(engine.elapsed == 2)
    #expect(engine.level == 0)
    #expect(!engine.isReceivingAudio)

    clock.monotonicTime += 10
    try engine.resume()
    clock.monotonicTime += 3
    let result = try engine.stop()

    #expect(result.createdAt == clock.wallDate)
    #expect(result.duration == 5)
    #expect(engine.state == .idle)
    #expect(engine.elapsed == 0)
    #expect(recorder.recordCallCount == 2)
    #expect(recorder.pauseCallCount == 1)
    #expect(recorder.stopCallCount == 1)
}

@Test("capture sequencing errors are explicit and discard returns to idle")
@MainActor
func captureSequencingErrorsAndDiscard() throws {
    let recorder = FakeAudioRecorder()
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { _ in recorder },
        wallClock: { Date(timeIntervalSince1970: 1_700_000_000) },
        monotonicClock: { 100 },
        schedulesUpdates: false
    )

    #expect(throws: CaptureEngineError.notRecording) { try engine.pause() }
    #expect(throws: CaptureEngineError.notRecording) { try engine.resume() }
    #expect(throws: CaptureEngineError.notRecording) { try engine.stop() }

    try engine.start()
    #expect(throws: CaptureEngineError.alreadyRecording) { try engine.start() }
    try engine.pause()
    #expect(throws: CaptureEngineError.notRecording) { try engine.pause() }
    try engine.resume()
    #expect(throws: CaptureEngineError.notRecording) { try engine.resume() }

    try engine.discard()
    #expect(engine.state == .idle)
    #expect(recorder.stopCallCount == 1)
}

@Test("capture start honestly rejects denied microphone authorization")
@MainActor
func deniedAuthorizationPreventsRecording() {
    var madeRecorder = false
    let engine = CaptureEngine(
        authorizationProvider: { .denied },
        recorderFactory: { _ in
            madeRecorder = true
            return FakeAudioRecorder()
        },
        wallClock: { Date() },
        monotonicClock: { 0 },
        schedulesUpdates: false
    )

    #expect(throws: CaptureEngineError.permissionDenied) { try engine.start() }
    #expect(!madeRecorder)
    #expect(engine.state == .idle)
}

@Test("capture start removes plaintext created before recorder factory throws")
@MainActor
func captureStartCleansUpAfterThrowingRecorderFactory() throws {
    let recordingDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("CaptureDelegateFactoryFailureTests", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    defer { try? FileManager.default.removeItem(at: recordingDirectory) }
    var createdRecordingURL: URL?
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { url in
            createdRecordingURL = url
            try Data("factory-created plaintext".utf8).write(to: url)
            throw FactoryError.injected
        },
        wallClock: { Date() },
        monotonicClock: { 0 },
        schedulesUpdates: false,
        recordingDirectory: recordingDirectory
    )

    #expect(throws: CaptureEngineError.self) {
        try engine.start()
    }

    let createdRecording = try #require(createdRecordingURL)
    #expect(!FileManager.default.fileExists(atPath: createdRecording.path))
    #expect(engine.state == .idle)
}

@Test("capture reconciliation preserves matching live identity and rejects PID reuse")
@MainActor
func captureReconciliationRespectsProcessOwnership() throws {
    let recordingDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("CaptureDelegateReconciliationTests", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    defer { try? FileManager.default.removeItem(at: recordingDirectory) }
    try FileManager.default.createDirectory(
        at: recordingDirectory,
        withIntermediateDirectories: true
    )
    let currentProcessID: Int32 = 7_001
    let liveForeignProcessID: Int32 = 7_002
    let deadProcessID: Int32 = 7_003
    let reusedProcessID: Int32 = 7_004
    let identityUnavailableProcessID: Int32 = 7_005
    let currentProcessIdentity = "currentlaunch"
    let liveForeignIdentity = "foreignlaunch"
    let oldReusedIdentity = "oldlaunch"
    let legacyRecording =
        recordingDirectory
        .appendingPathComponent(UUID().uuidString)
        .appendingPathExtension("m4a")
    let deadProcessRecording = recordingDirectory.appendingPathComponent(
        "process-\(deadProcessID)-deadlaunch-\(UUID().uuidString).m4a"
    )
    let liveForeignRecording = recordingDirectory.appendingPathComponent(
        "process-\(liveForeignProcessID)-\(liveForeignIdentity)-\(UUID().uuidString).m4a"
    )
    let reusedProcessRecording = recordingDirectory.appendingPathComponent(
        "process-\(reusedProcessID)-\(oldReusedIdentity)-\(UUID().uuidString).m4a"
    )
    let identityUnavailableRecording = recordingDirectory.appendingPathComponent(
        "process-\(identityUnavailableProcessID)-unknownlaunch-\(UUID().uuidString).m4a"
    )
    let heldDeadOwnerRecording = recordingDirectory.appendingPathComponent(
        "held-process-\(deadProcessID)-deadlaunch-\(UUID().uuidString).m4a"
    )
    try Data("legacy orphan".utf8).write(to: legacyRecording)
    try Data("dead owner".utf8).write(to: deadProcessRecording)
    try Data("live foreign owner".utf8).write(to: liveForeignRecording)
    try Data("reused pid owner".utf8).write(to: reusedProcessRecording)
    try Data("live owner with unavailable identity".utf8).write(
        to: identityUnavailableRecording
    )
    try Data("held after save failure".utf8).write(to: heldDeadOwnerRecording)
    var currentProcessRecordingURL: URL?
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { url in
            currentProcessRecordingURL = url
            try Data("current owner".utf8).write(to: url)
            return FakeAudioRecorder()
        },
        wallClock: { Date() },
        monotonicClock: { 0 },
        schedulesUpdates: false,
        recordingDirectory: recordingDirectory,
        processIdentifier: currentProcessID,
        processLivenessProvider: {
            $0 == liveForeignProcessID || $0 == reusedProcessID
                || $0 == identityUnavailableProcessID
        },
        processInstanceIdentity: currentProcessIdentity,
        processInstanceIdentityProvider: { processIdentifier in
            switch processIdentifier {
            case currentProcessID: currentProcessIdentity
            case liveForeignProcessID: liveForeignIdentity
            case reusedProcessID: "replacementlaunch"
            default: nil
            }
        }
    )
    try engine.start()
    let currentProcessRecording = try #require(currentProcessRecordingURL)

    try engine.reconcileStaleRecordings()

    #expect(
        currentProcessRecording.lastPathComponent.hasPrefix(
            "process-\(currentProcessID)-\(currentProcessIdentity)-"
        )
    )
    #expect(!FileManager.default.fileExists(atPath: legacyRecording.path))
    #expect(!FileManager.default.fileExists(atPath: deadProcessRecording.path))
    #expect(!FileManager.default.fileExists(atPath: reusedProcessRecording.path))
    #expect(FileManager.default.fileExists(atPath: currentProcessRecording.path))
    #expect(FileManager.default.fileExists(atPath: liveForeignRecording.path))
    #expect(FileManager.default.fileExists(atPath: identityUnavailableRecording.path))
    #expect(FileManager.default.fileExists(atPath: heldDeadOwnerRecording.path))

    try engine.discard()
}

@Test("held recordings survive reconciliation across a quit with an unresolved save failure")
@MainActor
func heldRecordingIsPreservedAcrossReconciliation() throws {
    let recordingDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("CaptureDelegateHeldRecordingTests", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    defer { try? FileManager.default.removeItem(at: recordingDirectory) }
    try FileManager.default.createDirectory(
        at: recordingDirectory,
        withIntermediateDirectories: true
    )
    let deadProcessID: Int32 = 8_001
    let originalRecording = recordingDirectory.appendingPathComponent(
        "process-\(deadProcessID)-deadlaunch-\(UUID().uuidString).m4a"
    )
    try Data("only copy of the user's audio".utf8).write(to: originalRecording)

    let heldRecording = try CaptureEngine.markRecordingHeld(at: originalRecording)
    #expect(heldRecording.lastPathComponent.hasPrefix(CaptureEngine.heldRecordingPrefix))
    #expect(!FileManager.default.fileExists(atPath: originalRecording.path))
    #expect(FileManager.default.fileExists(atPath: heldRecording.path))

    // Marking an already-held recording is a no-op, not a double prefix.
    #expect(try CaptureEngine.markRecordingHeld(at: heldRecording) == heldRecording)

    // Simulate the next launch: the owning process is gone, reconciliation runs.
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { _ in FakeAudioRecorder() },
        wallClock: { Date() },
        monotonicClock: { 0 },
        schedulesUpdates: false,
        recordingDirectory: recordingDirectory,
        processIdentifier: 8_002,
        processLivenessProvider: { _ in false },
        processInstanceIdentity: "nextlaunch",
        processInstanceIdentityProvider: { _ in nil }
    )
    try engine.reconcileStaleRecordings()

    #expect(FileManager.default.fileExists(atPath: heldRecording.path))
}

@Test("capture discard surfaces plaintext cleanup failure")
@MainActor
func captureDiscardSurfacesCleanupFailure() throws {
    let recordingDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("CaptureDelegateDiscardTests", isDirectory: true)
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    defer {
        try? FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: recordingDirectory.path
        )
        try? FileManager.default.removeItem(at: recordingDirectory)
    }
    let engine = CaptureEngine(
        authorizationProvider: { .authorized },
        recorderFactory: { url in
            try Data("live plaintext".utf8).write(to: url)
            return FakeAudioRecorder()
        },
        wallClock: { Date() },
        monotonicClock: { 0 },
        schedulesUpdates: false,
        recordingDirectory: recordingDirectory
    )
    try engine.start()
    try FileManager.default.setAttributes(
        [.posixPermissions: 0o500],
        ofItemAtPath: recordingDirectory.path
    )

    #expect(throws: CaptureEngineError.self) {
        try engine.discard()
    }
}

@Test("playback rejects corrupt audio data")
@MainActor
func playbackRejectsGarbageData() {
    #expect(throws: (any Error).self) {
        try PlaybackEngine(data: Data("not audio".utf8))
    }
}

@Test("playback reports duration for a generated audio fixture")
@MainActor
func playbackLoadsGeneratedAudio() throws {
    let fixtureURL = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
        .appendingPathExtension("caf")
    defer { try? FileManager.default.removeItem(at: fixtureURL) }

    let format = try #require(
        AVAudioFormat(standardFormatWithSampleRate: 8_000, channels: 1)
    )
    let buffer = try #require(
        AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 800)
    )
    buffer.frameLength = 800
    let samples = try #require(buffer.floatChannelData?[0])
    for index in 0..<Int(buffer.frameLength) {
        samples[index] = sin(Float(index) * 2 * .pi * 440 / 8_000) * 0.1
    }
    do {
        let file = try AVAudioFile(forWriting: fixtureURL, settings: format.settings)
        try file.write(from: buffer)
    }

    let playback = try PlaybackEngine(data: Data(contentsOf: fixtureURL))
    #expect(playback.duration > 0.09)
    #expect(playback.duration < 0.11)
    #expect(!playback.isPlaying)
    #expect(playback.currentTime == 0)
}

@Test("playback teardown deterministically invalidates its repeating timer")
@MainActor
func playbackTeardownInvalidatesTimer() throws {
    let fixtureURL = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString)
        .appendingPathExtension("caf")
    defer { try? FileManager.default.removeItem(at: fixtureURL) }

    let format = try #require(
        AVAudioFormat(standardFormatWithSampleRate: 8_000, channels: 1)
    )
    let buffer = try #require(
        AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 8_000)
    )
    buffer.frameLength = 8_000
    let samples = try #require(buffer.floatChannelData?[0])
    for index in 0..<Int(buffer.frameLength) {
        samples[index] = sin(Float(index) * 2 * .pi * 440 / 8_000) * 0.1
    }
    do {
        let file = try AVAudioFile(forWriting: fixtureURL, settings: format.settings)
        try file.write(from: buffer)
    }

    var playback: PlaybackEngine? = try PlaybackEngine(data: Data(contentsOf: fixtureURL))
    playback?.startUpdatesForTesting()
    let timer = try #require(playback?.updateTimerForTesting)
    weak let releasedPlayback = playback

    playback = nil

    #expect(releasedPlayback == nil)
    #expect(!timer.isValid)
}
