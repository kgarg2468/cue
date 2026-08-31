import CaptureDelegateCore
import SwiftUI

/// Local search over real titles and notes. No network, no fabricated matches — an empty query
/// shows the prompt, a non-matching query says so plainly, and results open the real reader.
struct SearchView: View {
    @ObservedObject var model: AppModel
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            searchField
            Divider()
            content
        }
        .navigationTitle("Search")
        .onAppear { fieldFocused = true }
        .onChange(of: model.searchFocusToken) { _, _ in fieldFocused = true }
    }

    private var searchField: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Search your captures by title or note.", text: $model.searchQuery)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($fieldFocused)
                .accessibilityLabel("Search captures")
                .accessibilityHint("Matches titles and notes on this Mac")
            if !model.searchQuery.isEmpty {
                Button {
                    model.searchQuery = ""
                    fieldFocused = true
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 16)
    }

    @ViewBuilder private var content: some View {
        let trimmed = model.searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            EmptyDestinationView(
                symbol: "magnifyingglass",
                title: "Search your captures by title or note.",
                message: "Results stay on this Mac. Nothing leaves your machine.")
        } else if model.searchResults.isEmpty {
            EmptyDestinationView(
                symbol: "text.magnifyingglass",
                title: "No matches for “\(trimmed)”.",
                message: "Try a different word from a title or note.")
        } else {
            List(model.searchResults) { session in
                Button {
                    model.open(session)
                } label: {
                    MomentRow(session: session)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(SessionDisplay.title(session.title))
                .accessibilityHint("Opens this capture")
            }
            .listStyle(.inset)
        }
    }
}
