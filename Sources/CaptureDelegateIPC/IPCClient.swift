import Darwin
import Dispatch
import Foundation

public enum IPCClientError: Error, Equatable {
    case responseTooLarge
}

public enum IPCClient {
    public static let healthRequest = "{\"version\":1,\"type\":\"health\"}\n"
    private static let maximumResponseBytes = 8 * 1024
    private static let responseTimeoutMicroseconds: Int32 = 500_000

    public static func checkHealth(socketPath: String) throws -> Bool {
        let descriptor = try connect(to: socketPath)
        defer { _ = Darwin.close(descriptor) }

        try writeHealthRequest(to: descriptor)
        let response = try readResponseLine(from: descriptor)
        return try validateHealthResponse(response)
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
        let bytes = Array(healthRequest.utf8)
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
        let deadline =
            DispatchTime.now().uptimeNanoseconds + UInt64(responseTimeoutMicroseconds) * 1_000

        while true {
            var pollDescriptor = pollfd(fd: descriptor, events: Int16(POLLIN), revents: 0)
            let pollResult = Darwin.poll(
                &pollDescriptor,
                1,
                try remainingTimeoutMilliseconds(until: deadline)
            )
            if pollResult < 0, errno == EINTR {
                continue
            }
            guard pollResult > 0 else {
                throw POSIXError(.ETIMEDOUT)
            }

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
