import Darwin
import Dispatch
import Foundation

public enum IPCClientError: Error, Equatable {
    case responseTooLarge
    case invalidProcessEvent
    case invalidSessionResponse
}

public struct Session: Equatable {
    public let id: String
    public let title: String
    public let createdAtMilliseconds: Int
    public let updatedAtMilliseconds: Int
    /// Nil for an uncategorized session; the backend omits the field entirely.
    public let kind: String?
    /// Nil for a session without a note; the backend omits the field entirely.
    public let note: String?

    public init(
        id: String,
        title: String,
        createdAtMilliseconds: Int,
        updatedAtMilliseconds: Int,
        kind: String? = nil,
        note: String? = nil
    ) {
        self.id = id
        self.title = title
        self.createdAtMilliseconds = createdAtMilliseconds
        self.updatedAtMilliseconds = updatedAtMilliseconds
        self.kind = kind
        self.note = note
    }
}

public struct SessionListPage: Equatable {
    public let sessions: [Session]
    public let truncated: Bool

    public init(sessions: [Session], truncated: Bool) {
        self.sessions = sessions
        self.truncated = truncated
    }
}

/// A stable pointer into one session's timeline, with the exact text it refers to.
public struct Source: Equatable {
    public let id: String
    public let sessionID: String
    public let startMilliseconds: Int
    public let endMilliseconds: Int
    /// Nil for an unattributed source; the backend omits the field entirely.
    public let speaker: String?
    public let text: String

    public init(
        id: String,
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) {
        self.id = id
        self.sessionID = sessionID
        self.startMilliseconds = startMilliseconds
        self.endMilliseconds = endMilliseconds
        self.speaker = speaker
        self.text = text
    }
}

public struct SourceListPage: Equatable {
    public let sources: [Source]
    public let truncated: Bool

    public init(sources: [Source], truncated: Bool) {
        self.sources = sources
        self.truncated = truncated
    }
}

/// One span of spoken text inside a session's timeline, as the transcript recorded it.
public struct TranscriptSegment: Equatable {
    public let id: String
    public let sessionID: String
    public let startMilliseconds: Int
    public let endMilliseconds: Int
    /// Nil for an unattributed segment; the backend omits the field entirely.
    public let speaker: String?
    public let text: String

    public init(
        id: String,
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) {
        self.id = id
        self.sessionID = sessionID
        self.startMilliseconds = startMilliseconds
        self.endMilliseconds = endMilliseconds
        self.speaker = speaker
        self.text = text
    }
}

public struct TranscriptPage: Equatable {
    public let segments: [TranscriptSegment]
    public let truncated: Bool

    public init(segments: [TranscriptSegment], truncated: Bool) {
        self.segments = segments
        self.truncated = truncated
    }
}

/// A user-placed moment inside one session's timeline, with the kind of attention it deserves.
public struct Marker: Equatable {
    public let id: String
    public let sessionID: String
    public let atMilliseconds: Int
    public let kind: String
    /// Nil for a marker without a note; the backend omits the field entirely.
    public let note: String?

    public init(
        id: String,
        sessionID: String,
        atMilliseconds: Int,
        kind: String,
        note: String? = nil
    ) {
        self.id = id
        self.sessionID = sessionID
        self.atMilliseconds = atMilliseconds
        self.kind = kind
        self.note = note
    }
}

public struct MarkerListPage: Equatable {
    public let markers: [Marker]
    public let truncated: Bool

    public init(markers: [Marker], truncated: Bool) {
        self.markers = markers
        self.truncated = truncated
    }
}

/// One durable execution of a process, as the backend recorded it.
public struct RunRecord: Equatable {
    public let id: String
    public let runID: String
    /// Nil for a run that is not linked to a session; the backend omits the field entirely.
    public let sessionID: String?
    public let executable: String
    public let status: String
    /// Nil while the run is live, and for a run that ended without an exit code.
    public let exitCode: Int?
    /// Nil for a run that ended cleanly; the backend omits the field entirely.
    public let errorCode: String?
    public let startedAtMilliseconds: Int
    /// Nil while the run is still live; the backend omits the field entirely.
    public let endedAtMilliseconds: Int?

    public init(
        id: String,
        runID: String,
        sessionID: String? = nil,
        executable: String,
        status: String,
        exitCode: Int? = nil,
        errorCode: String? = nil,
        startedAtMilliseconds: Int,
        endedAtMilliseconds: Int? = nil
    ) {
        self.id = id
        self.runID = runID
        self.sessionID = sessionID
        self.executable = executable
        self.status = status
        self.exitCode = exitCode
        self.errorCode = errorCode
        self.startedAtMilliseconds = startedAtMilliseconds
        self.endedAtMilliseconds = endedAtMilliseconds
    }
}

public struct RunListPage: Equatable {
    public let runs: [RunRecord]
    public let truncated: Bool

    public init(runs: [RunRecord], truncated: Bool) {
        self.runs = runs
        self.truncated = truncated
    }
}

/// One backend-authored moment in a run record's life, as the backend recorded it.
public struct RunEvent: Equatable {
    public let id: String
    /// The run record this event belongs to — the record's own id, not the reusable run id.
    public let recordID: String
    /// Orders a record's trail independently of the clock.
    public let sequence: Int
    public let atMilliseconds: Int
    public let kind: String

