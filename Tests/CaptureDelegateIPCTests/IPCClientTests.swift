import Darwin
import Foundation
import Testing

@testable import CaptureDelegateIPC

@Test("health request JSON is newline delimited")
func healthRequestJSON() {
    #expect(IPCClient.healthRequest == "{\"version\":1,\"type\":\"health\"}\n")
}

@Test("start process request JSON is typed and newline delimited")
func startProcessRequestJSON() {
    let request = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250
    )

    #expect(request.hasSuffix("\n"))
    let data = try! #require(request.dropLast().data(using: .utf8))
    let json = try! #require(try! JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["version"] as? Int == 1)
    #expect(json["type"] as? String == "start_process")
    #expect(json["run_id"] as? String == "run-1")
    #expect(json["executable"] as? String == "/bin/cat")
    #expect(json["arguments"] as? [String] == ["first", "second"])
    #expect(json["timeout_milliseconds"] as? Int == 250)
    #expect(!request.contains("pty"))
    #expect(!request.contains("input_wait_detect_milliseconds"))
    #expect(!request.contains("worktree_repository"))

    let ptyRequest = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250,
        pty: true
    )
    #expect(ptyRequest.hasSuffix(",\"pty\":true}\n"))
    #expect(ptyRequest.components(separatedBy: ",\"pty\":true").count == 2)

    let inputWaitRequest = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250,
        inputWaitDetectMilliseconds: 500
    )
    #expect(
        inputWaitRequest.hasSuffix(",\"input_wait_detect_milliseconds\":500}\n")
            && inputWaitRequest.components(
                separatedBy: ",\"input_wait_detect_milliseconds\":500"
            ).count == 2
    )

    let ptyInputWaitRequest = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250,
        pty: true,
        inputWaitDetectMilliseconds: 500
    )
    #expect(
        ptyInputWaitRequest.hasSuffix(
            ",\"pty\":true,\"input_wait_detect_milliseconds\":500}\n")
    )

    let worktreeRequest = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250,
        worktreeRepository: "/tmp/repo"
    )
    #expect(worktreeRequest.hasSuffix(",\"worktree_repository\":\"\\/tmp\\/repo\"}\n"))
    #expect(worktreeRequest.components(separatedBy: "\"worktree_repository\"").count == 2)
    let worktreeData = try! #require(worktreeRequest.dropLast().data(using: .utf8))
    let worktreeJSON = try! #require(
        try! JSONSerialization.jsonObject(with: worktreeData) as? [String: Any])
    #expect(worktreeJSON["worktree_repository"] as? String == "/tmp/repo")

    let allOptionalFieldsRequest = IPCClient.startProcessRequest(
        runID: "run-1",
        executable: "/bin/cat",
        arguments: ["first", "second"],
        timeoutMilliseconds: 250,
        pty: true,
        inputWaitDetectMilliseconds: 500,
        worktreeRepository: "/tmp/repo"
    )
    #expect(
        allOptionalFieldsRequest.hasSuffix(
            ",\"pty\":true,\"input_wait_detect_milliseconds\":500"
                + ",\"worktree_repository\":\"\\/tmp\\/repo\"}\n")
    )
    #expect(allOptionalFieldsRequest.components(separatedBy: "\"pty\"").count == 2)
    #expect(
        allOptionalFieldsRequest.components(
            separatedBy: "\"input_wait_detect_milliseconds\""
        ).count == 2
    )
    #expect(
        allOptionalFieldsRequest.components(separatedBy: "\"worktree_repository\"").count == 2
    )
}

@Test("cancel process request JSON and accepted result are typed")
func cancelProcessRequestAndAcceptedResult() throws {
    let request = IPCClient.cancelProcessRequest(runID: "run-1")
    #expect(request == "{\"version\":1,\"type\":\"cancel_process\",\"run_id\":\"run-1\"}\n")

    #expect(
        try IPCClient.decodeCancelProcessResponse(
            "{\"version\":1,\"type\":\"cancel_response\",\"run_id\":\"run-1\",\"status\":\"accepted\"}\n",
            expectedRunID: "run-1"
        ) == .accepted
    )
    #expect(
        try IPCClient.decodeCancelProcessResponse(
            "{\"version\":1,\"type\":\"cancel_response\",\"run_id\":\"run-1\",\"status\":\"not_found\"}\n",
            expectedRunID: "run-1"
        ) == .notFound
    )
}

