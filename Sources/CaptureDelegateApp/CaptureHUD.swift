import AppKit
import CaptureDelegateCore
import SwiftUI

/// Owns the floating capture HUD: a compact, draggable `NSPanel` that floats above normal windows
/// and hosts a SwiftUI surface. Shown only while a capture is active.
@MainActor
final class CaptureHUDController {
    private var panel: NSPanel?

    func show(model: AppModel, engine: CaptureEngine) {
        if panel == nil {
            panel = makePanel(model: model, engine: engine)
        }
        panel?.orderFrontRegardless()
    }

    func hide() {
        panel?.orderOut(nil)
    }

    func close() {
        panel?.orderOut(nil)
        panel = nil
    }

    private func makePanel(model: AppModel, engine: CaptureEngine) -> NSPanel {
        let hosting = NSHostingView(rootView: CaptureHUDView(model: model, engine: engine))
        let panel = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 360, height: 56),
            styleMask: [.borderless, .nonactivatingPanel, .fullSizeContentView],
            backing: .buffered,
            defer: false)
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.hidesOnDeactivate = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.isMovableByWindowBackground = true
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = true
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = hosting
        position(panel)
        return panel
    }

    private func position(_ panel: NSPanel) {
        guard let screen = NSScreen.main else { return }
        let visible = screen.visibleFrame
        let size = panel.frame.size
        panel.setFrameOrigin(
            NSPoint(x: visible.midX - size.width / 2, y: visible.maxY - size.height - 24))
    }
}

/// The HUD's SwiftUI content: honest state, a monospaced timer, real input-level truth, and working
/// Pause/Resume and Stop. It never implies a transcript exists.
struct CaptureHUDView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    var body: some View {
        HStack(spacing: 12) {
            stateGlyph
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(stateText)
                        .font(.callout.weight(.semibold))
                    Text(Formatting.timer(engine.elapsed))
                        .font(.callout.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                miniMeter
            }
            Spacer(minLength: 4)
            controls
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .frame(width: 360, height: 56)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 14))
        .overlay(
            RoundedRectangle(cornerRadius: 14)
                .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
        )
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Capture controls")
    }

    @ViewBuilder private var stateGlyph: some View {
        if model.isRecording {
            RecordingDot(diameter: 12)
        } else {
            Image(systemName: "pause.circle.fill")
                .foregroundStyle(.secondary)
                .imageScale(.large)
        }
    }

    private var stateText: String {
        model.isPaused ? "Paused" : "Recording"
    }

    private var miniMeter: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Capsule().fill(Color.secondary.opacity(0.2))
                Capsule()
                    .fill(model.isRecording ? Color.red.opacity(0.9) : Color.secondary)
                    .frame(
                        width: proxy.size.width
                            * CGFloat(model.isPaused ? 0 : min(max(engine.level, 0), 1)))
            }
        }
        .frame(height: 4)
        .accessibilityLabel("Input level")
        .accessibilityValue(
            model.isRecording && engine.isReceivingAudio ? "Receiving audio" : "Silent")
    }

    private var controls: some View {
        HStack(spacing: 8) {
            Button {
                model.togglePauseResume()
            } label: {
                Image(systemName: model.isPaused ? "play.fill" : "pause.fill")
            }
            .accessibilityLabel(model.isPaused ? "Resume" : "Pause")

            Button {
                model.stopAndSave()
            } label: {
                Image(systemName: "stop.fill")
            }
            .accessibilityLabel("Stop and save")
        }
        .buttonStyle(.borderless)
    }
}
