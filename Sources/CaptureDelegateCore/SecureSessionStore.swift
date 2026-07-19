import CryptoKit
import Foundation

public struct SecureSessionListProblem: Equatable, @unchecked Sendable {
    public let entryName: String
    public let error: SecureStoreError

    public init(entryName: String, error: SecureStoreError) {
        self.entryName = entryName
        self.error = error
    }
}

public final class SecureSessionStore: @unchecked Sendable {
    private static let audioFilename = "audio.m4a.enc"
    private static let metadataFilename = "metadata.json.enc"

    private let rootDirectory: URL
    private let keyProvider: any SymmetricKeyProviding
    private let lock = NSLock()

    public init(
        rootDirectory: URL? = nil,
        keyProvider: any SymmetricKeyProviding = KeychainKeyProvider()
    ) throws {
        let fileManager = FileManager.default
        self.rootDirectory = rootDirectory ?? Self.defaultRootDirectory(fileManager: fileManager)
        self.keyProvider = keyProvider

        do {
            try fileManager.createDirectory(
                at: self.rootDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try Self.removeStaleStagingDirectories(
                from: self.rootDirectory,
                fileManager: fileManager
            )
        } catch {
            throw SecureStoreError.ioFailure(error.localizedDescription)
        }
    }

    public func list() throws -> [CaptureSession] {
        try listWithProblems().sessions
    }

    public func listWithProblems() throws -> (
        sessions: [CaptureSession], problems: [SecureSessionListProblem]
    ) {
        try lock.withLock {
            let key = try persistentKey()
            let fileManager = FileManager.default
            let entries: [URL]
            do {
                entries = try fileManager.contentsOfDirectory(
                    at: rootDirectory,
                    includingPropertiesForKeys: [.isDirectoryKey],
                    options: [.skipsHiddenFiles]
                )
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }

            var sessions: [CaptureSession] = []
            var problems: [SecureSessionListProblem] = []
            for entry in entries {
                let values: URLResourceValues
                do {
                    values = try entry.resourceValues(forKeys: [.isDirectoryKey])
                } catch {
                    problems.append(
                        SecureSessionListProblem(
                            entryName: entry.lastPathComponent,
                            error: .ioFailure(error.localizedDescription)
                        )
                    )
                    continue
                }
                guard values.isDirectory == true else {
                    continue
                }
                do {
                    sessions.append(try loadSession(at: entry, using: key))
                } catch let error as SecureStoreError {
                    problems.append(
                        SecureSessionListProblem(
                            entryName: entry.lastPathComponent,
                            error: error
                        )
                    )
                } catch {
                    problems.append(
                        SecureSessionListProblem(
                            entryName: entry.lastPathComponent,
                            error: .ioFailure(error.localizedDescription)
                        )
                    )
                }
            }
            sessions.sort { first, second in
                first.createdAt > second.createdAt
            }
            return (sessions, problems)
        }
    }

    public func save(
        audioFileURL: URL,
        title: String,
        note: String,
        createdAt: Date,
        duration: TimeInterval
    ) throws -> CaptureSession {
        try lock.withLock {
            let key = try persistentKey()
            let audioData: Data
            do {
                audioData = try Data(contentsOf: audioFileURL)
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }

            let session = CaptureSession(
                id: UUID(),
                title: title,
                note: note,
                createdAt: createdAt,
                duration: duration,
                source: "Microphone"
            )
            let metadataData: Data
            do {
                metadataData = try JSONEncoder().encode(session)
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }
            let encryptedAudio = try seal(audioData, using: key)
            let encryptedMetadata = try seal(metadataData, using: key)

            let fileManager = FileManager.default
            let finalDirectory = sessionDirectory(for: session.id)
            let stagingDirectory = rootDirectory.appendingPathComponent(
                ".\(session.id.uuidString).staging-\(UUID().uuidString)",
                isDirectory: true
            )

            do {
                try fileManager.createDirectory(
                    at: stagingDirectory,
                    withIntermediateDirectories: false,
                    attributes: [.posixPermissions: 0o700]
                )
                try encryptedAudio.write(
                    to: stagingDirectory.appendingPathComponent(Self.audioFilename),
                    options: .atomic
                )
                try encryptedMetadata.write(
                    to: stagingDirectory.appendingPathComponent(Self.metadataFilename),
                    options: .atomic
                )
                try fileManager.moveItem(at: stagingDirectory, to: finalDirectory)

                do {
                    try fileManager.removeItem(at: audioFileURL)
                } catch {
                    try? fileManager.removeItem(at: finalDirectory)
                    throw SecureStoreError.ioFailure(error.localizedDescription)
                }
                return session
            } catch let error as SecureStoreError {
                try? fileManager.removeItem(at: stagingDirectory)
                throw error
            } catch {
                try? fileManager.removeItem(at: stagingDirectory)
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }
        }
    }

    public func updateTitle(_ title: String, for id: UUID) throws {
        try updateSession(id) { session in
            session.title = title
        }
    }

    public func updateNote(_ note: String, for id: UUID) throws {
        try updateSession(id) { session in
            session.note = note
        }
    }

    public func delete(_ id: UUID) throws {
        try lock.withLock {
            let directory = try existingSessionDirectory(for: id)
            do {
                try FileManager.default.removeItem(at: directory)
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }
        }
    }

    public func loadAudioData(for id: UUID) throws -> Data {
        try lock.withLock {
            let directory = try existingSessionDirectory(for: id)
            let key = try persistentKey()
            return try openEncryptedFile(
                at: directory.appendingPathComponent(Self.audioFilename),
                using: key
            )
        }
    }

    private static func defaultRootDirectory(fileManager: FileManager) -> URL {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("CaptureDelegate", isDirectory: true)
            .appendingPathComponent("sessions", isDirectory: true)
    }

    private static func removeStaleStagingDirectories(
        from rootDirectory: URL,
        fileManager: FileManager
    ) throws {
        let entries = try fileManager.contentsOfDirectory(
            at: rootDirectory,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: []
        )
        for entry in entries where isStagingDirectoryName(entry.lastPathComponent) {
            let values = try entry.resourceValues(forKeys: [.isDirectoryKey])
            guard values.isDirectory == true else { continue }
            try fileManager.removeItem(at: entry)
        }
    }

    private static func isStagingDirectoryName(_ name: String) -> Bool {
        guard name.first == ".", let separator = name.range(of: ".staging-") else {
            return false
        }
        let sessionStart = name.index(after: name.startIndex)
        let sessionID = String(name[sessionStart..<separator.lowerBound])
        let stagingID = String(name[separator.upperBound...])
        return UUID(uuidString: sessionID) != nil && UUID(uuidString: stagingID) != nil
    }

    private func sessionDirectory(for id: UUID) -> URL {
        rootDirectory.appendingPathComponent(id.uuidString, isDirectory: true)
    }

    private func existingSessionDirectory(for id: UUID) throws -> URL {
        let directory = sessionDirectory(for: id)
        var isDirectory: ObjCBool = false
        guard
            FileManager.default.fileExists(
                atPath: directory.path,
                isDirectory: &isDirectory
            ), isDirectory.boolValue
        else {
            throw SecureStoreError.sessionNotFound
        }
        return directory
    }

    private func persistentKey() throws -> SymmetricKey {
        do {
            return try keyProvider.key()
        } catch let error as SecureStoreError {
            if case .keyUnavailable = error {
                throw error
            }
            throw SecureStoreError.keyUnavailable(String(describing: error))
        } catch {
            throw SecureStoreError.keyUnavailable(String(describing: error))
        }
    }

    private func seal(_ data: Data, using key: SymmetricKey) throws -> Data {
        do {
            let sealedBox = try AES.GCM.seal(data, using: key)
            guard let combined = sealedBox.combined else {
                throw SecureStoreError.keyUnavailable("AES-GCM did not produce combined data")
            }
            return combined
        } catch let error as SecureStoreError {
            throw error
        } catch {
            throw SecureStoreError.keyUnavailable(error.localizedDescription)
        }
    }

    private func openEncryptedFile(at url: URL, using key: SymmetricKey) throws -> Data {
        let encryptedData: Data
        do {
            encryptedData = try Data(contentsOf: url)
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            throw SecureStoreError.corruptOrUndecryptable
        } catch {
            throw SecureStoreError.ioFailure(error.localizedDescription)
        }

        do {
            let sealedBox = try AES.GCM.SealedBox(combined: encryptedData)
            return try AES.GCM.open(sealedBox, using: key)
        } catch {
            throw SecureStoreError.corruptOrUndecryptable
        }
    }

    private func loadSession(at directory: URL, using key: SymmetricKey) throws -> CaptureSession {
        let metadata = try openEncryptedFile(
            at: directory.appendingPathComponent(Self.metadataFilename),
            using: key
        )
        do {
            return try JSONDecoder().decode(CaptureSession.self, from: metadata)
        } catch {
            throw SecureStoreError.corruptOrUndecryptable
        }
    }

    private func updateSession(
        _ id: UUID,
        mutation: (inout CaptureSession) -> Void
    ) throws {
        try lock.withLock {
            let directory = try existingSessionDirectory(for: id)
            let key = try persistentKey()
            var session = try loadSession(at: directory, using: key)
            mutation(&session)

            let encoded: Data
            do {
                encoded = try JSONEncoder().encode(session)
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }
            let encrypted = try seal(encoded, using: key)
            do {
                try encrypted.write(
                    to: directory.appendingPathComponent(Self.metadataFilename),
                    options: .atomic
                )
            } catch {
                throw SecureStoreError.ioFailure(error.localizedDescription)
            }
        }
    }
}
