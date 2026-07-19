import AppKit
import CaptureDelegateCore
import CaptureDelegateIPC
import Combine
import Foundation
import UniformTypeIdentifiers

/// The single `@MainActor` coordinator behind every capture surface. It owns navigation state, the
/// loaded sessions, the capture-flow state machine (permission gate → record → save → recover), the
/// informational runtime-health probe, and the Option-Space key monitor. Views observe it; the
/// engine and secure store do the real work.
@MainActor
final class AppModel: ObservableObject {
    // MARK: Navigation

    @Published var sidebarSelection: SidebarItem? = .today
    @Published var detailPath: [DetailRoute] = []
    @Published var activeSheet: ActiveSheet?
    @Published var isPalettePresented = false
    /// Bumped to ask the Search destination to take keyboard focus.
    @Published var searchFocusToken = 0
    @Published var searchQuery = ""

    // MARK: Data

    @Published private(set) var sessions: [CaptureSession] = []
    @Published private(set) var loadErrorMessage: String?

    // MARK: Capture drafts (editable while recording, before the save lands)

    @Published var draftTitle = ""
    @Published var draftNote = ""

    // MARK: Save / recover

    @Published private(set) var isSaving = false
    @Published private(set) var savedConfirmationVisible = false
    @Published private(set) var saveFailureMessage: String?
    @Published private(set) var exportConfirmationVisible = false
    private(set) var pendingCapture: PendingCapture?

    // MARK: Permission

    @Published private(set) var deniedAuthorization: MicrophoneAuthorization = .denied

    // MARK: Transient error surfaces

    @Published var captureErrorMessage: String?

    // MARK: Runtime health (informational only — capture never depends on it)

    @Published private(set) var runtimeOnline = false

    let engine: CaptureEngine
    private let store: SecureSessionStore?
    private let storeInitError: String?

    private var cancellables: Set<AnyCancellable> = []
    private var keyMonitor: Any?
    private var runtimeTask: Task<Void, Never>?
    private var savedConfirmationTask: Task<Void, Never>?
    private var storeMutationTask: Task<Void, Never>?
    private var terminationObserver: NSObjectProtocol?
    private var didReconcileTemporaryRecordings = false
    private let hud: CaptureHUDController

    init(engine: CaptureEngine) {
        self.engine = engine
        self.hud = CaptureHUDController()

        var initError: String?
        do {
            store = try SecureSessionStore()
        } catch {
            store = nil
            initError = Self.describe(error)
        }
        storeInitError = initError

        observeEngineForHUD()
    }

    // MARK: Lifecycle

    func onLaunch() {
        if !didReconcileTemporaryRecordings {
            didReconcileTemporaryRecordings = true
            do {
                try engine.reconcileStaleRecordings()
            } catch {
                captureErrorMessage = Self.describe(error)
            }
        }
        installTerminationObserver()
        installKeyMonitor()
        startRuntimeMonitor()
        Task { await reloadSessions() }
    }

    func tearDown() {
        cleanUpLiveRecordingForTermination()
        if let keyMonitor {
            NSEvent.removeMonitor(keyMonitor)
        }
        keyMonitor = nil
        if let terminationObserver {
            NotificationCenter.default.removeObserver(terminationObserver)
        }
        terminationObserver = nil
        runtimeTask?.cancel()
        savedConfirmationTask?.cancel()
        hud.close()
    }

    // MARK: Derived capture state

    var isRecording: Bool {
        if case .recording = engine.state { return true }
        return false
    }

    var isPaused: Bool { engine.state == .paused }
    var isCaptureActive: Bool { engine.state != .idle }

    // MARK: Data loading

