import Darwin
import Dispatch
import Foundation

public enum IPCClientError: Error, Equatable {
    case responseTooLarge
    case invalidProcessEvent
}

public enum ProcessOutputStream: String, Equatable {
    case stdout
    case stderr
}

public enum ProcessExitErrorCode: String, Equatable {
    case spawnFailed = "spawn_failed"
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
}

public enum CloseStdinResult: Equatable {
    case accepted
    case notFound
}

public enum ProcessEvent: Equatable {
    case output(runID: String, stream: ProcessOutputStream, output: String)
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
        onEvent: (ProcessEvent) -> Void
    ) throws {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeStartProcessRequest(
            runID: runID,
            executable: executable,
            arguments: arguments,
            timeoutMilliseconds: timeoutMilliseconds,
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
        timeoutMilliseconds: Int
    ) -> String {
        "{\"version\":1,\"type\":\"start_process\",\"run_id\":\(jsonString(runID)),\"executable\":\(jsonString(executable)),\"arguments\":\(jsonStringArray(arguments)),\"timeout_milliseconds\":\(timeoutMilliseconds)}\n"
    }

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
        to descriptor: Int32
    ) throws {
        try suppressSIGPIPE(on: descriptor)
        try write(
            Array(
                startProcessRequest(
                    runID: runID,
                    executable: executable,
                    arguments: arguments,
                    timeoutMilliseconds: timeoutMilliseconds
                ).utf8
            ), to: descriptor)
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
