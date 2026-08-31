import SwiftUI

/// The fixed sidebar. Order is mandated by the contract: Today, Moments, Projects, Actions,
/// Agent Runs, Search. Each row is text plus a distinct SF Symbol.
struct SidebarView: View {
    @ObservedObject var model: AppModel

    var body: some View {
        List(selection: selectionBinding) {
            Section {
                row(.today)
                row(.moments)
            }
            Section("Delegation") {
                row(.projects)
                row(.actions)
                row(.agentRuns)
            }
            Section {
                row(.search)
            }
        }
        .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 320)
    }

    private func row(_ item: SidebarItem) -> some View {
        Label(item.title, systemImage: item.symbol)
            .tag(item)
            .accessibilityHint("Shows \(item.title)")
    }

    /// Selecting a sidebar row resets the detail stack to that destination's root.
    private var selectionBinding: Binding<SidebarItem?> {
        Binding(
            get: { model.sidebarSelection },
            set: { newValue in
                if let newValue {
                    model.select(newValue)
                }
            })
    }
}