    public init(
        id: String,
        recordID: String,
        sequence: Int,
        atMilliseconds: Int,
        kind: String
    ) {
        self.id = id
        self.recordID = recordID
        self.sequence = sequence
        self.atMilliseconds = atMilliseconds
        self.kind = kind
    }
}

public struct RunEventPage: Equatable {
    public let events: [RunEvent]
    public let truncated: Bool

    public init(events: [RunEvent], truncated: Bool) {
        self.events = events
        self.truncated = truncated
    }
}

/// One unit of delegable work, as the backend drafted it.
public struct ActionRecord: Equatable {
    public let id: String
    /// Nil for an action that is not linked to a session; the backend omits the field entirely.
    public let sessionID: String?
    public let kind: String
    public let title: String
    /// Backend-authored; every action begins as a draft.
    public let status: String
    public let createdAtMilliseconds: Int
    public let updatedAtMilliseconds: Int

    public init(
        id: String,
        sessionID: String? = nil,
        kind: String,
        title: String,
        status: String,
        createdAtMilliseconds: Int,
        updatedAtMilliseconds: Int
    ) {
        self.id = id
        self.sessionID = sessionID
        self.kind = kind
        self.title = title
        self.status = status
        self.createdAtMilliseconds = createdAtMilliseconds
        self.updatedAtMilliseconds = updatedAtMilliseconds
    }
}

public struct ActionPage: Equatable {
    public let actions: [ActionRecord]
    public let truncated: Bool

    public init(actions: [ActionRecord], truncated: Bool) {
        self.actions = actions
        self.truncated = truncated
    }
}

public enum ProcessOutputStream: String, Equatable {
    case stdout
    case stderr
}

public enum ProcessExitErrorCode: String, Equatable {
    case spawnFailed = "spawn_failed"
    case worktreeFailed = "worktree_failed"
    case internalError = "internal_error"
    case capacityExhausted = "capacity_exhausted"
    case timedOut = "timed_out"
    case cancelled = "cancelled"
}

public enum CancelProcessResult: Equatable {
    case accepted
    case notFound
}

public enum PauseProcessResult: Equatable {
    case accepted
    case notFound
}

public enum ResumeProcessResult: Equatable {
    case accepted
    case notFound
}

public enum SendInputResult: Equatable {
    case accepted
    case notFound
    case closed
    case capacityExhausted
}

public enum CloseStdinResult: Equatable {
    case accepted
    case notFound
}

public enum ProcessEvent: Equatable {
    case output(runID: String, stream: ProcessOutputStream, output: String)
    case metadata(runID: String, pid: Int, durationMilliseconds: Int, redactions: Int)
    case inputWaiting(runID: String, quietForMilliseconds: Int)
    case exit(runID: String, exitCode: Int?, errorCode: ProcessExitErrorCode? = nil)
}

public enum IPCClient {
    public static let healthRequest = "{\"version\":1,\"type\":\"health\"}\n"
    private static let maximumResponseBytes = 8 * 1024
    private static let responseTimeoutMicroseconds: Int32 = 500_000

    public static func checkHealth(socketPath: String) throws -> Bool {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeHealthRequest(to: descriptor)
        let response = try readBoundedResponseLine(from: descriptor)
        return try validateHealthResponse(response)
    }

