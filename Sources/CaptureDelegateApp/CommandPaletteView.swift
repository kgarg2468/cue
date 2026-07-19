import CaptureDelegateCore
import SwiftUI

/// The ⌘K palette. It lists only commands that are implemented and valid right now — capture
/// controls appear only when they apply — plus real matches from saved sessions. Nothing here is
/// aspirational.
struct CommandPaletteView: View {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

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
                Text("No matching commands.")
                    .foregroundStyle(.secondary)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        } else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
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
            let sessions = model.sessions.filter {
                $0.title.lowercased().contains(trimmed) || $0.note.lowercased().contains(trimmed)
            }
            items.append(contentsOf: sessions.prefix(8).map { PaletteItem.session($0) })
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

    var id: String {
        switch self {
        case .command(let command): "command.\(command.id)"
        case .session(let session): "session.\(session.id.uuidString)"
        }
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
                if let subtitle {
                    Text(subtitle)
                        .font(.caption)
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
        .accessibilityLabel(title)
        .accessibilityAddTraits(isSelected ? [.isSelected] : [])
    }

    private var symbol: String {
        switch item {
        case .command(let command): command.symbol
        case .session: "waveform"
        }
    }

    private var title: String {
        switch item {
        case .command(let command): command.title
        case .session(let session): "Open: \(SessionDisplay.title(session.title))"
        }
    }

    private var subtitle: String? {
        switch item {
        case .command: nil
        case .session(let session):
            "\(Formatting.timestamp(session.createdAt)) · \(Formatting.timer(session.duration))"
        }
    }
}
