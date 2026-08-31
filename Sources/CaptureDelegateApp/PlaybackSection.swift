import CaptureDelegateCore
import SwiftUI

/// Loads a session's audio (decrypted to memory), builds a `PlaybackEngine`, and hosts the
/// controls. A missing, corrupt, or undecryptable recording shows the exact contract copy instead
/// of a broken control.
struct PlaybackSection: View {
    @ObservedObject var model: AppModel
    let sessionID: UUID

    private enum LoadState {
        case loading
        case ready(PlaybackEngine)
        case failed
    }

    @State private var loadState: LoadState = .loading

    var body: some View {
        Group {
            switch loadState {
            case .loading:
                HStack(spacing: 10) {
                    ProgressView().controlSize(.small)
                    Text("Opening recording…")
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
            case .ready(let engine):
                PlaybackControls(engine: engine)
            case .failed:
                Label("This recording couldn't be opened.", systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityLabel("This recording couldn't be opened.")
            }
        }
        .task(id: sessionID) { await load() }
    }

    private func load() async {
        loadState = .loading
        do {
            let data = try await model.loadAudioData(for: sessionID)
            let engine = try PlaybackEngine(data: data)
            loadState = .ready(engine)
        } catch {
            loadState = .failed
        }
    }
}

/// The transport: a play/pause button plus a labelled native slider with current/total time.
/// When focused, Space toggles playback and Left/Right scrub five seconds.
private struct PlaybackControls: View {
    @ObservedObject var engine: PlaybackEngine

    @State private var scrubValue: Double = 0
    @State private var isScrubbing = false
    @FocusState private var focused: Bool

    private var total: Double { max(engine.duration, 0.01) }

    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 16) {
                Button {
                    toggle()
                } label: {
                    Image(systemName: engine.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                        .font(.system(size: 40))
                        .symbolRenderingMode(.hierarchical)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(engine.isPlaying ? "Pause" : "Play")
                .accessibilityHint("Toggles playback of this recording")

                VStack(spacing: 4) {
                    Slider(value: $scrubValue, in: 0...total) { editing in
                        isScrubbing = editing
                        if !editing { engine.seek(to: scrubValue) }
                    }
                    .accessibilityLabel("Playback position")
                    .accessibilityValue(Formatting.spokenDuration(scrubValue))

                    HStack {
                        Text(Formatting.timer(engine.currentTime))
                        Spacer()
                        Text(Formatting.timer(engine.duration))
                    }
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                }
            }
        }
        .padding(16)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(Color.accentColor.opacity(focused ? 0.8 : 0), lineWidth: 2)
        )
        .focusable()
        .focused($focused)
        .onKeyPress(.space) {
            toggle()
            return .handled
        }
        .onKeyPress(.leftArrow) {
            engine.seek(to: engine.currentTime - 5)
            return .handled
        }
        .onKeyPress(.rightArrow) {
            engine.seek(to: engine.currentTime + 5)
            return .handled
        }
        .accessibilityElement(children: .contain)
        .onAppear { scrubValue = engine.currentTime }
        .onChange(of: engine.currentTime) { _, newValue in
            if !isScrubbing { scrubValue = newValue }
        }
    }

    private func toggle() {
        if engine.isPlaying {
            engine.pause()
        } else {
            engine.play()
        }
    }
}