    func reloadSessions() async {
        guard let store else {
            sessions = []
            loadErrorMessage = storeInitError
            return
        }
        do {
            let result = try await Task.detached { try store.listWithProblems() }.value
            sessions = result.sessions
            for problem in result.problems {
                NSLog(
                    "CaptureDelegate skipped unreadable session %@: %@",
                    problem.entryName,
                    String(describing: problem.error)
                )
            }
            loadErrorMessage = nil
        } catch {
            loadErrorMessage = Self.describe(error)
        }
    }

    func session(for id: UUID) -> CaptureSession? {
        sessions.first { $0.id == id }
    }

    var recentSessions: [CaptureSession] {
        Array(sessions.prefix(5))
    }

    var searchResults: [CaptureSession] {
        let query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return [] }
        return sessions.filter { session in
            session.title.lowercased().contains(query) || session.note.lowercased().contains(query)
        }
    }

    // MARK: Navigation intents

    func select(_ item: SidebarItem) {
        sidebarSelection = item
        detailPath.removeAll()
    }

    func focusSearch() {
        select(.search)
        searchFocusToken += 1
    }

    func open(_ session: CaptureSession) {
        detailPath.append(.session(session.id))
    }

    func jumpToLiveCapture() {
        guard isCaptureActive else { return }
        if detailPath.last != .liveCapture {
            detailPath.append(.liveCapture)
        }
    }

    // MARK: Capture flow — start & permission gate (states 2/3/4)

    func requestStartCapture() {
        guard !isSaving else { return }
        if pendingCapture != nil {
            activeSheet = .saveFailure
            return
        }
        guard !isCaptureActive else {
            jumpToLiveCapture()
            return
        }
        switch CaptureEngine.currentAuthorization() {
        case .authorized:
            beginRecording()
        case .undetermined:
            activeSheet = .permissionExplainer
        case .denied:
            deniedAuthorization = .denied
            activeSheet = .permissionDenied
        case .restricted:
            deniedAuthorization = .restricted
            activeSheet = .permissionDenied
        }
    }

    func confirmPermissionExplainer() {
        activeSheet = nil
        CaptureEngine.requestAccess { [weak self] granted in
            guard let self else { return }
            if granted {
                self.beginRecording()
            } else {
                self.deniedAuthorization = CaptureEngine.currentAuthorization()
                self.activeSheet = .permissionDenied
            }
        }
    }

    func cancelPermissionFlow() {
        activeSheet = nil
    }

    func openSystemMicrophoneSettings() {
        let settingsURL = URL(
            string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        if let settingsURL {
            NSWorkspace.shared.open(settingsURL)
        }
    }

    private func beginRecording() {
        guard !isSaving else { return }
        if pendingCapture != nil {
            activeSheet = .saveFailure
            return
        }
        guard !isCaptureActive else {
            jumpToLiveCapture()
            return
        }
        draftTitle = ""
        draftNote = ""
        do {
            try engine.start()
            if detailPath.last != .liveCapture {
                detailPath.append(.liveCapture)
            }
        } catch CaptureEngineError.permissionDenied {
            deniedAuthorization = CaptureEngine.currentAuthorization()
            activeSheet = .permissionDenied
        } catch {
            captureErrorMessage =
                "Recording couldn't start. \(Self.describe(error)) No audio was captured."
        }
    }

    // MARK: Capture flow — pause / resume (states 4/5)

    func togglePauseResume() {
        do {
            switch engine.state {
            case .recording:
                try engine.pause()
            case .paused:
                try engine.resume()
            case .idle:
                break
            }
        } catch {
            captureErrorMessage = Self.describe(error)
        }
    }

    // MARK: Capture flow — stop & save (states 6/7/10)

    func stopAndSave() {
        guard isCaptureActive else { return }
        let finished: (audioFileURL: URL, createdAt: Date, duration: TimeInterval)
        do {
            finished = try engine.stop()
        } catch {
            captureErrorMessage = "Recording couldn't be stopped cleanly. \(Self.describe(error))"
            return
        }
        let pending = PendingCapture(
            audioFileURL: finished.audioFileURL,
            title: draftTitle,
            note: draftNote,
            createdAt: finished.createdAt,
            duration: finished.duration)
        pendingCapture = pending
        performSave(pending)
    }

    func retrySave() {
        guard !isSaving, let pending = pendingCapture else { return }
        activeSheet = nil
        saveFailureMessage = nil
        performSave(pending, recoveringFromFailure: true)
    }

    private func performSave(_ pending: PendingCapture, recoveringFromFailure: Bool = false) {
        guard !isSaving else { return }
        guard let store else {
            holdPendingAudioForRecovery()
            saveFailureMessage =
                storeInitError ?? "The encrypted store could not be opened on this Mac."
            activeSheet = .saveFailure
            return
        }
        isSaving = true
        Task {
            let result: Result<CaptureSession, SecureStoreError>
            do {
                let saved = try await Task.detached {
                    try store.save(
                        audioFileURL: pending.audioFileURL,
                        title: pending.title,
                        note: pending.note,
                        createdAt: pending.createdAt,
                        duration: pending.duration)
                }.value
                result = .success(saved)
            } catch let error as SecureStoreError {
                result = .failure(error)
            } catch {
                result = .failure(.ioFailure(error.localizedDescription))
            }

            switch result {
            case .success(let session):
                pendingCapture = nil
                saveFailureMessage = nil
                activeSheet = nil
                await reloadSessions()
                if recoveringFromFailure {
                    // Retry path: the failure sheet was just dismissed and is still
                    // animating out. Replacing the NavigationStack path while that
                    // dismissal transition is in flight makes SwiftUI drop the
                    // replacement, stranding the user on the live-capture view with no
                    // Saved confirmation — the exact "looks like data loss" moment F8
                    // flagged. Let the dismissal settle before routing so retry-success
                    // lands on the saved capture just like a normal save. `isSaving`
                    // stays true until routing completes so a capture started in this
                    // window cannot be navigated away from.
                    try? await Task.sleep(for: .milliseconds(400))
                }
                routeToSaved(session)
                isSaving = false
                flashSavedConfirmation()
            case .failure(let error):
                isSaving = false
                holdPendingAudioForRecovery()
                saveFailureMessage = Self.saveFailureCopy(for: error)
                activeSheet = .saveFailure
            }
        }
    }

    /// After a failed save the finalized temp file is the only copy of the user's audio, and
    /// launch reconciliation deletes process-owned recordings once this process is dead — so
    /// quitting with the failure unresolved would silently lose the recording. Renaming the
    /// file with the held- prefix moves it out of reconciliation's reach. If the rename itself
    /// fails the original URL is kept, which is no worse than the pre-hold behavior.
    private func holdPendingAudioForRecovery() {
        guard let pending = pendingCapture,
            let heldURL = try? CaptureEngine.markRecordingHeld(at: pending.audioFileURL),
            heldURL != pending.audioFileURL
        else { return }
        pendingCapture = PendingCapture(
            audioFileURL: heldURL,
            title: pending.title,
            note: pending.note,
            createdAt: pending.createdAt,
            duration: pending.duration)
    }

    private func routeToSaved(_ session: CaptureSession) {
        detailPath = [.session(session.id)]
    }

    private func flashSavedConfirmation() {
        savedConfirmationTask?.cancel()
        savedConfirmationVisible = true
        savedConfirmationTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            guard let self, !Task.isCancelled else { return }
            self.savedConfirmationVisible = false
        }
    }

    // MARK: Capture flow — recovery (state 10)

    /// Copy the preserved plaintext temp recording to a user-chosen location via `NSSavePanel`.
    /// The recording is *not* considered saved; the failure sheet stays open for Retry or Discard.
    func exportPendingAudio() {
        guard let pending = pendingCapture else { return }
        let panel = NSSavePanel()
        panel.title = "Export Recording"
        panel.message = "Save the unencrypted audio so it isn't lost, then retry or discard."
        panel.allowedContentTypes = [UTType.mpeg4Audio]
        panel.canCreateDirectories = true
        panel.nameFieldStringValue = "Capture \(Formatting.exportStamp(pending.createdAt)).m4a"
        guard panel.runModal() == .OK, let destination = panel.url else { return }
        let fileManager = FileManager.default
        let stagedDestination = destination.deletingLastPathComponent().appendingPathComponent(
            ".\(destination.lastPathComponent).export-\(UUID().uuidString)"
        )
        defer {
            do {
                try fileManager.removeItem(at: stagedDestination)
            } catch let error as CocoaError where error.code == .fileNoSuchFile {
                // A successful move/replace consumes the staged sibling.
            } catch {
                saveFailureMessage = error.localizedDescription
                activeSheet = .saveFailure
            }
        }
        do {
            try fileManager.copyItem(at: pending.audioFileURL, to: stagedDestination)
            if fileManager.fileExists(atPath: destination.path) {
                _ = try fileManager.replaceItemAt(destination, withItemAt: stagedDestination)
            } else {
                try fileManager.moveItem(at: stagedDestination, to: destination)
            }
            exportConfirmationVisible = true
        } catch {
            saveFailureMessage =
                "The audio couldn't be exported (\(error.localizedDescription)). "
                + "It is still held safely — try again or export elsewhere."
        }
    }

    func discardPendingCapture() {
        if let pending = pendingCapture {
            do {
                if FileManager.default.fileExists(atPath: pending.audioFileURL.path) {
                    try FileManager.default.removeItem(at: pending.audioFileURL)
                }
            } catch {
                saveFailureMessage = Self.saveFailureCopy(
                    for: .ioFailure(error.localizedDescription)
                )
                activeSheet = .saveFailure
                return
            }
        }
        pendingCapture = nil
        saveFailureMessage = nil
        exportConfirmationVisible = false
        activeSheet = nil
    }

    // MARK: Session editing

    func updateTitle(_ title: String, for id: UUID) {
        guard let store else {
            loadErrorMessage = storeInitError
            return
        }
        guard sessions.contains(where: { $0.id == id }) else { return }
        enqueueStoreMutation(
            on: store,
            mutation: { store in
                try store.updateTitle(title, for: id)
            },
            onSuccess: { [weak self] in
                guard let self, let index = self.sessions.firstIndex(where: { $0.id == id }) else {
                    return
                }
                self.sessions[index].title = title
            }
        )
    }

    func updateNote(_ note: String, for id: UUID) {
        guard let store else {
            loadErrorMessage = storeInitError
            return
        }
        guard sessions.contains(where: { $0.id == id }) else { return }
        enqueueStoreMutation(
            on: store,
            mutation: { store in
                try store.updateNote(note, for: id)
            },
            onSuccess: { [weak self] in
                guard let self, let index = self.sessions.firstIndex(where: { $0.id == id }) else {
                    return
                }
                self.sessions[index].note = note
            }
        )
    }

    func delete(_ id: UUID) {
        guard let store else {
            loadErrorMessage = storeInitError
            return
        }
        enqueueStoreMutation(
            on: store,
            mutation: { store in
                try store.delete(id)
            },
            onSuccess: { [weak self] in
                guard let self else { return }
                self.sessions.removeAll { $0.id == id }
                if self.detailPath.last == .session(id) {
                    self.detailPath.removeLast()
                }
            }
        )
    }

    private func enqueueStoreMutation(
        on store: SecureSessionStore,
        mutation: @escaping @Sendable (SecureSessionStore) throws -> Void,
        onSuccess: @escaping @MainActor @Sendable () -> Void
    ) {
        let previousMutation = storeMutationTask
        storeMutationTask = Task { [weak self] in
            await previousMutation?.value
            do {
                try await Task.detached {
                    try mutation(store)
                }.value
                onSuccess()
            } catch {
                guard let self else { return }
                let mutationError = Self.describe(error)
                await self.reloadSessions()
                self.loadErrorMessage = mutationError
            }
        }
    }

    /// Decrypt a session's audio directly to memory for playback. Never writes plaintext to disk.
    func loadAudioData(for id: UUID) async throws -> Data {
        guard let store else {
            throw SecureStoreError.ioFailure("The encrypted store is unavailable.")
        }
        return try await Task.detached { try store.loadAudioData(for: id) }.value
    }

    // MARK: Runtime health

    private func startRuntimeMonitor() {
        runtimeTask?.cancel()
        runtimeTask = Task { [weak self] in
            while !Task.isCancelled {
                let online = await Self.probeRuntime()
                guard let self, !Task.isCancelled else { return }
                self.runtimeOnline = online
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    private nonisolated static func probeRuntime() async -> Bool {
        let environment = ProcessInfo.processInfo.environment
        guard let socketPath = environment["CAPTURE_DELEGATE_SOCKET"], !socketPath.isEmpty else {
            return false
        }
        return await Task.detached {
            (try? IPCClient.checkHealth(socketPath: socketPath)) ?? false
        }.value
    }

    // MARK: Option-Space key monitor (local — no global permission)

    private func installKeyMonitor() {
        guard keyMonitor == nil else { return }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            let optionOnly =
                event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .option
            // keyCode 49 is Space.
            if event.keyCode == 49, optionOnly {
                self.handleOptionSpace()
                return nil
            }
            return event
        }
    }

    private func installTerminationObserver() {
        guard terminationObserver == nil else { return }
        terminationObserver = NotificationCenter.default.addObserver(
            forName: NSApplication.willTerminateNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.cleanUpLiveRecordingForTermination()
            }
        }
    }

    private func cleanUpLiveRecordingForTermination() {
        guard pendingCapture == nil, !isSaving, isCaptureActive else { return }
        do {
            try engine.discard()
        } catch {
            captureErrorMessage = Self.describe(error)
        }
    }

    private func handleOptionSpace() {
        if isCaptureActive {
            stopAndSave()
        } else {
            requestStartCapture()
        }
    }

    // MARK: HUD

    private func observeEngineForHUD() {
        engine.$state
            .receive(on: RunLoop.main)
            .sink { [weak self] state in
                guard let self else { return }
                if state == .idle {
                    self.hud.hide()
                } else {
                    self.hud.show(model: self, engine: self.engine)
                }
            }
            .store(in: &cancellables)
    }

    // MARK: Error copy

    /// Turns a store error into the contract's state-10 recovery copy, naming the real cause and
    /// telling the truth about where the recording currently lives: a private temporary file on this
    /// Mac, readable only by the user's own account, which saving encrypts.
    static func saveFailureCopy(for error: SecureStoreError) -> String {
        let cause: String
        switch error {
        case .keyUnavailable(let detail):
            cause = "the encryption key was unavailable (\(detail))"
        case .ioFailure(let detail):
            cause = "the disk write failed (\(detail))"
        case .corruptOrUndecryptable:
            cause = "the encrypted data could not be written"
        case .sessionNotFound:
            cause = "the session record was missing"
        }
        return
            "This capture couldn't be saved securely — \(cause). Your recording is held in a "
            + "private temporary file on this Mac, readable only by your macOS account; saving "
            + "encrypts it. You can try again, export it, or discard it."
    }

    static func describe(_ error: Error) -> String {
        if let storeError = error as? SecureStoreError {
            switch storeError {
            case .keyUnavailable(let detail): return "Encryption key unavailable: \(detail)."
            case .ioFailure(let detail): return "Disk error: \(detail)."
            case .corruptOrUndecryptable: return "The stored data could not be read."
            case .sessionNotFound: return "The session could not be found."
            }
        }
        return error.localizedDescription
    }
}
