import CaptureDelegateCore
import SwiftUI

/// The menu bar icon. A neutral idle shape and a distinct filled recording shape — never tint alone.
struct MenuBarLabelView: View {
    @ObservedObject var engine: CaptureEngine

    var body: some View {
        Image(systemName: symbol)
            .accessibilityLabel(label)
    }

    private var symbol: String {
        switch engine.state {
        case .idle: "waveform.circle"
        case .recording: "record.circle.fill"
        case .paused: "pause.circle.fill"
        }
    }

    private var label: String {
        switch engine.state {
        case .idle: "Capture Delegate, idle"
        case .recording: "Capture Delegate, recording"
        case .paused: "Capture Delegate, paused"
        }
    }
}

/// The menu bar popover. Idle and active shapes both carry only real, working controls.
struct MenuBarContentView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if model.isCaptureActive {
                activeContent
            } else {
                idleContent
            }
        }
        .padding(16)
        .frame(width: 300)
    }

    // MARK: Idle

    private var idleContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text("Capture Delegate")
                    .font(.headline)
                Text("Records from the microphone")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Button {
                openMainWindow()
                model.requestStartCapture()
            } label: {
                Label("Start Capture", systemImage: "record.circle")
                    .frame(maxWidth: .infinity)
            }
            .controlSize(.large)
            .buttonStyle(.borderedProminent)
            .accessibilityHint("Starts a new microphone recording")

            recents

            Divider()

            Button("Open Main Window") { openMainWindow() }
                .buttonStyle(.plain)
        }
    }

    @ViewBuilder private var recents: some View {
        Divider()
        if model.sessions.isEmpty {
            Text("No captures yet")
                .font(.callout)
                .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                ForEach(model.sessions.prefix(3)) { session in
                    Button {
                        openMainWindow()
                        model.open(session)
                    } label: {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(SessionDisplay.title(session.title))
                                .foregroundStyle(.primary)
                                .lineLimit(1)
                            Text(
                                "\(Formatting.timestamp(session.createdAt)) · "
                                    + Formatting.timer(session.duration)
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityHint("Opens this capture")
                }
            }
        }
    }

    // MARK: Active

    private var activeContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                CaptureStateBadge(state: engine.state, isSaving: model.isSaving)
                Spacer()
                Text(Formatting.timer(engine.elapsed))
                    .font(.body.monospacedDigit())
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(model.draftTitle.isEmpty ? SessionDisplay.untitled : model.draftTitle)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Label("Microphone", systemImage: "mic")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            HStack(spacing: 8) {
                Button {
                    model.togglePauseResume()
                } label: {
                    Label(
                        model.isPaused ? "Resume" : "Pause",
                        systemImage: model.isPaused ? "play.fill" : "pause.fill"
                    )
                    .frame(maxWidth: .infinity)
                }
                .disabled(model.isSaving)

                Button(role: .destructive) {
                    model.stopAndSave()
                } label: {
                    Label("Stop", systemImage: "stop.fill")
                        .frame(maxWidth: .infinity)
                }
                .disabled(model.isSaving)
            }

            Divider()

            Button("Open Main Window") {
                openMainWindow()
                model.jumpToLiveCapture()
            }
            .buttonStyle(.plain)
        }
    }

    private func openMainWindow() {
        openWindow(id: CaptureDelegateWindowID.main)
        NSApplication.shared.activate(ignoringOtherApps: true)
    }
}
