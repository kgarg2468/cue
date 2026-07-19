import AVFoundation
import Combine
import Foundation

public enum CaptureEngineState: Equatable, Sendable {
    case idle
    case recording(startedAt: Date)
    case paused
}

public enum CaptureEngineError: Error, Equatable {
    case permissionDenied
    case audioSessionFailure(String)
    case notRecording
    case alreadyRecording
}

public enum MicrophoneAuthorization: Equatable, Sendable {
    case undetermined
    case authorized
    case denied
    case restricted
}

@MainActor
protocol AudioRecording: AnyObject {
    var isMeteringEnabled: Bool { get set }
    func record() -> Bool
    func pause()
    func stop()
    func updateMeters()
    func averagePower(forChannel channelNumber: Int) -> Float
}

extension AVAudioRecorder: AudioRecording {}

@MainActor
public final class CaptureEngine: ObservableObject {
    @Published public private(set) var state: CaptureEngineState = .idle
    @Published public private(set) var elapsed: TimeInterval = 0
    @Published public private(set) var level: Float = 0
    @Published public private(set) var isReceivingAudio = false

    private let authorizationProvider: () -> MicrophoneAuthorization
    private let recorderFactory: (URL) throws -> any AudioRecording
    private let wallClock: () -> Date
    private let monotonicClock: () -> TimeInterval
    private let schedulesUpdates: Bool

    private var recorder: (any AudioRecording)?
    private var recordingURL: URL?
    private var captureStartedAt: Date?
    private var activeIntervalStartedAt: TimeInterval?
    private var accumulatedDuration: TimeInterval = 0
    private var recentPowerSamples: [Float] = []
    private var updateTimer: Timer?

    public convenience init() {
        self.init(
            authorizationProvider: Self.currentAuthorization,
            recorderFactory: Self.makeRecorder(at:),
            wallClock: Date.init,
            monotonicClock: { ProcessInfo.processInfo.systemUptime },
            schedulesUpdates: true
        )
    }

    init(
        authorizationProvider: @escaping () -> MicrophoneAuthorization,
        recorderFactory: @escaping (URL) throws -> any AudioRecording,
        wallClock: @escaping () -> Date,
        monotonicClock: @escaping () -> TimeInterval,
        schedulesUpdates: Bool
    ) {
        self.authorizationProvider = authorizationProvider
        self.recorderFactory = recorderFactory
        self.wallClock = wallClock
        self.monotonicClock = monotonicClock
        self.schedulesUpdates = schedulesUpdates
    }