    public static func createSession(socketPath: String, title: String, kind: String? = nil) throws
        -> Session
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeCreateSessionRequest(title: title, kind: kind, to: descriptor)
        return try decodeCreateSessionResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listSessions(socketPath: String) throws -> SessionListPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListSessionsRequest(to: descriptor)
        return try decodeListSessionsResponse(readBoundedResponseLine(from: descriptor))
    }

    /// Replaces one session's note wholesale; a nil note clears it.
    public static func setSessionNote(socketPath: String, sessionID: String, note: String?) throws
        -> Session
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeSetSessionNoteRequest(sessionID: sessionID, note: note, to: descriptor)
        return try decodeSetSessionNoteResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func addSource(
        socketPath: String,
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) throws -> Source {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeAddSourceRequest(
            sessionID: sessionID,
            startMilliseconds: startMilliseconds,
            endMilliseconds: endMilliseconds,
            speaker: speaker,
            text: text,
            to: descriptor
        )
        return try decodeAddSourceResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listSources(socketPath: String, sessionID: String) throws -> SourceListPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListSourcesRequest(sessionID: sessionID, to: descriptor)
        return try decodeListSourcesResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func addTranscriptSegment(
        socketPath: String,
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) throws -> TranscriptSegment {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeAddTranscriptSegmentRequest(
            sessionID: sessionID,
            startMilliseconds: startMilliseconds,
            endMilliseconds: endMilliseconds,
            speaker: speaker,
            text: text,
            to: descriptor
        )
        return try decodeAddTranscriptSegmentResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listTranscript(socketPath: String, sessionID: String) throws
        -> TranscriptPage
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListTranscriptRequest(sessionID: sessionID, to: descriptor)
        return try decodeListTranscriptResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func addMarker(
        socketPath: String,
        sessionID: String,
        atMilliseconds: Int,
        kind: String,
        note: String? = nil
    ) throws -> Marker {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeAddMarkerRequest(
            sessionID: sessionID,
            atMilliseconds: atMilliseconds,
            kind: kind,
            note: note,
            to: descriptor
        )
        return try decodeAddMarkerResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listMarkers(socketPath: String, sessionID: String) throws -> MarkerListPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListMarkersRequest(sessionID: sessionID, to: descriptor)
        return try decodeListMarkersResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listRuns(socketPath: String) throws -> RunListPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListRunsRequest(to: descriptor)
        return try decodeListRunsResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listRunEvents(socketPath: String, recordID: String) throws -> RunEventPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListRunEventsRequest(recordID: recordID, to: descriptor)
        return try decodeListRunEventsResponse(readBoundedResponseLine(from: descriptor))
    }

    /// Drafts one action; a nil sessionID leaves it unlinked to any session.
    public static func createAction(
        socketPath: String,
        kind: String,
        title: String,
        sessionID: String? = nil
    ) throws -> ActionRecord {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeCreateActionRequest(
            kind: kind,
            title: title,
            sessionID: sessionID,
            to: descriptor
        )
        return try decodeCreateActionResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func listActions(socketPath: String) throws -> ActionPage {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeListActionsRequest(to: descriptor)
        return try decodeListActionsResponse(readBoundedResponseLine(from: descriptor))
    }

    public static func cancelProcess(socketPath: String, runID: String) throws
        -> CancelProcessResult
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeCancelProcessRequest(runID: runID, to: descriptor)
        return try decodeCancelProcessResponse(
            readBoundedResponseLine(from: descriptor), expectedRunID: runID)
    }

    public static func pauseProcess(socketPath: String, runID: String) throws
        -> PauseProcessResult
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writePauseProcessRequest(runID: runID, to: descriptor)
        return try decodePauseProcessResponse(
            readBoundedResponseLine(from: descriptor), expectedRunID: runID)
    }

    public static func resumeProcess(socketPath: String, runID: String) throws
        -> ResumeProcessResult
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeResumeProcessRequest(runID: runID, to: descriptor)
        return try decodeResumeProcessResponse(
            readBoundedResponseLine(from: descriptor), expectedRunID: runID)
    }

    public static func sendInput(socketPath: String, runID: String, data: String) throws
        -> SendInputResult
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeSendInputRequest(runID: runID, data: data, to: descriptor)
        return try decodeSendInputResponse(
            readBoundedResponseLine(from: descriptor), expectedRunID: runID)
    }

    public static func closeStdin(socketPath: String, runID: String) throws
        -> CloseStdinResult
    {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeCloseStdinRequest(runID: runID, to: descriptor)
        return try decodeCloseStdinResponse(
            readBoundedResponseLine(from: descriptor), expectedRunID: runID)
    }

    public static func startProcess(
        socketPath: String,
        runID: String,
        executable: String,
        arguments: [String],
        timeoutMilliseconds: Int,
        pty: Bool = false,
        inputWaitDetectMilliseconds: Int? = nil,
        worktreeRepository: String? = nil,
        onEvent: (ProcessEvent) -> Void
    ) throws {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeStartProcessRequest(
            runID: runID,
            executable: executable,
            arguments: arguments,
            timeoutMilliseconds: timeoutMilliseconds,
            pty: pty,
            inputWaitDetectMilliseconds: inputWaitDetectMilliseconds,
            worktreeRepository: worktreeRepository,
            to: descriptor
        )
        try readProcessEvents(from: descriptor, expectedRunID: runID, onEvent: onEvent)
    }

    static func readProcessEvents(
        from descriptor: Int32,
        expectedRunID: String,
        onEvent: (ProcessEvent) -> Void
    ) throws {
        while true {
            let event = try decodeProcessEvent(
                try readResponseLine(from: descriptor),
                expectedRunID: expectedRunID
            )
            onEvent(event)
            if case .exit = event {
                return
            }
        }
    }

    public static func startProcessRequest(
        runID: String,
        executable: String,
        arguments: [String],
        timeoutMilliseconds: Int,
        pty: Bool = false,
        inputWaitDetectMilliseconds: Int? = nil,
        worktreeRepository: String? = nil
    ) -> String {
        let ptyField = pty ? ",\"pty\":true" : ""
        let inputWaitDetectField =
            inputWaitDetectMilliseconds.map {
                ",\"input_wait_detect_milliseconds\":\($0)"
            } ?? ""
        let worktreeRepositoryField =
            worktreeRepository.map {
                ",\"worktree_repository\":\(jsonString($0))"
            } ?? ""
        return "{\"version\":1,\"type\":\"start_process\",\"run_id\":\(jsonString(runID)),"
            + "\"executable\":\(jsonString(executable)),\"arguments\":\(jsonStringArray(arguments)),"
            + "\"timeout_milliseconds\":\(timeoutMilliseconds)\(ptyField)"
            + "\(inputWaitDetectField)\(worktreeRepositoryField)}\n"
    }

    public static let listSessionsRequest = "{\"version\":1,\"type\":\"list_sessions\"}\n"

    public static func createSessionRequest(title: String, kind: String? = nil) -> String {
        let kindField = kind.map { ",\"kind\":\(jsonString($0))" } ?? ""
        return "{\"version\":1,\"type\":\"create_session\",\"title\":\(jsonString(title))"
            + "\(kindField)}\n"
    }

    public static func setSessionNoteRequest(sessionID: String, note: String?) -> String {
        // This message is an update, so the key is always stated: an explicit null is how
        // the note is cleared, and omitting it is rejected by the backend.
        let noteField = note.map { jsonString($0) } ?? "null"
        return "{\"version\":1,\"type\":\"set_session_note\","
            + "\"session_id\":\(jsonString(sessionID)),\"note\":\(noteField)}\n"
    }

    public static func addSourceRequest(
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) -> String {
        // An unattributed source omits the key; an explicit null is rejected by the backend.
        let speakerField = speaker.map { ",\"speaker\":\(jsonString($0))" } ?? ""
        return "{\"version\":1,\"type\":\"add_source\",\"session_id\":\(jsonString(sessionID)),"
            + "\"start_ms\":\(startMilliseconds),\"end_ms\":\(endMilliseconds)\(speakerField),"
            + "\"text\":\(jsonString(text))}\n"
    }

    public static func listSourcesRequest(sessionID: String) -> String {
        "{\"version\":1,\"type\":\"list_sources\",\"session_id\":\(jsonString(sessionID))}\n"
    }

    public static func addTranscriptSegmentRequest(
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String? = nil,
        text: String
    ) -> String {
        // An unattributed segment omits the key; an explicit null is rejected by the backend.
        let speakerField = speaker.map { ",\"speaker\":\(jsonString($0))" } ?? ""
        return "{\"version\":1,\"type\":\"add_transcript_segment\","
            + "\"session_id\":\(jsonString(sessionID)),"
            + "\"start_ms\":\(startMilliseconds),\"end_ms\":\(endMilliseconds)\(speakerField),"
            + "\"text\":\(jsonString(text))}\n"
    }

    public static func listTranscriptRequest(sessionID: String) -> String {
        "{\"version\":1,\"type\":\"list_transcript\",\"session_id\":\(jsonString(sessionID))}\n"
    }

    public static func addMarkerRequest(
        sessionID: String,
        atMilliseconds: Int,
        kind: String,
        note: String? = nil
    ) -> String {
        // A marker without a note omits the key; an explicit null is rejected by the backend.
        let noteField = note.map { ",\"note\":\(jsonString($0))" } ?? ""
        return "{\"version\":1,\"type\":\"add_marker\",\"session_id\":\(jsonString(sessionID)),"
            + "\"at_ms\":\(atMilliseconds),\"kind\":\(jsonString(kind))\(noteField)}\n"
    }

    public static func listMarkersRequest(sessionID: String) -> String {
        "{\"version\":1,\"type\":\"list_markers\",\"session_id\":\(jsonString(sessionID))}\n"
    }

    public static let listRunsRequest = "{\"version\":1,\"type\":\"list_runs\"}\n"

    public static func listRunEventsRequest(recordID: String) -> String {
        "{\"version\":1,\"type\":\"list_run_events\",\"record_id\":\(jsonString(recordID))}\n"
    }

    public static func createActionRequest(kind: String, title: String, sessionID: String? = nil)
        -> String
    {
        // An unlinked action omits the key; an explicit null is rejected by the backend.
        // Status is not a field of this message: the backend authors it, and every action
        // begins as a draft.
        let sessionField = sessionID.map { ",\"session_id\":\(jsonString($0))" } ?? ""
        return "{\"version\":1,\"type\":\"create_action\",\"kind\":\(jsonString(kind)),"
            + "\"title\":\(jsonString(title))\(sessionField)}\n"
    }

    public static let listActionsRequest = "{\"version\":1,\"type\":\"list_actions\"}\n"

    public static func cancelProcessRequest(runID: String) -> String {
        "{\"version\":1,\"type\":\"cancel_process\",\"run_id\":\(jsonString(runID))}\n"
    }

    public static func pauseProcessRequest(runID: String) -> String {
        "{\"version\":1,\"type\":\"pause_process\",\"run_id\":\(jsonString(runID))}\n"
    }

    public static func resumeProcessRequest(runID: String) -> String {
        "{\"version\":1,\"type\":\"resume_process\",\"run_id\":\(jsonString(runID))}\n"
    }

    public static func sendInputRequest(runID: String, data: String) -> String {
        "{\"version\":1,\"type\":\"send_input\",\"run_id\":\(jsonString(runID)),\"data\":\(jsonString(data))}\n"
    }

    public static func closeStdinRequest(runID: String) -> String {
        "{\"version\":1,\"type\":\"close_stdin\",\"run_id\":\(jsonString(runID))}\n"
    }

    public static func validateHealthResponse(_ response: String) throws -> Bool {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "health_response",
            json["status"] as? String == "ok"
        else {
            return false
        }

        return true
    }

    private static func connect(to socketPath: String) throws -> Int32 {
        let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else {
            throw posixError()
        }

        do {
            try suppressSIGPIPE(on: descriptor)
        } catch {
            _ = Darwin.close(descriptor)
            throw error
        }

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(socketPath.utf8) + [0]
        guard pathBytes.count <= MemoryLayout.size(ofValue: address.sun_path) else {
            _ = Darwin.close(descriptor)
            throw POSIXError(.ENAMETOOLONG)
        }
        withUnsafeMutableBytes(of: &address.sun_path) { destination in
            destination.copyBytes(from: pathBytes)
        }

        let result = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard result == 0 else {
            let error = posixError()
            _ = Darwin.close(descriptor)
            throw error
        }

        return descriptor
    }

    static func writeHealthRequest(to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(healthRequest.utf8), to: descriptor)
    }

    static func writeStartProcessRequest(
        runID: String,
        executable: String,
        arguments: [String],
        timeoutMilliseconds: Int,
        pty: Bool,
        inputWaitDetectMilliseconds: Int?,
        worktreeRepository: String?,
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(
                startProcessRequest(
                    runID: runID,
                    executable: executable,
                    arguments: arguments,
                    timeoutMilliseconds: timeoutMilliseconds,
                    pty: pty,
                    inputWaitDetectMilliseconds: inputWaitDetectMilliseconds,
                    worktreeRepository: worktreeRepository
                ).utf8
            ), to: descriptor)
    }

    static func writeCreateSessionRequest(title: String, kind: String?, to descriptor: Int32) throws
    {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(createSessionRequest(title: title, kind: kind).utf8), to: descriptor)
    }

    static func writeListSessionsRequest(to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listSessionsRequest.utf8), to: descriptor)
    }

    static func writeSetSessionNoteRequest(sessionID: String, note: String?, to descriptor: Int32)
        throws
    {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(setSessionNoteRequest(sessionID: sessionID, note: note).utf8), to: descriptor)
    }

    static func writeAddSourceRequest(
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String?,
        text: String,
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(
                addSourceRequest(
                    sessionID: sessionID,
                    startMilliseconds: startMilliseconds,
                    endMilliseconds: endMilliseconds,
                    speaker: speaker,
                    text: text
                ).utf8
            ), to: descriptor)
    }

    static func writeListSourcesRequest(sessionID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listSourcesRequest(sessionID: sessionID).utf8), to: descriptor)
    }

    static func writeAddTranscriptSegmentRequest(
        sessionID: String,
        startMilliseconds: Int,
        endMilliseconds: Int,
        speaker: String?,
        text: String,
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(
                addTranscriptSegmentRequest(
                    sessionID: sessionID,
                    startMilliseconds: startMilliseconds,
                    endMilliseconds: endMilliseconds,
                    speaker: speaker,
                    text: text
                ).utf8
            ), to: descriptor)
    }

    static func writeListTranscriptRequest(sessionID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listTranscriptRequest(sessionID: sessionID).utf8), to: descriptor)
    }

    static func writeAddMarkerRequest(
        sessionID: String,
        atMilliseconds: Int,
        kind: String,
        note: String?,
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(
                addMarkerRequest(
                    sessionID: sessionID,
                    atMilliseconds: atMilliseconds,
                    kind: kind,
                    note: note
                ).utf8
            ), to: descriptor)
    }

    static func writeListMarkersRequest(sessionID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listMarkersRequest(sessionID: sessionID).utf8), to: descriptor)
    }

    static func writeListRunsRequest(to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listRunsRequest.utf8), to: descriptor)
    }

    static func writeListRunEventsRequest(recordID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listRunEventsRequest(recordID: recordID).utf8), to: descriptor)
    }

    static func writeCreateActionRequest(
        kind: String,
        title: String,
        sessionID: String?,
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(createActionRequest(kind: kind, title: title, sessionID: sessionID).utf8),
            to: descriptor)
    }

    static func writeListActionsRequest(to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(listActionsRequest.utf8), to: descriptor)
    }

    static func writeCancelProcessRequest(runID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(cancelProcessRequest(runID: runID).utf8), to: descriptor)
    }

    static func writePauseProcessRequest(runID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(pauseProcessRequest(runID: runID).utf8), to: descriptor)
    }

    static func writeResumeProcessRequest(runID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(resumeProcessRequest(runID: runID).utf8), to: descriptor)
    }

    static func writeSendInputRequest(runID: String, data: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(sendInputRequest(runID: runID, data: data).utf8), to: descriptor)
    }

    static func writeCloseStdinRequest(runID: String, to descriptor: Int32) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(Array(closeStdinRequest(runID: runID).utf8), to: descriptor)
    }

    private static func write(_ bytes: [UInt8], to descriptor: Int32) throws {
        var written = 0
        while written < bytes.count {
            let count = bytes.withUnsafeBytes { buffer in
                Darwin.write(
                    descriptor,
                    buffer.baseAddress!.advanced(by: written),
                    bytes.count - written
                )
            }
            if count < 0, errno == EINTR {
                continue
            }
            guard count > 0 else {
                throw posixError()
            }
            written += count
        }
    }

    static func readResponseLine(from descriptor: Int32) throws -> String {
        var bytes: [UInt8] = []
        var byte: UInt8 = 0

        while true {
            try waitForReadable(descriptor, timeoutMilliseconds: -1)

            let count = Darwin.read(descriptor, &byte, 1)
            if count < 0, errno == EINTR {
                continue
            }
            guard count > 0 else {
                throw posixError()
            }
            guard bytes.count < maximumResponseBytes else {
                throw IPCClientError.responseTooLarge
            }
            bytes.append(byte)
            if byte == 10 {
                return String(decoding: bytes, as: UTF8.self)
            }
            break
        }

        let deadline =
            DispatchTime.now().uptimeNanoseconds + UInt64(responseTimeoutMicroseconds) * 1_000
        return try readResponseLine(from: descriptor, bytes: bytes, deadline: deadline)
    }

    private static func readBoundedResponseLine(from descriptor: Int32) throws -> String {
        let deadline =
            DispatchTime.now().uptimeNanoseconds + UInt64(responseTimeoutMicroseconds) * 1_000
        return try readResponseLine(from: descriptor, bytes: [], deadline: deadline)
    }

    private static func readResponseLine(
        from descriptor: Int32,
        bytes: [UInt8],
        deadline: UInt64
    ) throws -> String {
        var bytes = bytes
        var byte: UInt8 = 0

        while true {
            try waitForReadable(
                descriptor,
                timeoutMilliseconds: try remainingTimeoutMilliseconds(until: deadline)
            )

            let count = Darwin.read(descriptor, &byte, 1)
            if count < 0, errno == EINTR {
                continue
            }
            guard count > 0 else {
                throw posixError()
            }
            guard bytes.count < maximumResponseBytes else {
                throw IPCClientError.responseTooLarge
            }
            bytes.append(byte)
            if byte == 10 {
                return String(decoding: bytes, as: UTF8.self)
            }
        }
    }

    private static func waitForReadable(_ descriptor: Int32, timeoutMilliseconds: Int32) throws {
        while true {
            var pollDescriptor = pollfd(fd: descriptor, events: Int16(POLLIN), revents: 0)
            let pollResult = Darwin.poll(&pollDescriptor, 1, timeoutMilliseconds)
            if pollResult < 0, errno == EINTR {
                continue
            }
            guard pollResult > 0 else {
                throw POSIXError(.ETIMEDOUT)
            }
            return
        }
    }

    private static func decodeProcessEvent(_ response: String, expectedRunID: String) throws
        -> ProcessEvent
    {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            let type = json["type"] as? String,
            json["run_id"] as? String == expectedRunID
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch type {
        case "run_output":
            guard
                let streamName = json["stream"] as? String,
                let stream = ProcessOutputStream(rawValue: streamName),
                let output = json["output"] as? String
            else {
                throw IPCClientError.invalidProcessEvent
            }
            return .output(runID: expectedRunID, stream: stream, output: output)
        case "run_metadata":
            // Decode the fields currently consumed by the app; richer process/environment
            // metadata remains future client work. Extra wire fields are intentionally ignored.
            guard
                let pid = json["pid"] as? Int,
                let durationMilliseconds = json["duration_ms"] as? Int,
                let redactions = json["redactions"] as? Int
            else {
                throw IPCClientError.invalidProcessEvent
            }
            return .metadata(
                runID: expectedRunID, pid: pid,
                durationMilliseconds: durationMilliseconds, redactions: redactions)
        case "run_input_waiting":
            guard let quietForMilliseconds = json["quiet_for_milliseconds"] as? Int else {
                throw IPCClientError.invalidProcessEvent
            }
            return .inputWaiting(
                runID: expectedRunID, quietForMilliseconds: quietForMilliseconds)
        case "run_exit":
            let exitCode: Int?
            if json["exit_code"] is NSNull {
                exitCode = nil
            } else if let code = json["exit_code"] as? Int {
                exitCode = code
            } else {
                throw IPCClientError.invalidProcessEvent
            }
            let errorCode: ProcessExitErrorCode?
            if json["error_code"] is NSNull || json["error_code"] == nil {
                errorCode = nil
            } else if let value = json["error_code"] as? String,
                let value = ProcessExitErrorCode(rawValue: value)
            {
                errorCode = value
            } else {
                throw IPCClientError.invalidProcessEvent
            }
            return .exit(runID: expectedRunID, exitCode: exitCode, errorCode: errorCode)
        default:
            throw IPCClientError.invalidProcessEvent
        }
    }

    static func decodeCreateSessionResponse(_ response: String) throws -> Session {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "create_session_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeSession(json["session"])
    }

    static func decodeListSessionsResponse(_ response: String) throws -> SessionListPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_sessions_response",
            let sessions = json["sessions"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return SessionListPage(
            sessions: try sessions.map { try decodeSession($0) },
            truncated: truncated
        )
    }

    static func decodeSetSessionNoteResponse(_ response: String) throws -> Session {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "set_session_note_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeSession(json["session"])
    }

    private static func decodeSession(_ value: Any?) throws -> Session {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let title = json["title"] as? String,
            let createdAtMilliseconds = json["created_at_ms"] as? Int,
            let updatedAtMilliseconds = json["updated_at_ms"] as? Int
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // An uncategorized session omits the field; a present kind must still be a string.
        let kind: String?
        if json["kind"] == nil {
            kind = nil
        } else if let value = json["kind"] as? String {
            kind = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        // A session without a note omits the field; a present note must still be a string.
        let note: String?
        if json["note"] == nil {
            note = nil
        } else if let value = json["note"] as? String {
            note = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return Session(
            id: id,
            title: title,
            createdAtMilliseconds: createdAtMilliseconds,
            updatedAtMilliseconds: updatedAtMilliseconds,
            kind: kind,
            note: note
        )
    }

    static func decodeAddSourceResponse(_ response: String) throws -> Source {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "add_source_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeSource(json["source"])
    }

    static func decodeListSourcesResponse(_ response: String) throws -> SourceListPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_sources_response",
            let sources = json["sources"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return SourceListPage(
            sources: try sources.map { try decodeSource($0) },
            truncated: truncated
        )
    }

    private static func decodeSource(_ value: Any?) throws -> Source {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let sessionID = json["session_id"] as? String, !sessionID.isEmpty,
            let startMilliseconds = json["start_ms"] as? Int,
            let endMilliseconds = json["end_ms"] as? Int,
            let text = json["text"] as? String
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // An unattributed source omits the field; a present speaker must still be a string.
        let speaker: String?
        if json["speaker"] == nil {
            speaker = nil
        } else if let value = json["speaker"] as? String {
            speaker = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return Source(
            id: id,
            sessionID: sessionID,
            startMilliseconds: startMilliseconds,
            endMilliseconds: endMilliseconds,
            speaker: speaker,
            text: text
        )
    }

    static func decodeAddTranscriptSegmentResponse(_ response: String) throws -> TranscriptSegment {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "add_transcript_segment_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeTranscriptSegment(json["segment"])
    }

    static func decodeListTranscriptResponse(_ response: String) throws -> TranscriptPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_transcript_response",
            let segments = json["segments"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return TranscriptPage(
            segments: try segments.map { try decodeTranscriptSegment($0) },
            truncated: truncated
        )
    }

    private static func decodeTranscriptSegment(_ value: Any?) throws -> TranscriptSegment {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let sessionID = json["session_id"] as? String, !sessionID.isEmpty,
            let startMilliseconds = json["start_ms"] as? Int,
            let endMilliseconds = json["end_ms"] as? Int,
            let text = json["text"] as? String
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // An unattributed segment omits the field; a present speaker must still be a string.
        let speaker: String?
        if json["speaker"] == nil {
            speaker = nil
        } else if let value = json["speaker"] as? String {
            speaker = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return TranscriptSegment(
            id: id,
            sessionID: sessionID,
            startMilliseconds: startMilliseconds,
            endMilliseconds: endMilliseconds,
            speaker: speaker,
            text: text
        )
    }

    static func decodeAddMarkerResponse(_ response: String) throws -> Marker {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "add_marker_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeMarker(json["marker"])
    }

    static func decodeListMarkersResponse(_ response: String) throws -> MarkerListPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_markers_response",
            let markers = json["markers"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return MarkerListPage(
            markers: try markers.map { try decodeMarker($0) },
            truncated: truncated
        )
    }

    private static func decodeMarker(_ value: Any?) throws -> Marker {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let sessionID = json["session_id"] as? String, !sessionID.isEmpty,
            let atMilliseconds = json["at_ms"] as? Int,
            let kind = json["kind"] as? String, !kind.isEmpty
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // A marker without a note omits the field; a present note must still be a string.
        let note: String?
        if json["note"] == nil {
            note = nil
        } else if let value = json["note"] as? String {
            note = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return Marker(
            id: id,
            sessionID: sessionID,
            atMilliseconds: atMilliseconds,
            kind: kind,
            note: note
        )
    }

    static func decodeListRunsResponse(_ response: String) throws -> RunListPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_runs_response",
            let runs = json["runs"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return RunListPage(
            runs: try runs.map { try decodeRunRecord($0) },
            truncated: truncated
        )
    }

    private static func decodeRunRecord(_ value: Any?) throws -> RunRecord {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let runID = json["run_id"] as? String, !runID.isEmpty,
            let executable = json["executable"] as? String,
            let status = json["status"] as? String,
            let startedAtMilliseconds = json["started_at_ms"] as? Int
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // An unlinked run omits the field; a present link must still be a string.
        let sessionID: String?
        if json["session_id"] == nil {
            sessionID = nil
        } else if let value = json["session_id"] as? String {
            sessionID = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        // An exit code is always stated, null included, mirroring the terminal run_exit frame.
        let exitCode: Int?
        if json["exit_code"] is NSNull {
            exitCode = nil
        } else if let value = json["exit_code"] as? Int {
            exitCode = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        // A run that ended cleanly omits the field; a present code must still be a string.
        let errorCode: String?
        if json["error_code"] == nil {
            errorCode = nil
        } else if let value = json["error_code"] as? String {
            errorCode = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        // A live run omits the field; a present end must still be a whole millisecond count.
        let endedAtMilliseconds: Int?
        if json["ended_at_ms"] == nil {
            endedAtMilliseconds = nil
        } else if let value = json["ended_at_ms"] as? Int {
            endedAtMilliseconds = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return RunRecord(
            id: id,
            runID: runID,
            sessionID: sessionID,
            executable: executable,
            status: status,
            exitCode: exitCode,
            errorCode: errorCode,
            startedAtMilliseconds: startedAtMilliseconds,
            endedAtMilliseconds: endedAtMilliseconds
        )
    }

    static func decodeListRunEventsResponse(_ response: String) throws -> RunEventPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_run_events_response",
            let events = json["events"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return RunEventPage(
            events: try events.map { try decodeRunEvent($0) },
            truncated: truncated
        )
    }

    private static func decodeRunEvent(_ value: Any?) throws -> RunEvent {
        // Every field is backend-authored and always stated, so a missing one is not a
        // decodable event.
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let recordID = json["record_id"] as? String, !recordID.isEmpty,
            let sequence = json["seq"] as? Int,
            let atMilliseconds = json["at_ms"] as? Int,
            let kind = json["kind"] as? String, !kind.isEmpty
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return RunEvent(
            id: id,
            recordID: recordID,
            sequence: sequence,
            atMilliseconds: atMilliseconds,
            kind: kind
        )
    }

    static func decodeCreateActionResponse(_ response: String) throws -> ActionRecord {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "create_action_response"
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return try decodeActionRecord(json["action"])
    }

    static func decodeListActionsResponse(_ response: String) throws -> ActionPage {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "list_actions_response",
            let actions = json["actions"] as? [Any],
            let truncated = json["truncated"] as? Bool
        else {
            throw IPCClientError.invalidSessionResponse
        }

        return ActionPage(
            actions: try actions.map { try decodeActionRecord($0) },
            truncated: truncated
        )
    }

    private static func decodeActionRecord(_ value: Any?) throws -> ActionRecord {
        guard
            let json = value as? [String: Any],
            let id = json["id"] as? String, !id.isEmpty,
            let kind = json["kind"] as? String, !kind.isEmpty,
            let title = json["title"] as? String, !title.isEmpty,
            let status = json["status"] as? String, !status.isEmpty,
            let createdAtMilliseconds = json["created_at_ms"] as? Int,
            let updatedAtMilliseconds = json["updated_at_ms"] as? Int
        else {
            throw IPCClientError.invalidSessionResponse
        }

        // An unlinked action omits the field; a present link must still be a string.
        let sessionID: String?
        if json["session_id"] == nil {
            sessionID = nil
        } else if let value = json["session_id"] as? String {
            sessionID = value
        } else {
            throw IPCClientError.invalidSessionResponse
        }

        return ActionRecord(
            id: id,
            sessionID: sessionID,
            kind: kind,
            title: title,
            status: status,
            createdAtMilliseconds: createdAtMilliseconds,
            updatedAtMilliseconds: updatedAtMilliseconds
        )
    }

    static func decodeCancelProcessResponse(
        _ response: String,
        expectedRunID: String
    ) throws -> CancelProcessResult {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "cancel_response",
            json["run_id"] as? String == expectedRunID,
            let status = json["status"] as? String
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch status {
        case "accepted": return .accepted
        case "not_found": return .notFound
        default: throw IPCClientError.invalidProcessEvent
        }
    }

    static func decodePauseProcessResponse(
        _ response: String,
        expectedRunID: String
    ) throws -> PauseProcessResult {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "pause_response",
            json["run_id"] as? String == expectedRunID,
            let status = json["status"] as? String
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch status {
        case "accepted": return .accepted
        case "not_found": return .notFound
        default: throw IPCClientError.invalidProcessEvent
        }
    }

    static func decodeResumeProcessResponse(
        _ response: String,
        expectedRunID: String
    ) throws -> ResumeProcessResult {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "resume_response",
            json["run_id"] as? String == expectedRunID,
            let status = json["status"] as? String
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch status {
        case "accepted": return .accepted
        case "not_found": return .notFound
        default: throw IPCClientError.invalidProcessEvent
        }
    }

    static func decodeSendInputResponse(
        _ response: String,
        expectedRunID: String
    ) throws -> SendInputResult {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "input_response",
            json["run_id"] as? String == expectedRunID,
            let status = json["status"] as? String
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch status {
        case "accepted": return .accepted
        case "not_found": return .notFound
        case "closed": return .closed
        case "capacity_exhausted": return .capacityExhausted
        default: throw IPCClientError.invalidProcessEvent
        }
    }

    static func decodeCloseStdinResponse(
        _ response: String,
        expectedRunID: String
    ) throws -> CloseStdinResult {
        guard
            let data = response.data(using: .utf8),
            let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
            json["version"] as? Int == 1,
            json["type"] as? String == "close_stdin_response",
            json["run_id"] as? String == expectedRunID,
            let status = json["status"] as? String
        else {
            throw IPCClientError.invalidProcessEvent
        }

        switch status {
        case "accepted": return .accepted
        case "not_found": return .notFound
        default: throw IPCClientError.invalidProcessEvent
        }
    }

    private static func jsonString(_ value: String) -> String {
        let data = try! JSONSerialization.data(withJSONObject: [value])
        let array = String(decoding: data, as: UTF8.self)
        return String(array.dropFirst().dropLast())
    }

    private static func jsonStringArray(_ values: [String]) -> String {
        let data = try! JSONSerialization.data(withJSONObject: values)
        return String(decoding: data, as: UTF8.self)
    }

    private static func remainingTimeoutMilliseconds(until deadline: UInt64) throws -> Int32 {
        let now = DispatchTime.now().uptimeNanoseconds
        guard now < deadline else {
            throw POSIXError(.ETIMEDOUT)
        }

        let remainingNanoseconds = deadline - now
        let roundedUpMilliseconds = (remainingNanoseconds + 999_999) / 1_000_000
        return Int32(min(roundedUpMilliseconds, UInt64(Int32.max)))
    }

    private static func suppressSIGPIPE(on descriptor: Int32) throws {
        var enabled: Int32 = 1
        guard
            setsockopt(
                descriptor,
                SOL_SOCKET,
                SO_NOSIGPIPE,
                &enabled,
                socklen_t(MemoryLayout.size(ofValue: enabled))
            ) == 0
        else {
            throw posixError()
        }
    }

    private static func posixError() -> POSIXError {
        POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }
}
