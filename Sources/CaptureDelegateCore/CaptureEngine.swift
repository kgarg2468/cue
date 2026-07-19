import AVFoundation
import Combine
import Darwin
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
    private struct RecordingOwner {
        let processIdentifier: Int32
        let processInstanceIdentity: String
    }

    @Published public private(set) var state: CaptureEngineState = .idle
    @Published public private(set) var elapsed: TimeInterval = 0
    @Published public private(set) var level: Float = 0
    @Published public private(set) var isReceivingAudio = false

    private let authorizationProvider: () -> MicrophoneAuthorization
    private let recorderFactory: (URL) throws -> any AudioRecording
    private let wallClock: () -> Date
    private let monotonicClock: () -> TimeInterval
    private let schedulesUpdates: Bool
    private let recordingDirectory: URL
    private let processIdentifier: Int32
    private let processLivenessProvider: (Int32) -> Bool
    private let processInstanceIdentity: String
    private let processInstanceIdentityProvider: (Int32) -> String?

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
            schedulesUpdates: true,
            recordingDirectory: Self.defaultRecordingDirectory()
        )
    }

    init(
        authorizationProvider: @escaping () -> MicrophoneAuthorization,
        recorderFactory: @escaping (URL) throws -> any AudioRecording,
        wallClock: @escaping () -> Date,
        monotonicClock: @escaping () -> TimeInterval,
        schedulesUpdates: Bool,
        recordingDirectory: URL = CaptureEngine.defaultRecordingDirectory(),
        processIdentifier: Int32 = ProcessInfo.processInfo.processIdentifier,
        processLivenessProvider: @escaping (Int32) -> Bool = CaptureEngine.isProcessRunning(_:),
        processInstanceIdentity: String? = nil,
        processInstanceIdentityProvider: @escaping (Int32) -> String? =
            CaptureEngine.processStartIdentity(_:)
    ) {
        self.authorizationProvider = authorizationProvider
        self.recorderFactory = recorderFactory
        self.wallClock = wallClock
        self.monotonicClock = monotonicClock
        self.schedulesUpdates = schedulesUpdates
        self.recordingDirectory = recordingDirectory
        self.processIdentifier = processIdentifier
        self.processLivenessProvider = processLivenessProvider
        self.processInstanceIdentityProvider = processInstanceIdentityProvider
        self.processInstanceIdentity =
            processInstanceIdentity
            ?? processInstanceIdentityProvider(processIdentifier)
            ?? UUID().uuidString.replacingOccurrences(of: "-", with: "")
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

        do {
            try FileManager.default.createDirectory(
                at: recordingDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: recordingDirectory.path
            )
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }

        let fileURL =
            recordingDirectory
            .appendingPathComponent(
                "process-\(processIdentifier)-\(processInstanceIdentity)-\(UUID().uuidString).m4a"
            )
        let newRecorder: any AudioRecording
        do {
            newRecorder = try recorderFactory(fileURL)
        } catch {
            let factoryError = error
            do {
                try removeTemporaryFileIfPresent(at: fileURL)
            } catch {
                throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
            }
            if let factoryError = factoryError as? CaptureEngineError {
                throw factoryError
            }
            throw CaptureEngineError.audioSessionFailure(factoryError.localizedDescription)
        }
        newRecorder.isMeteringEnabled = true
        guard newRecorder.record() else {
            do {
                try removeTemporaryFileIfPresent(at: fileURL)
            } catch {
                throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
            }
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

    /// Files with this prefix are finalized recordings that an unresolved save failure still
    /// owns. They may be the only copy of the user's audio, so reconciliation never removes
    /// them — not even when their original owning process is gone.
    public static let heldRecordingPrefix = "held-"

    /// Move a finalized recording out of reconciliation's reach by renaming it with
    /// ``heldRecordingPrefix``. Returns the recording's new URL; already-held files are
    /// returned unchanged.
    public static func markRecordingHeld(at url: URL) throws -> URL {
        guard !url.lastPathComponent.hasPrefix(heldRecordingPrefix) else { return url }
        let heldURL = url.deletingLastPathComponent()
            .appendingPathComponent(heldRecordingPrefix + url.lastPathComponent)
        do {
            try FileManager.default.moveItem(at: url, to: heldURL)
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }
        return heldURL
    }

    public func reconcileStaleRecordings() throws {
        let fileManager = FileManager.default
        guard fileManager.fileExists(atPath: recordingDirectory.path) else {
            return
        }
        do {
            let entries = try fileManager.contentsOfDirectory(
                at: recordingDirectory,
                includingPropertiesForKeys: [.isRegularFileKey],
                options: []
            )
            for entry in entries {
                let values = try entry.resourceValues(forKeys: [.isRegularFileKey])
                guard values.isRegularFile == true else { continue }
                if entry.lastPathComponent.hasPrefix(Self.heldRecordingPrefix) { continue }
                if let owner = Self.recordingOwner(for: entry) {
                    if owner.processIdentifier == processIdentifier,
                        owner.processInstanceIdentity == processInstanceIdentity
                    {
                        continue
                    }
                    if processLivenessProvider(owner.processIdentifier) {
                        guard
                            let liveIdentity = processInstanceIdentityProvider(
                                owner.processIdentifier
                            )
                        else {
                            continue
                        }
                        if liveIdentity == owner.processInstanceIdentity {
                            continue
                        }
                    }
                }
                try fileManager.removeItem(at: entry)
            }
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }
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
        resetState()
        return (recordingURL, captureStartedAt, duration)
    }

    public func discard() throws {
        recorder?.stop()
        let fileToDelete = recordingURL
        resetState()
        if let fileToDelete {
            try removeTemporaryFileIfPresent(at: fileToDelete)
        }
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

    private static func defaultRecordingDirectory() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("CaptureDelegateRecordings", isDirectory: true)
    }

    private nonisolated static func isProcessRunning(_ processIdentifier: Int32) -> Bool {
        guard processIdentifier > 0 else { return false }
        if Darwin.kill(processIdentifier, 0) == 0 {
            return true
        }
        return errno == EPERM
    }

    private nonisolated static func processStartIdentity(
        _ processIdentifier: Int32
    ) -> String? {
        guard processIdentifier > 0 else { return nil }
        var info = proc_bsdinfo()
        let expectedSize = Int32(MemoryLayout<proc_bsdinfo>.size)
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            proc_pidinfo(
                processIdentifier,
                PROC_PIDTBSDINFO,
                0,
                pointer,
                expectedSize
            )
        }
        guard result == expectedSize else { return nil }
        return "\(info.pbi_start_tvsec)x\(info.pbi_start_tvusec)"
    }

    private nonisolated static func recordingOwner(for url: URL) -> RecordingOwner? {
        let stem = url.deletingPathExtension().lastPathComponent
        let prefix = "process-"
        guard stem.hasPrefix(prefix) else { return nil }
        let components = stem.dropFirst(prefix.count).split(
            separator: "-",
            maxSplits: 2,
            omittingEmptySubsequences: false
        )
        guard
            components.count == 3,
            let processIdentifier = Int32(components[0]),
            processIdentifier > 0,
            !components[1].isEmpty,
            UUID(uuidString: String(components[2])) != nil
        else {
            return nil
        }
        return RecordingOwner(
            processIdentifier: processIdentifier,
            processInstanceIdentity: String(components[1])
        )
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

    private func resetState() {
        stopUpdates()
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
    }

    private func removeTemporaryFileIfPresent(at url: URL) throws {
        let fileManager = FileManager.default
        guard fileManager.fileExists(atPath: url.path) else { return }
        do {
            try fileManager.removeItem(at: url)
        } catch {
            throw CaptureEngineError.audioSessionFailure(error.localizedDescription)
        }
    }
}
