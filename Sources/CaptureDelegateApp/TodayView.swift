import CaptureDelegateCore
import SwiftUI

/// The default destination: a calm, centered editorial column. The saved moment — not the engine —
/// is the centre of the product, so the strong capture card and recents lead; empty sections are
/// simply omitted.
struct TodayView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 28) {
                header
                StrongCaptureCard(model: model)
                if model.isCaptureActive {
                    ActiveCaptureRow(model: model)
                }
                recents
                if let message = model.loadErrorMessage {
                    LoadErrorNotice(message: message)
                }
            }
            .frame(maxWidth: 680, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 36)
        }
        .navigationTitle("Today")
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(Formatting.greeting())
                .font(.largeTitle.weight(.semibold))
            Text("Capture Delegate")
                .font(.title3)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private var recents: some View {
        if !model.recentSessions.isEmpty {
            VStack(alignment: .leading, spacing: 10) {
                Text("Recent moments")
                    .font(.headline)
                    .foregroundStyle(.secondary)
                VStack(spacing: 0) {
                    ForEach(model.recentSessions) { session in
                        Button {
                            model.open(session)
                        } label: {
                            MomentRow(session: session)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .padding(.vertical, 8)
                        .accessibilityHint("Opens this capture")
                        if session.id != model.recentSessions.last?.id {
                            Divider()
                        }
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 4)
                .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
            }
        }
    }
}

/// The one strong card. Its headline doubles as Today's empty-state copy, so an empty Mac still
/// reads as an invitation rather than a void.
private struct StrongCaptureCard: View {
    @ObservedObject var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Capture something worth keeping.")
                    .font(.title2.weight(.semibold))
                Text("Record a meeting, a conversation, a presentation, or a passing idea.")
                    .font(.body)
                    .foregroundStyle(.secondary)
                Label("It stays on this Mac, encrypted.", systemImage: "lock")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Button {
                model.requestStartCapture()
            } label: {
                Label("Start capture", systemImage: "record.circle")
                    .frame(maxWidth: .infinity)
            }
            .controlSize(.large)
            .buttonStyle(.borderedProminent)
            .disabled(model.isCaptureActive)
            .accessibilityHint("Begins a new microphone recording")
        }
        .padding(24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 16))
    }
}

/// Shown only while a capture is in progress: honest state, live timer, and a jump-to-live action.
private struct ActiveCaptureRow: View {
    @ObservedObject var model: AppModel

    var body: some View {
        Button {
            model.jumpToLiveCapture()
        } label: {
            HStack(spacing: 12) {
                CaptureStateBadge(state: model.engine.state, isSaving: model.isSaving)
                Spacer()
                Text(Formatting.timer(model.engine.elapsed))
                    .font(.body.monospacedDigit())
                    .foregroundStyle(.secondary)
                Image(systemName: "chevron.right")
                    .font(.footnote)
                    .foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 14)
            .frame(maxWidth: .infinity)
            .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: 12))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityHint("Opens the live capture")
    }
}

private struct LoadErrorNotice: View {
    let message: String

    var body: some View {
        Label(message, systemImage: "exclamationmark.triangle")
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 10))
    }
}
