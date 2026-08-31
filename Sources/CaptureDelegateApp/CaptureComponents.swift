import CaptureDelegateCore
import SwiftUI

/// A live input-level meter driven by the engine's real metering. A single honest bar — never a
/// decorative or fabricated waveform. Colour alone is never load-bearing: a textual
/// "Receiving audio" / "Silent" alternative always accompanies it.
struct CaptureLevelMeter: View {
    let level: Float
    let isReceiving: Bool
    let isActive: Bool

    private var fill: Color { isActive ? .red : .secondary }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            GeometryReader { proxy in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(Color.secondary.opacity(0.15))
                    Capsule()
                        .fill(fill.opacity(isActive ? 0.9 : 0.4))
                        .frame(width: proxy.size.width * CGFloat(min(max(level, 0), 1)))
                }
            }
            .frame(height: 6)
            .accessibilityElement()
            .accessibilityLabel("Input level")
            .accessibilityValue(isReceiving ? "Receiving audio" : "Silent")

            Label(
                isReceiving ? "Receiving audio" : "Silent",
                systemImage: isReceiving ? "waveform" : "waveform.slash"
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            .accessibilityHidden(true)
        }
    }
}

/// A small red dot that gently pulses while recording, and holds steady when Reduce Motion is on.
/// Purely decorative; it is marked hidden from accessibility because the state text carries meaning.
struct RecordingDot: View {
    var diameter: CGFloat = 10
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var dimmed = false

    var body: some View {
        Circle()
            .fill(Color.red)
            .frame(width: diameter, height: diameter)
            .opacity(reduceMotion ? 1 : (dimmed ? 0.35 : 1))
            .animation(
                reduceMotion
                    ? nil : .easeInOut(duration: 0.9).repeatForever(autoreverses: true),
                value: dimmed
            )
            .onAppear { if !reduceMotion { dimmed = true } }
            .accessibilityHidden(true)
    }
}

/// The textual + symbolic identity of the current capture state. Every state is text plus a
/// distinct SF Symbol, so it stays unambiguous in grayscale and under VoiceOver.
struct CaptureStateBadge: View {
    let state: CaptureEngineState
    let isSaving: Bool

    private var symbol: String {
        if isSaving { return "arrow.triangle.2.circlepath" }
        switch state {
        case .recording: return "record.circle.fill"
        case .paused: return "pause.circle.fill"
        case .idle: return "circle"
        }
    }

    private var text: String {
        if isSaving { return "Saving" }
        switch state {
        case .recording: return "Recording"
        case .paused: return "Paused"
        case .idle: return "Idle"
        }
    }

    private var tint: Color {
        if case .recording = state, !isSaving { return .red }
        return .secondary
    }

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: symbol)
                .foregroundStyle(tint)
                .imageScale(.medium)
            Text(text)
                .font(.headline)
                .foregroundStyle(.primary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(text)
    }
}

/// A single row in Today's recents and the Moments list: display title plus a compact meta line.
struct MomentRow: View {
    let session: CaptureSession

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(SessionDisplay.title(session.title))
                .font(.body)
                .foregroundStyle(.primary)
                .lineLimit(1)
            HStack(spacing: 6) {
                Text(Formatting.timestamp(session.createdAt))
                Text("·")
                Text(Formatting.timer(session.duration))
                Text("·")
                Label(session.source, systemImage: "mic")
                    .labelStyle(.titleOnly)
            }
            .font(.caption)
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        .padding(.vertical, 2)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(SessionDisplay.title(session.title))
        .accessibilityValue(
            "\(Formatting.timestamp(session.createdAt)), "
                + "\(Formatting.spokenDuration(session.duration)), \(session.source)")
    }
}
