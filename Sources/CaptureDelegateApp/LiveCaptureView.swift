import CaptureDelegateCore
import SwiftUI

/// The in-window live capture surface (states 4 Recording, 5 Paused, 6 Saving). The human note is
/// the focus; the timer and meter report truth without ever implying a transcript or AI.
struct LiveCaptureView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                statusHeader
                meter
                editors
            }
            .frame(maxWidth: 680, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 32)
            .padding(.vertical, 32)
        }
        .navigationTitle("Live capture")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if model.isSaving {
                    savingIndicator
                } else {
                    Button {
                        model.togglePauseResume()
                    } label: {
                        Label(
                            model.isPaused ? "Resume" : "Pause",
                            systemImage: model.isPaused ? "play.fill" : "pause.fill")
                    }
                    .accessibilityHint(model.isPaused ? "Resumes recording" : "Pauses recording")

                    Button(role: .destructive) {
                        model.stopAndSave()
                    } label: {
                        Label("Stop", systemImage: "stop.fill")
                    }
                    .accessibilityHint("Stops and saves this capture")
                }
            }
        }
    }

    private var statusHeader: some View {
        HStack(spacing: 12) {
            if model.isRecording, !model.isSaving {
                RecordingDot(diameter: 12)
            }
            CaptureStateBadge(state: engine.state, isSaving: model.isSaving)
            Spacer()
            Text(Formatting.timer(engine.elapsed))
                .font(.system(.largeTitle, design: .monospaced))
                .fontWeight(.medium)
                .foregroundStyle(model.isRecording && !model.isSaving ? Color.red : .primary)
                .accessibilityLabel("Elapsed time")
                .accessibilityValue(Formatting.spokenDuration(engine.elapsed))
        }
    }

    @ViewBuilder private var meter: some View {
        if model.isSaving {
            EmptyView()
        } else {
            CaptureLevelMeter(
                level: model.isPaused ? 0 : engine.level,
                isReceiving: model.isRecording && engine.isReceivingAudio,
                isActive: model.isRecording)
        }
    }

    private var editors: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Title")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextField("Untitled capture", text: $model.draftTitle)
                    .textFieldStyle(.plain)
                    .font(.title2.weight(.semibold))
                    .accessibilityLabel("Capture title")
            }
            VStack(alignment: .leading, spacing: 6) {
                Text("Note")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                NoteEditor(text: $model.draftNote)
                    .frame(minHeight: 180)
                    .accessibilityLabel("Capture note")
            }
        }
    }

    private var savingIndicator: some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.small)
            Text("Saving")
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Saving")
    }
}

/// A `TextEditor` with a visible placeholder — the platform control has no native one.
struct NoteEditor: View {
    @Binding var text: String
    var placeholder = "Add a note… what mattered, decisions, follow-ups."

    var body: some View {
        ZStack(alignment: .topLeading) {
            if text.isEmpty {
                Text(placeholder)
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 8)
                    .allowsHitTesting(false)
                    .accessibilityHidden(true)
            }
            TextEditor(text: $text)
                .font(.body)
                .scrollContentBackground(.hidden)
                .padding(.horizontal, 1)
        }
        .padding(10)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 10))
    }
}
