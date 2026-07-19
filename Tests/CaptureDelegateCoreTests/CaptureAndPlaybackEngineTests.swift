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

    engine.discard()
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
