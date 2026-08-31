import CaptureDelegateCore
import SwiftUI

/// The main window: a native split view whose detail column is a navigation stack. The global
/// toolbar (search, capture, palette, runtime) stays stable across pushes; destinations add their
/// own contextual items. All capture-flow sheets and the command palette hang off this view.
struct RootView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    var body: some View {
        NavigationSplitView {
            SidebarView(model: model)
        } detail: {
            NavigationStack(path: $model.detailPath) {
                destinationRoot
                    .navigationDestination(for: DetailRoute.self) { route in
                        switch route {
                        case .liveCapture:
                            LiveCaptureView(model: model, engine: engine)
                        case .session(let id):
                            SessionDetailView(model: model, sessionID: id)
                        }
                    }
            }
            .toolbar { globalToolbar }
        }
        .frame(minWidth: 920, minHeight: 620)
        .sheet(item: $model.activeSheet, content: sheet)
        .sheet(isPresented: $model.isPalettePresented) {
            CommandPaletteView(model: model, engine: engine)
        }
        .alert("Recording problem", isPresented: captureErrorBinding) {
            Button("OK", role: .cancel) { model.captureErrorMessage = nil }
        } message: {
            Text(model.captureErrorMessage ?? "")
        }
    }

    @ViewBuilder private var destinationRoot: some View {
        switch model.sidebarSelection ?? .today {
        case .today: TodayView(model: model)
        case .moments: MomentsView(model: model)
        case .projects: ProjectsView()
        case .actions: ActionsView()
        case .agentRuns: AgentRunsView()
        case .search: SearchView(model: model)
        }
    }

    @ToolbarContentBuilder private var globalToolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button {
                model.focusSearch()
            } label: {
                Label("Search", systemImage: "magnifyingglass")
            }
            .accessibilityHint("Searches your captures")

            ToolbarCaptureControl(model: model, engine: engine)

            Button {
                model.isPalettePresented = true
            } label: {
                Label("Commands", systemImage: "command")
            }
            .accessibilityHint("Opens the command palette")

            RuntimeStatusControl(model: model)
        }
    }

    @ViewBuilder private func sheet(_ sheet: ActiveSheet) -> some View {
        switch sheet {
        case .permissionExplainer:
            PermissionExplainerView(model: model)
        case .permissionDenied:
            PermissionDeniedView(model: model)
        case .saveFailure:
            SaveFailureView(model: model)
        }
    }

    private var captureErrorBinding: Binding<Bool> {
        Binding(
            get: { model.captureErrorMessage != nil },
            set: { if !$0 { model.captureErrorMessage = nil } })
    }
}
