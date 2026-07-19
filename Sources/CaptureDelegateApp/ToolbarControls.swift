import CaptureDelegateCore
import SwiftUI

/// The toolbar's capture affordance. Idle: a prominent "New Capture" button. Active: a live chip
/// with distinct recording/paused text, a monospaced timer, and jump-to-live behaviour.
struct ToolbarCaptureControl: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    var body: some View {
        if model.isCaptureActive {
            Button {
                model.jumpToLiveCapture()
            } label: {
                HStack(spacing: 7) {
                    if model.isRecording, !model.isSaving {
                        RecordingDot(diameter: 8)
                    } else {
                        Image(systemName: chipSymbol)
                            .foregroundStyle(.secondary)
                    }
                    Text(chipText)
                        .font(.callout.weight(.medium))
                    Text(Formatting.timer(engine.elapsed))
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
            }
            .accessibilityLabel("\(chipText), \(Formatting.spokenDuration(engine.elapsed))")
            .accessibilityHint("Jumps to the live capture")
        } else {
            Button {
                model.requestStartCapture()
            } label: {
                Label("New Capture", systemImage: "record.circle")
            }
            .buttonStyle(.bordered)
            .accessibilityLabel("New capture")
            .accessibilityHint("Starts a new microphone recording")
        }
    }

    private var chipText: String {
        if model.isSaving { return "Saving" }
        return model.isPaused ? "Paused" : "Recording"
    }

    private var chipSymbol: String {
        if model.isSaving { return "arrow.triangle.2.circlepath" }
        return model.isPaused ? "pause.circle.fill" : "record.circle.fill"
    }
}

/// The real, informational runtime-health item. It never gates capture; it reports the local
/// runtime and, in a popover, explains that recording, notes, and playback do not need it.
struct RuntimeStatusControl: View {
    @ObservedObject var model: AppModel
    @State private var showPopover = false

    var body: some View {
        Button {
            showPopover.toggle()
        } label: {
            HStack(spacing: 5) {
                Image(systemName: model.runtimeOnline ? "bolt.horizontal.circle" : "bolt.slash")
                    .foregroundStyle(.secondary)
                Text(model.runtimeOnline ? "Runtime · Online" : "Runtime · Offline")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel(model.runtimeOnline ? "Runtime online" : "Runtime offline")
        .accessibilityHint("Explains the local runtime status")
        .popover(isPresented: $showPopover, arrowEdge: .bottom) {
            runtimeExplanation
        }
    }

    private var runtimeExplanation: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(
                model.runtimeOnline ? "Runtime · Online" : "Runtime · Offline",
                systemImage: model.runtimeOnline ? "bolt.horizontal.circle" : "bolt.slash"
            )
            .font(.headline)
            Text(
                model.runtimeOnline
                    ? "The local runtime is reachable. It will host agent delegation in a later "
                        + "milestone."
                    : "The local runtime isn't running. That's fine — recording, notes, and "
                        + "playback all work without it. It will host agent delegation later."
            )
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        .padding(16)
        .frame(width: 300)
    }
}
