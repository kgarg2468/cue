import CaptureDelegateCore
import SwiftUI

/// State 2 — the pre-permission explainer, shown before the system prompt so the ask is never a
/// surprise. Honest about what recording uses and where the audio lives.
struct PermissionExplainerView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        SheetScaffold(
            symbol: "mic",
            symbolTint: .secondary,
            title: "Capture needs your microphone"
        ) {
            VStack(alignment: .leading, spacing: 10) {
                Label(
                    "Recording uses the microphone on this Mac.",
                    systemImage: "waveform")
                Label(
                    "Your audio stays here, encrypted — it is never uploaded.",
                    systemImage: "lock")
            }
            .font(.callout)
            .foregroundStyle(.secondary)
        } actions: {
            Button("Not now") { model.cancelPermissionFlow() }
                .keyboardShortcut(.cancelAction)
                .accessibilityHint("Dismisses without recording")
            Button("Continue") { model.confirmPermissionExplainer() }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .accessibilityHint("Asks macOS for microphone access")
        }
    }
}

/// State 3 — microphone access is off or restricted. A truthful System Settings path plus a working
/// button to open it.
struct PermissionDeniedView: View {
    @ObservedObject var model: AppModel

    private var restricted: Bool { model.deniedAuthorization == .restricted }

    var body: some View {
        SheetScaffold(
            symbol: "mic.slash",
            symbolTint: .secondary,
            title: "Microphone access is off"
        ) {
            VStack(alignment: .leading, spacing: 10) {
                Text(
                    restricted
                        ? "Microphone access is restricted on this Mac, so Capture Delegate can't "
                            + "record. A device administrator may control this."
                        : "Capture Delegate can't record until you allow the microphone."
                )
                Label(
                    "Open System Settings → Privacy & Security → Microphone, then turn on "
                        + "Capture Delegate.",
                    systemImage: "gearshape")
            }
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        } actions: {
            Button("Cancel") { model.cancelPermissionFlow() }
                .keyboardShortcut(.cancelAction)
            Button("Open System Settings") {
                model.openSystemMicrophoneSettings()
                model.cancelPermissionFlow()
            }
            .keyboardShortcut(.defaultAction)
            .buttonStyle(.borderedProminent)
            .accessibilityHint("Opens the Microphone privacy settings")
        }
    }
}

/// State 10 — secure save failed. Names the real cause, tells the truth about where the recording
/// currently lives (a private temporary file on this Mac, held until the encrypted save lands), and
/// offers working Try Again / Export audio / Discard. The finalized recording is never dropped
/// silently: the sheet cannot be dismissed without an explicit choice.
struct SaveFailureView: View {
    @ObservedObject var model: AppModel
    @State private var confirmDiscard = false

    var body: some View {
        SheetScaffold(
            symbol: "exclamationmark.lock",
            symbolTint: .primary,
            title: "This capture couldn't be saved securely"
        ) {
            VStack(alignment: .leading, spacing: 12) {
                Text(model.saveFailureMessage ?? "")
                    .font(.callout)
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                if model.exportConfirmationVisible {
                    Label(
                        "Audio exported. It still isn't saved in the app.",
                        systemImage: "tray.and.arrow.down"
                    )
                    .font(.callout)
                    .foregroundStyle(.secondary)
                }
            }
        } actions: {
            Button("Export audio…") { model.exportPendingAudio() }
                .accessibilityHint("Saves the unencrypted recording to a location you choose")
            Button("Discard", role: .destructive) { confirmDiscard = true }
                .accessibilityHint("Permanently deletes this recording")
            Button("Try Again") { model.retrySave() }
                .keyboardShortcut(.defaultAction)
                .buttonStyle(.borderedProminent)
                .accessibilityHint("Retries the secure save")
        }
        .interactiveDismissDisabled()
        .confirmationDialog(
            "Discard this recording?",
            isPresented: $confirmDiscard,
            titleVisibility: .visible
        ) {
            Button("Discard recording", role: .destructive) { model.discardPendingCapture() }
            Button("Keep trying", role: .cancel) {}
        } message: {
            Text("The recording will be permanently deleted and cannot be recovered.")
        }
    }
}

/// Shared layout for the capture-flow sheets: a symbol, a title, body content, and a trailing
/// action row.
private struct SheetScaffold<Body: View, Actions: View>: View {
    let symbol: String
    var symbolTint: Color = .secondary
    let title: String
    @ViewBuilder var content: () -> Body
    @ViewBuilder var actions: () -> Actions

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 12) {
                Image(systemName: symbol)
                    .font(.system(size: 28))
                    .foregroundStyle(symbolTint)
                Text(title)
                    .font(.title2.weight(.semibold))
                    .fixedSize(horizontal: false, vertical: true)
            }
            content()
            HStack {
                Spacer()
                actions()
            }
        }
        .padding(24)
        .frame(width: 460)
        .accessibilityElement(children: .contain)
    }
}