@Test("send input capacity exhaustion is decoded as a typed result")
func sendInputCapacityExhaustionIsTyped() throws {
    #expect(
        try IPCClient.decodeSendInputResponse(
            "{\"version\":1,\"type\":\"input_response\",\"run_id\":\"run-1\",\"status\":\"capacity_exhausted\"}\n",
            expectedRunID: "run-1"
        ) == .capacityExhausted
    )
}

@Test("create session request JSON is typed and its response decodes a session")
func createSessionRequestAndResponse() throws {
    let request = IPCClient.createSessionRequest(title: "Weekly \"review\"")
    #expect(request.hasSuffix("\n"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["version"] as? Int == 1)
    #expect(json["type"] as? String == "create_session")
    #expect(json["title"] as? String == "Weekly \"review\"")

    let session = try IPCClient.decodeCreateSessionResponse(
        "{\"version\":1,\"type\":\"create_session_response\",\"session\":"
            + "{\"id\":\"session-1\",\"title\":\"Weekly review\",\"created_at_ms\":1700000000000,"
            + "\"updated_at_ms\":1700000000001}}\n"
    )
    #expect(
        session
            == Session(
                id: "session-1",
                title: "Weekly review",
                createdAtMilliseconds: 1_700_000_000_000,
                updatedAtMilliseconds: 1_700_000_000_001
            )
    )

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeCreateSessionResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"invalid_create_session\"}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeCreateSessionResponse(
            "{\"version\":1,\"type\":\"create_session_response\",\"session\":"
                + "{\"id\":\"session-1\",\"title\":\"Weekly review\"}}\n"
        )
    }
}

@Test("a session kind travels in the create request and back in its response")
func createSessionRequestAndResponseCarryAKind() throws {
    let request = IPCClient.createSessionRequest(title: "Standup", kind: "meeting")
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["type"] as? String == "create_session")
    #expect(json["title"] as? String == "Standup")
    #expect(json["kind"] as? String == "meeting")

    let session = try IPCClient.decodeCreateSessionResponse(
        "{\"version\":1,\"type\":\"create_session_response\",\"session\":"
            + "{\"id\":\"session-1\",\"title\":\"Standup\",\"created_at_ms\":1,"
            + "\"updated_at_ms\":1,\"kind\":\"meeting\"}}\n"
    )
    #expect(
        session
            == Session(
                id: "session-1",
                title: "Standup",
                createdAtMilliseconds: 1,
                updatedAtMilliseconds: 1,
                kind: "meeting"
            )
    )
}

@Test("an uncategorized session omits the kind key in both directions")
func uncategorizedSessionsOmitTheKindKey() throws {
    let request = IPCClient.createSessionRequest(title: "No kind chosen")
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["kind"] == nil)

    let session = try IPCClient.decodeCreateSessionResponse(
        "{\"version\":1,\"type\":\"create_session_response\",\"session\":"
            + "{\"id\":\"session-1\",\"title\":\"No kind chosen\",\"created_at_ms\":1,"
            + "\"updated_at_ms\":1}}\n"
    )
    #expect(session.kind == nil)
}

@Test("a non-string kind makes the session response invalid")
func nonStringKindIsRejected() throws {
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeCreateSessionResponse(
            "{\"version\":1,\"type\":\"create_session_response\",\"session\":"
                + "{\"id\":\"session-1\",\"title\":\"Standup\",\"created_at_ms\":1,"
                + "\"updated_at_ms\":1,\"kind\":7}}\n"
        )
    }
}

