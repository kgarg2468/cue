import SwiftUI

/// The three future destinations. They exist only to explain honestly what will arrive later and to
/// reassure that captures work fully today. They contain no dead controls — just copy.
struct ProjectsView: View {
    var body: some View {
        EmptyDestinationView(
            symbol: "folder",
            title: "Projects will group related captures, folders, and repositories.",
            message: "They arrive with agent delegation. For now, everything lives in Moments."
        )
        .navigationTitle("Projects")
    }
}

struct ActionsView: View {
    var body: some View {
        EmptyDestinationView(
            symbol: "checklist",
            title: "Actions turn part of a capture into work you can hand to an agent.",
            message:
                "This becomes available once agents are connected. Your captures and notes work "
                + "fully today."
        )
        .navigationTitle("Actions")
    }
}

struct AgentRunsView: View {
    var body: some View {
        EmptyDestinationView(
            symbol: "sparkles.rectangle.stack",
            title: "Agent runs will show what an agent is doing and what it returned.",
            message: "Nothing runs yet — agent delegation comes in a later milestone."
        )
        .navigationTitle("Agent Runs")
    }
}
