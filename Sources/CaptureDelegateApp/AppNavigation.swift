import CaptureDelegateCore
import Foundation

/// Top-level sidebar destinations, in the order mandated by the UX contract.
enum SidebarItem: String, CaseIterable, Identifiable, Hashable {
    case today
    case moments
    case projects
    case actions
    case agentRuns
    case search

    var id: String { rawValue }

    var title: String {
        switch self {
        case .today: "Today"
        case .moments: "Moments"
        case .projects: "Projects"
        case .actions: "Actions"
        case .agentRuns: "Agent Runs"
        case .search: "Search"
        }
    }

    /// Every destination carries a distinct SF Symbol so identity never rests on colour alone.
    var symbol: String {
        switch self {
        case .today: "sun.max"
        case .moments: "waveform"
        case .projects: "folder"
        case .actions: "checklist"
        case .agentRuns: "sparkles.rectangle.stack"
        case .search: "magnifyingglass"
        }
    }
}

/// A value pushed onto the detail-column navigation stack.
enum DetailRoute: Hashable {
    /// The live capture surface for an in-progress recording (states 4/5/6).
    case liveCapture
    /// A saved session opened as the reader (states 7/8).
    case session(UUID)
}

/// The single modal sheet currently presented over the main window.
enum ActiveSheet: String, Identifiable {
    case permissionExplainer
    case permissionDenied
    case saveFailure

    var id: String { rawValue }
}

/// A finalized recording awaiting (or retrying) a secure save. Everything it holds is `Sendable`
/// so it can be handed to a detached save task. The audio file is the private plaintext temp
/// recording; it is deleted only once `SecureSessionStore.save` succeeds.
struct PendingCapture: Sendable {
    let audioFileURL: URL
    var title: String
    var note: String
    let createdAt: Date
    let duration: TimeInterval
}