@Test("list sessions request JSON is typed and its response decodes ordered sessions")
func listSessionsRequestAndResponse() throws {
    #expect(IPCClient.listSessionsRequest == "{\"version\":1,\"type\":\"list_sessions\"}\n")

    let page = try IPCClient.decodeListSessionsResponse(
        "{\"version\":1,\"type\":\"list_sessions_response\",\"sessions\":["
            + "{\"id\":\"session-2\",\"title\":\"Newer\",\"created_at_ms\":2,\"updated_at_ms\":2},"
            + "{\"id\":\"session-1\",\"title\":\"Older\",\"created_at_ms\":1,\"updated_at_ms\":1}],"
            + "\"truncated\":false}\n"
    )
    #expect(
        page
            == SessionListPage(
                sessions: [
                    Session(
                        id: "session-2", title: "Newer", createdAtMilliseconds: 2,
                        updatedAtMilliseconds: 2),
                    Session(
                        id: "session-1", title: "Older", createdAtMilliseconds: 1,
                        updatedAtMilliseconds: 1),
                ],
                truncated: false
            )
    )

    let truncatedPage = try IPCClient.decodeListSessionsResponse(
        "{\"version\":1,\"type\":\"list_sessions_response\",\"sessions\":[],\"truncated\":true}\n"
    )
    #expect(truncatedPage.sessions.isEmpty)
    #expect(truncatedPage.truncated)

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListSessionsResponse(
            "{\"version\":2,\"type\":\"list_sessions_response\",\"sessions\":[],\"truncated\":false}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListSessionsResponse(
            "{\"version\":1,\"type\":\"list_sessions_response\",\"sessions\":[]}\n"
        )
    }
}

@Test("set session note request JSON is typed and its response decodes the updated session")
func setSessionNoteRequestAndResponse() throws {
    let request = IPCClient.setSessionNoteRequest(
        sessionID: "session-1", note: "Follow up with \"Sarah\"")
    #expect(request.hasSuffix("\n"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["version"] as? Int == 1)
    #expect(json["type"] as? String == "set_session_note")
    #expect(json["session_id"] as? String == "session-1")
    #expect(json["note"] as? String == "Follow up with \"Sarah\"")

    let session = try IPCClient.decodeSetSessionNoteResponse(
        "{\"version\":1,\"type\":\"set_session_note_response\",\"session\":"
            + "{\"id\":\"session-1\",\"title\":\"Sprint planning\",\"created_at_ms\":1,"
            + "\"updated_at_ms\":2,\"kind\":\"meeting\",\"note\":\"Follow up with Sarah\"}}\n"
    )
    #expect(
        session
            == Session(
                id: "session-1",
                title: "Sprint planning",
                createdAtMilliseconds: 1,
                updatedAtMilliseconds: 2,
                kind: "meeting",
                note: "Follow up with Sarah"
            )
    )

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeSetSessionNoteResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"invalid_set_session_note\"}\n"
        )
    }
}

@Test("clearing a note states an explicit null, and a cleared session omits the note key")
func clearingASessionNoteStatesAnExplicitNull() throws {
    let request = IPCClient.setSessionNoteRequest(sessionID: "session-1", note: nil)
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    // Clearing is an update, not an omission: the key must be present and null.
    #expect(json.keys.contains("note"))
    #expect(json["note"] is NSNull)

    let session = try IPCClient.decodeSetSessionNoteResponse(
        "{\"version\":1,\"type\":\"set_session_note_response\",\"session\":"
            + "{\"id\":\"session-1\",\"title\":\"Sprint planning\",\"created_at_ms\":1,"
            + "\"updated_at_ms\":2}}\n"
    )
    #expect(session.note == nil)
}

@Test("a non-string session note makes the response invalid")
func nonStringSessionNoteIsRejected() throws {
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeSetSessionNoteResponse(
            "{\"version\":1,\"type\":\"set_session_note_response\",\"session\":"
                + "{\"id\":\"session-1\",\"title\":\"Sprint planning\",\"created_at_ms\":1,"
                + "\"updated_at_ms\":2,\"note\":7}}\n"
        )
    }
}

