import CaptureDelegateCore
import SwiftUI

/// The ⌘K palette. It lists only commands that are implemented and valid right now — capture
/// controls appear only when they apply — plus real matches from saved sessions rendered as
/// keyboard-navigable rows. The highlighted row is always the true Enter target; nothing here is
/// aspirational or a hidden fall-through.
struct CommandPaletteView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    /// Session matches shown inline before spilling the rest into full Search.
    private static let maxSessionRows = 5

    @State private var query = ""
    @State private var selection = 0
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            field
            Divider()
            results
        }
        .frame(width: 560, height: 420)
        .onAppear { fieldFocused = true }
    }

    private var field: some View {
        HStack(spacing: 8) {
            Image(systemName: "command")
                .foregroundStyle(.secondary)
            TextField("Run a command or open a capture…", text: $query)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($fieldFocused)
                .onChange(of: query) { _, _ in selection = 0 }
                .onSubmit { runSelection() }
                .onKeyPress(.downArrow) { move(by: 1) }
                .onKeyPress(.upArrow) { move(by: -1) }
                .onKeyPress(.escape) {
                    model.isPalettePresented = false
                    return .handled
                }
                .accessibilityLabel("Command palette query")
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }

    @ViewBuilder private var results: some View {
        let items = matchedItems
        if items.isEmpty {
            VStack {
                Spacer()
                Text("No matching commands or captures.")
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        } else {
            let firstSessionIndex = items.firstIndex { $0.isCaptureResult }
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                            if index == firstSessionIndex {
                                SectionCaption(title: "Captures")
                            }
                            PaletteRow(item: item, isSelected: index == selection)
                                .id(index)
                                .contentShape(Rectangle())
                                .onTapGesture { run(item) }
                        }
                    }
                    .padding(8)
                }
                .onChange(of: selection) { _, newValue in
                    withAnimation(.none) { proxy.scrollTo(newValue, anchor: .center) }
                }
            }
        }
    }

    // MARK: Items

    private var matchedItems: [PaletteItem] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let commands = availableCommands.filter {
            trimmed.isEmpty || $0.title.lowercased().contains(trimmed)
        }
        var items = commands.map { PaletteItem.command($0) }
        if !trimmed.isEmpty {
            let matches = model.sessions.filter {
                // Match the displayed name, not the raw stored title — untitled captures render
                // as "Untitled capture", so typing that must find them.
                SessionDisplay.title($0.title).lowercased().contains(trimmed)
                    || $0.note.lowercased().contains(trimmed)
            }
            items.append(
                contentsOf: matches.prefix(Self.maxSessionRows).map { PaletteItem.session($0) })
            if matches.count > Self.maxSessionRows {
                items.append(.searchAll(query: query, remaining: matches.count))
            }
        }
        return items
    }

    private var availableCommands: [PaletteCommand] {
        var commands: [PaletteCommand] = []
        if !model.isCaptureActive {
            commands.append(
                PaletteCommand(key: "start", title: "Start capture", symbol: "record.circle") {
                    model.requestStartCapture()
                })
        } else {
            commands.append(
                PaletteCommand(
                    key: "live", title: "Jump to live capture",
                    symbol: "dot.radiowaves.left.and.right"
                ) {
                    model.jumpToLiveCapture()
                })
            if model.isPaused {
                commands.append(
                    PaletteCommand(key: "resume", title: "Resume capture", symbol: "play.fill") {
                        model.togglePauseResume()
                    })
            } else {
                commands.append(
                    PaletteCommand(key: "pause", title: "Pause capture", symbol: "pause.fill") {
                        model.togglePauseResume()
                    })
            }
            commands.append(
                PaletteCommand(key: "stop", title: "Stop and save capture", symbol: "stop.fill") {
                    model.stopAndSave()
                })
        }
        commands.append(
            PaletteCommand(key: "today", title: "Go to Today", symbol: "sun.max") {
                model.select(.today)
            })
        commands.append(
            PaletteCommand(key: "moments", title: "Go to Moments", symbol: "waveform") {
                model.select(.moments)
            })
        commands.append(
            PaletteCommand(key: "search", title: "Search captures", symbol: "magnifyingglass") {
                model.focusSearch()
            })
        return commands
    }

    // MARK: Actions

    private func move(by delta: Int) -> KeyPress.Result {
        let count = matchedItems.count
        guard count > 0 else { return .handled }
        selection = (selection + delta + count) % count
        return .handled
    }

    private func runSelection() {
        let items = matchedItems
        guard items.indices.contains(selection) else { return }
        run(items[selection])
    }

    private func run(_ item: PaletteItem) {
        model.isPalettePresented = false
        switch item {
        case .command(let command): command.action()
        case .session(let session): model.open(session)
        case .searchAll(let query, _):
            model.searchQuery = query
            model.focusSearch()
        }
    }
}

struct PaletteCommand: Identifiable {
    let key: String
    let title: String
    let symbol: String
    let action: () -> Void
    var id: String { key }
}

enum PaletteItem: Identifiable {
    case command(PaletteCommand)
    case session(CaptureSession)
    case searchAll(query: String, remaining: Int)

    var id: String {
        switch self {
        case .command(let command): "command.\(command.id)"
        case .session(let session): "session.\(session.id.uuidString)"
        case .searchAll: "searchAll"
        }
    }

    /// True for rows that represent saved captures (the inline matches and the search overflow),
    /// so the results list can caption that group.
    var isCaptureResult: Bool {
        switch self {
        case .command: false
        case .session, .searchAll: true
        }
    }
}

/// A quiet group caption between the command and capture rows.
private struct SectionCaption: View {
    let title: String

    var body: some View {
        Text(title.uppercased())
            .font(.caption2.weight(.semibold))
            .foregroundStyle(.tertiary)
            .padding(.horizontal, 10)
            .padding(.top, 8)
            .padding(.bottom, 2)
            .accessibilityHidden(true)
    }
}

private struct PaletteRow: View {
    let item: PaletteItem
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: symbol)
                .frame(width: 20)
                .foregroundStyle(isSelected ? Color.primary : .secondary)
            VStack(alignment: .leading, spacing: 1) {
                Text(title)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                if let subtitle {
                    Text(subtitle)
                        .font(.caption)
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
        .background(
            isSelected ? Color.accentColor.opacity(0.18) : .clear,
            in: RoundedRectangle(cornerRadius: 8)
        )
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }

    private var symbol: String {
        switch item {
        case .command(let command): command.symbol
        case .session: "waveform"
        case .searchAll: "magnifyingglass"
        }
    }

    private var title: String {
        switch item {
        case .command(let command): command.title
        case .session(let session): SessionDisplay.title(session.title)
        case .searchAll(_, let remaining): "Search all \(remaining) matching captures"
        }
    }

    private var subtitle: String? {
        switch item {
        case .command: nil
        case .session(let session):
            "\(Formatting.timestamp(session.createdAt)) · \(Formatting.timer(session.duration))"
        case .searchAll(let query, _):
            "for “\(query)”"
        }
    }

    private var accessibilityLabel: String {
        switch item {
        case .command, .searchAll: title
        case .session(let session):
            "Open \(SessionDisplay.title(session.title)), "
                + "\(Formatting.timestamp(session.createdAt)), "
                + Formatting.spokenDuration(session.duration)
        }
    }
}
