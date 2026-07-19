import AVFAudio
import Combine
import Foundation

public enum PlaybackEngineError: Error, Equatable {
    case corruptData
}

@MainActor
public final class PlaybackEngine: NSObject, ObservableObject, AVAudioPlayerDelegate {
    @Published public private(set) var isPlaying = false
    @Published public private(set) var currentTime: TimeInterval = 0

    public var onFinish: (() -> Void)?

    private let player: AVAudioPlayer
    private var updateTimer: Timer?

    var updateTimerForTesting: Timer? { updateTimer }

    func startUpdatesForTesting() {
        startUpdates()
    }

    public init(data: Data) throws {
        do {
            player = try AVAudioPlayer(data: data)
        } catch {
            throw PlaybackEngineError.corruptData
        }
        super.init()
        player.prepareToPlay()
        player.delegate = self
    }

    isolated deinit {
        updateTimer?.invalidate()
    }

    public var duration: TimeInterval {
        player.duration
    }

    public func play() {
        guard player.play() else {
            isPlaying = false
            return
        }
        isPlaying = true
        startUpdates()
    }

    public func pause() {
        player.pause()
        currentTime = player.currentTime
        isPlaying = false
        stopUpdates()
    }

    public func seek(to time: TimeInterval) {
        player.currentTime = min(max(0, time), duration)
        currentTime = player.currentTime
    }

    public nonisolated func audioPlayerDidFinishPlaying(
        _ player: AVAudioPlayer,
        successfully flag: Bool
    ) {
        Task { @MainActor [weak self] in
            self?.playbackFinished()
        }
    }

    private func startUpdates() {
        stopUpdates()
        updateTimer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) {
            [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else {
                    return
                }
                self.currentTime = self.player.currentTime
                if !self.player.isPlaying {
                    self.isPlaying = false
                    self.stopUpdates()
                }
            }
        }
    }

    private func stopUpdates() {
        updateTimer?.invalidate()
        updateTimer = nil
    }

    private func playbackFinished() {
        stopUpdates()
        isPlaying = false
        currentTime = duration
        onFinish?()
    }
}