@Test("add source request JSON is typed and its response decodes a source")
func addSourceRequestAndResponse() throws {
    let request = IPCClient.addSourceRequest(
        sessionID: "session-1",
        startMilliseconds: 872_000,
        endMilliseconds: 884_000,
        speaker: "Sarah",
        text: "Check PR \"482\""
    )
    #expect(request.hasSuffix("\n"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["version"] as? Int == 1)
    #expect(json["type"] as? String == "add_source")
    #expect(json["session_id"] as? String == "session-1")
    #expect(json["start_ms"] as? Int == 872_000)
    #expect(json["end_ms"] as? Int == 884_000)
    #expect(json["speaker"] as? String == "Sarah")
    #expect(json["text"] as? String == "Check PR \"482\"")

    let source = try IPCClient.decodeAddSourceResponse(
        "{\"version\":1,\"type\":\"add_source_response\",\"source\":"
            + "{\"id\":\"source-1\",\"session_id\":\"session-1\",\"start_ms\":872000,"
            + "\"end_ms\":884000,\"speaker\":\"Sarah\",\"text\":\"Check PR 482\"}}\n"
    )
    #expect(
        source
            == Source(
                id: "source-1",
                sessionID: "session-1",
                startMilliseconds: 872_000,
                endMilliseconds: 884_000,
                speaker: "Sarah",
                text: "Check PR 482"
            )
    )

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddSourceResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"invalid_add_source\"}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddSourceResponse(
            "{\"version\":1,\"type\":\"add_source_response\",\"source\":"
                + "{\"id\":\"source-1\",\"session_id\":\"session-1\",\"start_ms\":0}}\n"
        )
    }
}

@Test("an unattributed source omits the speaker key in both directions")
func unattributedSourcesOmitTheSpeakerKey() throws {
    let request = IPCClient.addSourceRequest(
        sessionID: "session-1",
        startMilliseconds: 1000,
        endMilliseconds: 1000,
        text: "Zero-length unattributed span"
    )
    #expect(!request.contains("speaker"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["speaker"] == nil)

    let source = try IPCClient.decodeAddSourceResponse(
        "{\"version\":1,\"type\":\"add_source_response\",\"source\":"
            + "{\"id\":\"source-1\",\"session_id\":\"session-1\",\"start_ms\":1000,"
            + "\"end_ms\":1000,\"text\":\"Zero-length unattributed span\"}}\n"
    )
    #expect(source.speaker == nil)
}

@Test("a non-string speaker makes the source response invalid")
func nonStringSpeakerIsRejected() throws {
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddSourceResponse(
            "{\"version\":1,\"type\":\"add_source_response\",\"source\":"
                + "{\"id\":\"source-1\",\"session_id\":\"session-1\",\"start_ms\":0,"
                + "\"end_ms\":1,\"speaker\":7,\"text\":\"t\"}}\n"
        )
    }
}

@Test("list sources request JSON is typed and its response decodes ordered sources")
func listSourcesRequestAndResponse() throws {
    #expect(
        IPCClient.listSourcesRequest(sessionID: "session-1")
            == "{\"version\":1,\"type\":\"list_sources\",\"session_id\":\"session-1\"}\n"
    )

    let page = try IPCClient.decodeListSourcesResponse(
        "{\"version\":1,\"type\":\"list_sources_response\",\"sources\":["
            + "{\"id\":\"source-1\",\"session_id\":\"session-1\",\"start_ms\":1,\"end_ms\":2,"
            + "\"text\":\"Earlier\"},"
            + "{\"id\":\"source-2\",\"session_id\":\"session-1\",\"start_ms\":3,\"end_ms\":4,"
            + "\"speaker\":\"Sarah\",\"text\":\"Later\"}],"
            + "\"truncated\":false}\n"
    )
    #expect(
        page
            == SourceListPage(
                sources: [
                    Source(
                        id: "source-1", sessionID: "session-1", startMilliseconds: 1,
                        endMilliseconds: 2, text: "Earlier"),
                    Source(
                        id: "source-2", sessionID: "session-1", startMilliseconds: 3,
                        endMilliseconds: 4, speaker: "Sarah", text: "Later"),
                ],
                truncated: false
            )
    )

    let truncatedPage = try IPCClient.decodeListSourcesResponse(
        "{\"version\":1,\"type\":\"list_sources_response\",\"sources\":[],\"truncated\":true}\n"
    )
    #expect(truncatedPage.sources.isEmpty)
    #expect(truncatedPage.truncated)

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListSourcesResponse(
            "{\"version\":1,\"type\":\"list_sources_response\",\"sources\":[]}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListSourcesResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"unknown_session\"}\n"
        )
    }
}