    public nonisolated static func currentAuthorization() -> MicrophoneAuthorization {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .notDetermined:
            .undetermined
        case .authorized:
            .authorized
        case .denied:
            .denied
        case .restricted:
            .restricted
        @unknown default:
            .restricted
        }
    }

    public nonisolated static func requestAccess(
        completion: @escaping @MainActor (Bool) -> Void
    ) {
        AVCaptureDevice.requestAccess(for: .audio) { granted in
            Task { @MainActor in
                completion(granted)
            }
        }
    }

    public func start() throws {
        guard state == .idle else {
            throw CaptureEngineError.alreadyRecording
        }
        guard authorizationProvider() == .authorized else {
            throw CaptureEngineError.permissionDenied
        }

        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("CaptureDelegateRecordings", isDirectory: true)
        do {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: directory.path
            )
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }

        let fileURL =
            directory
            .appendingPathComponent(UUID().uuidString)
            .appendingPathExtension("m4a")
        let newRecorder: any AudioRecording
        do {
            newRecorder = try recorderFactory(fileURL)
        } catch let error as CaptureEngineError {
            throw error
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }
        newRecorder.isMeteringEnabled = true
        guard newRecorder.record() else {
            try? FileManager.default.removeItem(at: fileURL)
            throw CaptureEngineError.audioSessionFailure(
                "AVAudioRecorder failed to start recording"
            )
        }

        let startedAt = wallClock()
        recorder = newRecorder
        recordingURL = fileURL
        captureStartedAt = startedAt
        accumulatedDuration = 0
        activeIntervalStartedAt = monotonicClock()
        recentPowerSamples.removeAll(keepingCapacity: true)
        elapsed = 0
        level = 0
        isReceivingAudio = false
        state = .recording(startedAt: startedAt)
        scheduleUpdates()
    }

    public func pause() throws {
        guard case .recording = state, let recorder else {
            throw CaptureEngineError.notRecording
        }
        accumulateActiveInterval()
        recorder.pause()
        state = .paused
        stopUpdates()
        elapsed = accumulatedDuration
        level = 0
        isReceivingAudio = false
        recentPowerSamples.removeAll(keepingCapacity: true)
    }

    public func resume() throws {
        guard state == .paused, let recorder, let startedAt = captureStartedAt else {
            throw CaptureEngineError.notRecording
        }
        guard recorder.record() else {
            throw CaptureEngineError.audioSessionFailure(
                "AVAudioRecorder failed to resume recording"
            )
        }
        activeIntervalStartedAt = monotonicClock()
        state = .recording(startedAt: startedAt)
        scheduleUpdates()
    }

    public func stop() throws -> (
        audioFileURL: URL, createdAt: Date, duration: TimeInterval
    ) {
        guard
            state != .idle,
            let recorder,
            let recordingURL,
            let captureStartedAt
        else {
            throw CaptureEngineError.notRecording
        }
        if case .recording = state {
            accumulateActiveInterval()
        }
        recorder.stop()
        let duration = accumulatedDuration
        resetState(deleteTemporaryFile: false)
        return (recordingURL, captureStartedAt, duration)
    }

    public func discard() {
        recorder?.stop()
        resetState(deleteTemporaryFile: true)
    }

    private static func makeRecorder(at url: URL) throws -> any AudioRecording {
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 44_100,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ]
        let recorder = try AVAudioRecorder(url: url, settings: settings)
        guard recorder.prepareToRecord() else {
            throw CaptureEngineError.audioSessionFailure(
                "AVAudioRecorder could not prepare the recording file"
            )
        }
        return recorder
    }

    private func scheduleUpdates() {
        guard schedulesUpdates else {
            return
        }
        stopUpdates()
        updateTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) {
            [weak self] _ in
            Task { @MainActor [weak self] in
                self?.updateMeasurements()
            }
        }
    }

    private func stopUpdates() {
        updateTimer?.invalidate()
        updateTimer = nil
    }

    private func updateMeasurements() {
        guard case .recording = state, let recorder else {
            return
        }
        elapsed =
            accumulatedDuration
            + max(0, monotonicClock() - (activeIntervalStartedAt ?? monotonicClock()))

        recorder.updateMeters()
        let averagePower = recorder.averagePower(forChannel: 0)

        // AVAudioRecorder reports dBFS. Values at or below -60 dBFS map to 0,
        // 0 dBFS maps to 1, and values between are linearly normalized.
        let clampedPower = min(0, max(-60, averagePower))
        level = (clampedPower + 60) / 60

        recentPowerSamples.append(averagePower)
        if recentPowerSamples.count > 10 {
            recentPowerSamples.removeFirst(recentPowerSamples.count - 10)
        }
        let rollingAverage =
            recentPowerSamples.reduce(0, +)
            / Float(recentPowerSamples.count)
        // A rolling one-second average above -45 dBFS is treated as real input.
        isReceivingAudio = rollingAverage > -45
    }

    private func accumulateActiveInterval() {
        guard let activeIntervalStartedAt else {
            return
        }
        accumulatedDuration += max(0, monotonicClock() - activeIntervalStartedAt)
        self.activeIntervalStartedAt = nil
    }

    private func resetState(deleteTemporaryFile: Bool) {
        stopUpdates()
        let fileToDelete = recordingURL
        recorder = nil
        recordingURL = nil
        captureStartedAt = nil
        activeIntervalStartedAt = nil
        accumulatedDuration = 0
        recentPowerSamples.removeAll(keepingCapacity: false)
        state = .idle
        elapsed = 0
        level = 0
        isReceivingAudio = false

        if deleteTemporaryFile, let fileToDelete {
            try? FileManager.default.removeItem(at: fileToDelete)
        }
    }
}
