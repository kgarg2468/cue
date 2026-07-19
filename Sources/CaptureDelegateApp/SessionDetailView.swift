import CaptureDelegateCore
import SwiftUI

/// The reader for one saved session (states 7 Saved and 8 Playback). Title and note are editable and
/// persisted; audio is decrypted to memory for playback and never written back to disk in plaintext.
struct SessionDetailView: View {
    @ObservedObject var model: AppModel
    let sessionID: UUID

    @State private var titleDraft = ""
    @State private var noteDraft = ""
    @State private var didSeed = false
    @State private var showDeleteConfirmation = false
    @FocusState private var titleFocused: Bool
    @FocusState private var noteFocused: Bool

    private var session: CaptureSession? { model.session(for: sessionID) }

    private var deleteConfirmationTitle: String {
        "Delete “\(session.map { SessionDisplay.title($0.title) } ?? "this capture")”?"
    }

    var body: some View {
        Group {
            if let session {
                content(for: session)
            } else {
                EmptyDestinationView(
                    symbol: "questionmark.folder",
                    title: "This capture is no longer available.",
                    message: "It may have been deleted.")
            }
        }
        .navigationTitle(session.map { SessionDisplay.title($0.title) } ?? "Capture")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    showDeleteConfirmation = true
                } label: {
                    Label("Delete", systemImage: "trash")
                }
                .accessibilityLabel("Delete capture")
                .accessibilityHint("Deletes this capture")
                .disabled(session == nil)
            }
        }
        .confirmationDialog(
            deleteConfirmationTitle,
            isPresented: $showDeleteConfirmation,
            titleVisibility: .visible
        ) {
            Button("Delete Capture", role: .destructive) {
                model.delete(sessionID)
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This permanently removes the recording and its note from this Mac.")
        }
    }

    private func content(for session: CaptureSession) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                if model.savedConfirmationVisible, model.pendingCapture == nil {
                    SavedConfirmationBanner()
                }
                titleField
                metadata(for: session)
                Divider()
                PlaybackSection(model: model, sessionID: sessionID)
                Divider()
                noteField
            }
            .frame(maxWidth: 680, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 30)
        }
        .onAppear { seed(from: session) }
        .onDisappear { commitEdits() }
        .onChange(of: titleFocused) { _, focused in if !focused { commitTitle() } }
        .onChange(of: noteFocused) { _, focused in if !focused { commitNote() } }
    }

    private var titleField: some View {
        TextField("Untitled capture", text: $titleDraft)
            .textFieldStyle(.plain)
            .font(.largeTitle.weight(.semibold))
            .focused($titleFocused)
            .onSubmit { commitTitle() }
            .accessibilityLabel("Capture title")
    }

    private func metadata(for session: CaptureSession) -> some View {
        HStack(spacing: 8) {
            Text(Formatting.timestamp(session.createdAt))
            Text("·")
            Text(Formatting.timer(session.duration))
            Text("·")
            Label(session.source, systemImage: "mic")
                .labelStyle(.titleAndIcon)
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            "\(Formatting.timestamp(session.createdAt)), "
                + "\(Formatting.spokenDuration(session.duration)), from \(session.source)")
    }

    private var noteField: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Note")
                .font(.headline)
                .foregroundStyle(.secondary)
            NoteEditor(text: $noteDraft)
                .frame(minHeight: 200)
                .focused($noteFocused)
                .accessibilityLabel("Capture note")
        }
    }

    private func seed(from session: CaptureSession) {
        guard !didSeed else { return }
        titleDraft = session.title
        noteDraft = session.note
        didSeed = true
    }

    private func commitEdits() {
        commitTitle()
        commitNote()
    }

    private func commitTitle() {
        model.updateTitle(titleDraft, for: sessionID)
    }

    private func commitNote() {
        model.updateNote(noteDraft, for: sessionID)
    }
}

private struct SavedConfirmationBanner: View {
    var body: some View {
        Label("Saved", systemImage: "checkmark.circle.fill")
            .font(.callout.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.quaternary.opacity(0.4), in: Capsule())
            .accessibilityLabel("Capture saved")
    }
}