@Test("add marker request JSON is typed and its response decodes a marker")
func addMarkerRequestAndResponse() throws {
    let request = IPCClient.addMarkerRequest(
        sessionID: "session-1",
        atMilliseconds: 872_000,
        kind: "decision",
        note: "Ship behind a \"flag\""
    )
    #expect(request.hasSuffix("\n"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["version"] as? Int == 1)
    #expect(json["type"] as? String == "add_marker")
    #expect(json["session_id"] as? String == "session-1")
    #expect(json["at_ms"] as? Int == 872_000)
    #expect(json["kind"] as? String == "decision")
    #expect(json["note"] as? String == "Ship behind a \"flag\"")

    let marker = try IPCClient.decodeAddMarkerResponse(
        "{\"version\":1,\"type\":\"add_marker_response\",\"marker\":"
            + "{\"id\":\"marker-1\",\"session_id\":\"session-1\",\"at_ms\":872000,"
            + "\"kind\":\"decision\",\"note\":\"Ship behind a flag\"}}\n"
    )
    #expect(
        marker
            == Marker(
                id: "marker-1",
                sessionID: "session-1",
                atMilliseconds: 872_000,
                kind: "decision",
                note: "Ship behind a flag"
            )
    )

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddMarkerResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"invalid_add_marker\"}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddMarkerResponse(
            "{\"version\":1,\"type\":\"add_marker_response\",\"marker\":"
                + "{\"id\":\"marker-1\",\"session_id\":\"session-1\",\"at_ms\":0}}\n"
        )
    }
}

@Test("a marker without a note omits the note key in both directions")
func noteFreeMarkersOmitTheNoteKey() throws {
    let request = IPCClient.addMarkerRequest(
        sessionID: "session-1",
        atMilliseconds: 1000,
        kind: "important"
    )
    #expect(!request.contains("note"))
    let data = try #require(request.dropLast().data(using: .utf8))
    let json = try #require(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    #expect(json["note"] == nil)

    let marker = try IPCClient.decodeAddMarkerResponse(
        "{\"version\":1,\"type\":\"add_marker_response\",\"marker\":"
            + "{\"id\":\"marker-1\",\"session_id\":\"session-1\",\"at_ms\":1000,"
            + "\"kind\":\"important\"}}\n"
    )
    #expect(marker.note == nil)
}

@Test("a non-string note makes the marker response invalid")
func nonStringNoteIsRejected() throws {
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeAddMarkerResponse(
            "{\"version\":1,\"type\":\"add_marker_response\",\"marker\":"
                + "{\"id\":\"marker-1\",\"session_id\":\"session-1\",\"at_ms\":0,"
                + "\"kind\":\"important\",\"note\":7}}\n"
        )
    }
}

@Test("list markers request JSON is typed and its response decodes ordered markers")
func listMarkersRequestAndResponse() throws {
    #expect(
        IPCClient.listMarkersRequest(sessionID: "session-1")
            == "{\"version\":1,\"type\":\"list_markers\",\"session_id\":\"session-1\"}\n"
    )

    let page = try IPCClient.decodeListMarkersResponse(
        "{\"version\":1,\"type\":\"list_markers_response\",\"markers\":["
            + "{\"id\":\"marker-1\",\"session_id\":\"session-1\",\"at_ms\":1,"
            + "\"kind\":\"important\"},"
            + "{\"id\":\"marker-2\",\"session_id\":\"session-1\",\"at_ms\":3,"
            + "\"kind\":\"action\",\"note\":\"Follow up\"}],"
            + "\"truncated\":false}\n"
    )
    #expect(
        page
            == MarkerListPage(
                markers: [
                    Marker(
                        id: "marker-1", sessionID: "session-1", atMilliseconds: 1,
                        kind: "important"),
                    Marker(
                        id: "marker-2", sessionID: "session-1", atMilliseconds: 3,
                        kind: "action", note: "Follow up"),
                ],
                truncated: false
            )
    )

    let truncatedPage = try IPCClient.decodeListMarkersResponse(
        "{\"version\":1,\"type\":\"list_markers_response\",\"markers\":[],\"truncated\":true}\n"
    )
    #expect(truncatedPage.markers.isEmpty)
    #expect(truncatedPage.truncated)

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListMarkersResponse(
            "{\"version\":1,\"type\":\"list_markers_response\",\"markers\":[]}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListMarkersResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"unknown_session\"}\n"
        )
    }
}

