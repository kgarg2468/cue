import CaptureDelegateCore
import SwiftUI

/// The newest-first archive of every capture. Native list; Return opens; rename and delete are real
/// actions with confirmation.
struct MomentsView: View {
    @ObservedObject var model: AppModel
    @State private var renameTarget: CaptureSession?
    @State private var renameText = ""
    @State private var deleteTarget: CaptureSession?

    var body: some View {
        Group {
            if model.sessions.isEmpty {
                EmptyDestinationView(
                    symbol: "waveform",
                    title: "No moments yet.",
                    message: "Every capture you make lands here, newest first.")
            } else {
                list
            }
        }
        .navigationTitle("Moments")
        .alert("Rename capture", isPresented: renameBinding) {
            TextField("Title", text: $renameText)
            Button("Save") {
                if let target = renameTarget {
                    model.updateTitle(renameText, for: target.id)
                }
                renameTarget = nil
            }
            Button("Cancel", role: .cancel) { renameTarget = nil }
        }
        .confirmationDialog(
            "Delete “\(deleteTarget.map { SessionDisplay.title($0.title) } ?? "this capture")”?",
            isPresented: deleteBinding,
            titleVisibility: .visible
        ) {
            Button("Delete Capture", role: .destructive) {
                if let target = deleteTarget {
                    model.delete(target.id)
                }
                deleteTarget = nil
            }
            Button("Cancel", role: .cancel) { deleteTarget = nil }
        } message: {
            Text("This permanently removes the recording and its note from this Mac.")
        }
    }

    private var list: some View {
        List {
            ForEach(model.sessions) { session in
                Button {
                    model.open(session)
                } label: {
                    MomentRow(session: session)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityHint("Opens this capture")
                .contextMenu {
                    Button("Open") { model.open(session) }
                    Button("Rename…") { beginRename(session) }
                    Button("Delete…", role: .destructive) { deleteTarget = session }
                }
                .swipeActions(edge: .trailing) {
                    Button("Delete", role: .destructive) { deleteTarget = session }
                    Button("Rename") { beginRename(session) }
                        .tint(.gray)
                }
            }
        }
        .listStyle(.inset)
    }

    private func beginRename(_ session: CaptureSession) {
        renameText = session.title
        renameTarget = session
    }

    private var renameBinding: Binding<Bool> {
        Binding(get: { renameTarget != nil }, set: { if !$0 { renameTarget = nil } })
    }

    private var deleteBinding: Binding<Bool> {
        Binding(get: { deleteTarget != nil }, set: { if !$0 { deleteTarget = nil } })
    }
}

/// Reusable empty / explanatory state: a centered symbol with a title and one supporting line.
struct EmptyDestinationView: View {
    let symbol: String
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.title3.weight(.semibold))
            Text(message)
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: 420)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(40)
        .accessibilityElement(children: .combine)
    }
}
