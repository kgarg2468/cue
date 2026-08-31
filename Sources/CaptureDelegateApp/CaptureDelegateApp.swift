import AppKit
import CaptureDelegateCore
import SwiftUI

enum CaptureDelegateWindowID {
    static let main = "main"
}

@main
struct CaptureDelegateApp: App {
    @StateObject private var model: AppModel
    @StateObject private var engine: CaptureEngine

    init() {
        NSApplication.shared.setActivationPolicy(.regular)
        DispatchQueue.main.async {
            NSApplication.shared.activate(ignoringOtherApps: true)
        }
        let engine = CaptureEngine()
        _engine = StateObject(wrappedValue: engine)
        _model = StateObject(wrappedValue: AppModel(engine: engine))
    }

    var body: some Scene {
        WindowGroup(id: CaptureDelegateWindowID.main) {
            RootView(model: model, engine: engine)
                .onAppear {
                    model.onLaunch()
                    NSApplication.shared.activate(ignoringOtherApps: true)
                }
        }
        .defaultSize(width: 1180, height: 760)
        .commands {
            CaptureCommands(model: model, engine: engine)
        }

        MenuBarExtra {
            MenuBarContentView(model: model, engine: engine)
        } label: {
            MenuBarLabelView(engine: engine)
        }
        .menuBarExtraStyle(.window)
    }
}

/// The application's keyboard map. Capture and navigation shortcuts stay in native menus so they are
/// discoverable and reachable; contextual items disable when they don't apply. Option-Space is
/// handled by a local key monitor in `AppModel` (no global permission), and Space/arrow playback
/// shortcuts live in the focused reader.
struct CaptureCommands: Commands {
    @ObservedObject var model: AppModel
    @ObservedObject var engine: CaptureEngine

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New Capture") { model.requestStartCapture() }
                .keyboardShortcut("n", modifiers: .command)
                .disabled(model.isCaptureActive)
        }

        CommandMenu("Capture") {
            Button(model.isPaused ? "Resume Capture" : "Pause Capture") {
                model.togglePauseResume()
            }
            .keyboardShortcut("p", modifiers: [.command, .shift])
            .disabled(!model.isCaptureActive)

            Button("Stop & Save") { model.stopAndSave() }
                .keyboardShortcut(".", modifiers: .command)
                .disabled(!model.isCaptureActive)

            Divider()

            Button("Jump to Live Capture") { model.jumpToLiveCapture() }
                .keyboardShortcut("l", modifiers: .command)
                .disabled(!model.isCaptureActive)
        }

        CommandMenu("Go") {
            Button("Today") { model.select(.today) }
                .keyboardShortcut("1", modifiers: .command)
            Button("Moments") { model.select(.moments) }
                .keyboardShortcut("2", modifiers: .command)

            Divider()

            Button("Search") { model.focusSearch() }
                .keyboardShortcut("f", modifiers: .command)
            Button("Command Palette") { model.isPalettePresented = true }
                .keyboardShortcut("k", modifiers: .command)
        }
    }
}