@Test("list runs request JSON is typed and its response decodes live and finished runs")
func listRunsRequestAndResponse() throws {
    #expect(IPCClient.listRunsRequest == "{\"version\":1,\"type\":\"list_runs\"}\n")

    let page = try IPCClient.decodeListRunsResponse(
        "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":["
            + "{\"id\":\"run-record-2\",\"run_id\":\"live-run\",\"executable\":\"/bin/sleep\","
            + "\"status\":\"running\",\"exit_code\":null,\"started_at_ms\":20},"
            + "{\"id\":\"run-record-1\",\"run_id\":\"done-run\",\"session_id\":\"session-1\","
            + "\"executable\":\"/usr/bin/true\",\"status\":\"exited\",\"exit_code\":0,"
            + "\"started_at_ms\":10,\"ended_at_ms\":11}],"
            + "\"truncated\":false}\n"
    )
    #expect(
        page
            == RunListPage(
                runs: [
                    RunRecord(
                        id: "run-record-2", runID: "live-run", executable: "/bin/sleep",
                        status: "running", startedAtMilliseconds: 20),
                    RunRecord(
                        id: "run-record-1", runID: "done-run", sessionID: "session-1",
                        executable: "/usr/bin/true", status: "exited", exitCode: 0,
                        startedAtMilliseconds: 10, endedAtMilliseconds: 11),
                ],
                truncated: false
            )
    )

    let truncatedPage = try IPCClient.decodeListRunsResponse(
        "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":[],\"truncated\":true}\n"
    )
    #expect(truncatedPage.runs.isEmpty)
    #expect(truncatedPage.truncated)

    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListRunsResponse(
            "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":[]}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListRunsResponse(
            "{\"version\":1,\"type\":\"error\",\"code\":\"store_unavailable\"}\n"
        )
    }
}

@Test("an interrupted run decodes with its error code and without an exit code")
func interruptedRunDecodes() throws {
    let page = try IPCClient.decodeListRunsResponse(
        "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":["
            + "{\"id\":\"run-record-3\",\"run_id\":\"late-run\",\"executable\":\"/bin/sleep\","
            + "\"status\":\"exited\",\"exit_code\":null,\"error_code\":\"timed_out\","
            + "\"started_at_ms\":10,\"ended_at_ms\":110}],\"truncated\":false}\n"
    )
    let run = try #require(page.runs.first)
    #expect(run.exitCode == nil)
    #expect(run.errorCode == "timed_out")
    #expect(run.sessionID == nil)

    // exit_code is always stated on the wire, so a missing one is not a decodable record.
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListRunsResponse(
            "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":["
                + "{\"id\":\"run-record-4\",\"run_id\":\"late-run\",\"executable\":\"/bin/sleep\","
                + "\"status\":\"exited\",\"started_at_ms\":10}],\"truncated\":false}\n"
        )
    }
    #expect(throws: IPCClientError.invalidSessionResponse) {
        try IPCClient.decodeListRunsResponse(
            "{\"version\":1,\"type\":\"list_runs_response\",\"runs\":["
                + "{\"id\":\"run-record-5\",\"run_id\":\"late-run\",\"executable\":\"/bin/sleep\","
                + "\"status\":\"exited\",\"exit_code\":null,\"error_code\":7,"
                + "\"started_at_ms\":10}],\"truncated\":false}\n"
        )
    }
}

@Test("closed peer returns an error without SIGPIPE termination")
func closedPeerDoesNotRaiseSIGPIPE() throws {
    var descriptors = [Int32](repeating: -1, count: 2)
    #expect(socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0)
    _ = Darwin.close(descriptors[0])
    defer { _ = Darwin.close(descriptors[1]) }

    var receivedError = false
    do {
        try IPCClient.writeHealthRequest(to: descriptors[1])
    } catch {
        receivedError = true
    }
    #expect(receivedError)
}

@Test("a quiet process may send its first output frame after the frame deadline")
func delayedFirstProcessFrameSucceeds() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let response =
        "{\"version\":1,\"type\":\"run_output\",\"run_id\":\"quiet-run\",\"stream\":\"stdout\",\"output\":\"ready\"}\n"
    let writerFinished = DispatchSemaphore(value: 0)
    Thread {
        defer { writerFinished.signal() }
        usleep(700_000)
        try? writeAll(Array(response.utf8), to: descriptors.writer)
    }.start()

    let started = Date()
    var receivedResponse: String?
    var receivedError = false
    do {
        receivedResponse = try IPCClient.readResponseLine(from: descriptors.reader)
    } catch {
        receivedError = true
    }

    #expect(writerFinished.wait(timeout: .now() + 2) == .success)
    #expect(!receivedError)
    #expect(receivedResponse == response)
    #expect(Date().timeIntervalSince(started) >= 0.6)
    #expect(Date().timeIntervalSince(started) < 2)
}

@Test("byte-dribbled responses honor one total monotonic deadline")
func byteDribbledResponseIsBoundedByTotalDeadline() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let writerFinished = DispatchSemaphore(value: 0)
    Thread {
        defer { writerFinished.signal() }
        for byte in Array("dribble".utf8) {
            _ = Darwin.write(descriptors.writer, [byte], 1)
            usleep(100_000)
        }
    }.start()

    let started = Date()
    var receivedError = false
    do {
        _ = try IPCClient.readResponseLine(from: descriptors.reader)
    } catch {
        receivedError = true
    }

    #expect(receivedError)
    #expect(Date().timeIntervalSince(started) < 0.9)
    #expect(writerFinished.wait(timeout: .now() + 2) == .success)
}

@Test("oversized response fails within the frame bound")
func oversizedResponseIsRejected() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }
    var sendBufferBytes: Int32 = 32 * 1024
    #expect(
        setsockopt(
            descriptors.writer,
            SOL_SOCKET,
            SO_SNDBUF,
            &sendBufferBytes,
            socklen_t(MemoryLayout.size(ofValue: sendBufferBytes))
        ) == 0
    )
    let oversized = [UInt8](repeating: 120, count: 8 * 1024 + 1) + [10]
    try writeAll(oversized, to: descriptors.writer)

    do {
        _ = try IPCClient.readResponseLine(from: descriptors.reader)
        Issue.record("oversized response should be rejected")
    } catch let error as IPCClientError {
        #expect(error == .responseTooLarge)
    } catch {
        Issue.record("unexpected error: \(error)")
    }
}

@Test("valid response remains readable over a local socket")
func validResponseRemainsReadable() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }
    let response = "{\"version\":1,\"type\":\"health_response\",\"status\":\"ok\"}\n"
    try writeAll(Array(response.utf8), to: descriptors.writer)

    #expect(try IPCClient.readResponseLine(from: descriptors.reader) == response)
}

@Test("process output callback fires before the terminal frame is sent")
func processOutputStreamsBeforeTerminalFrame() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let outputReceived = DispatchSemaphore(value: 0)
    let readingFinished = DispatchSemaphore(value: 0)
    let collector = ProcessEventCollector()
    Thread {
        defer { readingFinished.signal() }
        do {
            try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "run-1") {
                event in
                collector.append(event)
                if case .output = event {
                    outputReceived.signal()
                }
            }
        } catch {
            collector.record(error)
        }
    }.start()

    usleep(300_000)
    try writeAll(
        Array(
            "{\"version\":1,\"type\":\"run_output\",\"run_id\":\"run-1\",\"stream\":\"stdout\",\"output\":\"first\"}\n"
                .utf8),
        to: descriptors.writer
    )
    #expect(outputReceived.wait(timeout: .now() + 1) == .success)
    #expect(readingFinished.wait(timeout: .now()) == .timedOut)

    usleep(300_000)
    try writeAll(
        Array(
            "{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"run-1\",\"exit_code\":0}\n".utf8),
        to: descriptors.writer
    )
    #expect(readingFinished.wait(timeout: .now() + 1) == .success)

    let result = collector.result()
    #expect(result.error == nil)
    #expect(
        result.events == [
            .output(runID: "run-1", stream: .stdout, output: "first"),
            .exit(runID: "run-1", exitCode: 0, errorCode: nil),
        ])
}

@Test("run exit decodes a typed spawn failure")
func spawnFailureExitDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let collector = ProcessEventCollector()
    try writeAll(
        Array(
            "{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"missing-run\",\"exit_code\":null,\"error_code\":\"spawn_failed\"}\n"
                .utf8),
        to: descriptors.writer
    )
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "missing-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .exit(runID: "missing-run", exitCode: nil, errorCode: .spawnFailed)
        ])
}

@Test("run exit decodes a typed worktree failure")
func worktreeFailureExitDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let collector = ProcessEventCollector()
    try writeAll(
        Array(
            ("{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"worktree-run\","
                + "\"exit_code\":null,\"error_code\":\"worktree_failed\"}\n")
                .utf8),
        to: descriptors.writer
    )
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "worktree-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .exit(runID: "worktree-run", exitCode: nil, errorCode: .worktreeFailed)
        ])
}

@Test("run exit decodes a typed internal error")
func internalErrorExitDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    let collector = ProcessEventCollector()
    try writeAll(
        Array(
            ("{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"internal-run\","
                + "\"exit_code\":null,\"error_code\":\"internal_error\"}\n")
                .utf8),
        to: descriptors.writer
    )
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "internal-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .exit(runID: "internal-run", exitCode: nil, errorCode: .internalError)
        ])
}

@Test("run exit decodes a typed capacity exhaustion failure")
func capacityExhaustionExitDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    try writeAll(
        Array(
            "{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"capacity-run\",\"exit_code\":null,\"error_code\":\"capacity_exhausted\"}\n"
                .utf8),
        to: descriptors.writer
    )
    let collector = ProcessEventCollector()
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "capacity-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .exit(runID: "capacity-run", exitCode: nil, errorCode: .capacityExhausted)
        ])
}

@Test("run exit decodes a typed timeout failure")
func timeoutExitDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    try writeAll(
        Array(
            "{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"timeout-run\",\"exit_code\":null,\"error_code\":\"timed_out\"}\n"
                .utf8),
        to: descriptors.writer
    )
    let collector = ProcessEventCollector()
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "timeout-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .exit(runID: "timeout-run", exitCode: nil, errorCode: .timedOut)
        ])
}

@Test("run input waiting decodes before the terminal frame")
func inputWaitingDecodes() throws {
    let descriptors = try localSocketPair()
    defer {
        _ = Darwin.close(descriptors.reader)
        _ = Darwin.close(descriptors.writer)
    }

    try writeAll(
        Array(
            ("{\"version\":1,\"type\":\"run_input_waiting\",\"run_id\":\"wait-run\","
                + "\"quiet_for_milliseconds\":750}\n"
                + "{\"version\":1,\"type\":\"run_exit\",\"run_id\":\"wait-run\",\"exit_code\":0}\n")
                .utf8),
        to: descriptors.writer
    )
    let collector = ProcessEventCollector()
    try IPCClient.readProcessEvents(from: descriptors.reader, expectedRunID: "wait-run") {
        collector.append($0)
    }

    #expect(
        collector.result().events == [
            .inputWaiting(runID: "wait-run", quietForMilliseconds: 750),
            .exit(runID: "wait-run", exitCode: 0, errorCode: nil),
        ])
}

private final class ProcessEventCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var events: [ProcessEvent] = []
    private var error: Error?

    func append(_ event: ProcessEvent) {
        lock.lock()
        events.append(event)
        lock.unlock()
    }

    func record(_ error: Error) {
        lock.lock()
        self.error = error
        lock.unlock()
    }

    func result() -> (events: [ProcessEvent], error: Error?) {
        lock.lock()
        defer { lock.unlock() }
        return (events, error)
    }
}

private func localSocketPair() throws -> (reader: Int32, writer: Int32) {
    var descriptors = [Int32](repeating: -1, count: 2)
    guard socketpair(AF_UNIX, SOCK_STREAM, 0, &descriptors) == 0 else {
        throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }
    return (descriptors[0], descriptors[1])
}

private func writeAll(_ bytes: [UInt8], to descriptor: Int32) throws {
    var offset = 0
    while offset < bytes.count {
        let count = bytes.withUnsafeBytes { buffer in
            Darwin.write(descriptor, buffer.baseAddress!.advanced(by: offset), bytes.count - offset)
        }
        guard count > 0 else {
            throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        offset += count
    }
}
