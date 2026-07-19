import CryptoKit
import Foundation
import Testing

@testable import CaptureDelegateCore

private struct FixedKeyProvider: SymmetricKeyProviding {
    let storedKey: SymmetricKey

    init(key: SymmetricKey = SymmetricKey(size: .bits256)) {
        storedKey = key
    }

    func key() throws -> SymmetricKey {
        storedKey
    }
}

private struct FailingKeyProvider: SymmetricKeyProviding {
    func key() throws -> SymmetricKey {
        throw SecureStoreError.keyUnavailable("injected key failure")
    }
}

private struct StoreFixture {
    let sandbox: URL
    let root: URL

    init() throws {
        sandbox = FileManager.default.temporaryDirectory
            .appendingPathComponent("CaptureDelegateCoreTests", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        root = sandbox.appendingPathComponent("store", isDirectory: true)
        try FileManager.default.createDirectory(
            at: sandbox,
            withIntermediateDirectories: true
        )
    }

    func makeAudio(_ data: Data, named name: String = UUID().uuidString) throws -> URL {
        let url = sandbox.appendingPathComponent(name).appendingPathExtension("m4a")
        try data.write(to: url)
        return url
    }

    func cleanUp() {
        try? FileManager.default.removeItem(at: sandbox)
    }
}

@Test("save, list, and load round-trip encrypted audio and ordered metadata")
func roundTripAndNewestFirstOrdering() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let keyProvider = FixedKeyProvider()
    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: keyProvider
    )
    let oldBytes = Data("older recording bytes".utf8)
    let newBytes = Data("newer recording bytes".utf8)
    let oldDate = Date(timeIntervalSince1970: 1_700_000_000)
    let newDate = oldDate.addingTimeInterval(90)

    let old = try store.save(
        audioFileURL: fixture.makeAudio(oldBytes, named: "old"),
        title: "Old title",
        note: "Old note",
        createdAt: oldDate,
        duration: 12.5
    )
    let new = try store.save(
        audioFileURL: fixture.makeAudio(newBytes, named: "new"),
        title: "",
        note: "New note",
        createdAt: newDate,
        duration: 4
    )

    #expect(try store.list() == [new, old])
    #expect(old.title == "Old title")
    #expect(old.note == "Old note")
    #expect(old.createdAt == oldDate)
    #expect(old.duration == 12.5)
    #expect(old.source == "Microphone")
    #expect(new.title == "")
    #expect(try store.loadAudioData(for: old.id) == oldBytes)
    #expect(try store.loadAudioData(for: new.id) == newBytes)
}

@Test("title and note updates persist across store instances")
func metadataUpdatesPersistAcrossReinitialization() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let keyProvider = FixedKeyProvider()
    let firstStore = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: keyProvider
    )
    let session = try firstStore.save(
        audioFileURL: fixture.makeAudio(Data("audio".utf8)),
        title: "Before",
        note: "Initial note",
        createdAt: Date(timeIntervalSince1970: 1_700_000_000),
        duration: 1
    )

    let editingStore = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: keyProvider
    )
    try editingStore.updateTitle("After", for: session.id)
    try editingStore.updateNote("Persisted note", for: session.id)

    let reloadedStore = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: keyProvider
    )
    let reloaded = try #require(try reloadedStore.list().first)
    #expect(reloaded.title == "After")
    #expect(reloaded.note == "Persisted note")
}

@Test("delete removes the session and subsequent access reports sessionNotFound")
func deleteRemovesSessionDirectory() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: FixedKeyProvider()
    )
    let session = try store.save(
        audioFileURL: fixture.makeAudio(Data("audio".utf8)),
        title: "",
        note: "",
        createdAt: Date(),
        duration: 1
    )
    let sessionDirectory = fixture.root.appendingPathComponent(
        session.id.uuidString,
        isDirectory: true
    )
    #expect(FileManager.default.fileExists(atPath: sessionDirectory.path))

    try store.delete(session.id)

    #expect(!FileManager.default.fileExists(atPath: sessionDirectory.path))
    #expect(throws: SecureStoreError.sessionNotFound) {
        try store.loadAudioData(for: session.id)
    }
    #expect(throws: SecureStoreError.sessionNotFound) {
        try store.delete(session.id)
    }
}

@Test("corrupt encrypted audio reports corruptOrUndecryptable")
func corruptCiphertextIsReported() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: FixedKeyProvider()
    )
    let session = try store.save(
        audioFileURL: fixture.makeAudio(Data("audio to corrupt".utf8)),
        title: "",
        note: "",
        createdAt: Date(),
        duration: 1
    )
    let encryptedAudio = fixture.root
        .appendingPathComponent(session.id.uuidString, isDirectory: true)
        .appendingPathComponent("audio.m4a.enc")
    var corruptBytes = try Data(contentsOf: encryptedAudio)
    corruptBytes[corruptBytes.startIndex] ^= 0xFF
    try corruptBytes.write(to: encryptedAudio)

    #expect(throws: SecureStoreError.corruptOrUndecryptable) {
        try store.loadAudioData(for: session.id)
    }
}

@Test("key failure preserves plaintext and creates no session files")
func keyFailurePreservesRecoveryAudio() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: FailingKeyProvider()
    )
    let plaintext = try fixture.makeAudio(Data("recovery audio".utf8))

    #expect(throws: SecureStoreError.keyUnavailable("injected key failure")) {
        try store.save(
            audioFileURL: plaintext,
            title: "",
            note: "",
            createdAt: Date(),
            duration: 1
        )
    }
    #expect(FileManager.default.fileExists(atPath: plaintext.path))
    #expect(try FileManager.default.contentsOfDirectory(atPath: fixture.root.path).isEmpty)
}

@Test("successful save leaves no plaintext audio copy under the store root")
func successfulSaveLeavesNoPlaintextCopy() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let original = Data("a unique plaintext payload 71B129D0".utf8)
    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: FixedKeyProvider()
    )
    let plaintext = try fixture.makeAudio(original)
    _ = try store.save(
        audioFileURL: plaintext,
        title: "",
        note: "",
        createdAt: Date(),
        duration: 1
    )

    #expect(!FileManager.default.fileExists(atPath: plaintext.path))
    let enumerator = try #require(
        FileManager.default.enumerator(
            at: fixture.root,
            includingPropertiesForKeys: [.isRegularFileKey]
        )
    )
    for case let storedURL as URL in enumerator {
        let values = try storedURL.resourceValues(forKeys: [.isRegularFileKey])
        if values.isRegularFile == true {
            #expect(try Data(contentsOf: storedURL) != original)
        }
    }
}

@Test("disk failure leaves no partial session and preserves recovery audio")
func diskFailureIsAtomic() throws {
    let fixture = try StoreFixture()
    defer { fixture.cleanUp() }

    let store = try SecureSessionStore(
        rootDirectory: fixture.root,
        keyProvider: FixedKeyProvider()
    )
    try FileManager.default.removeItem(at: fixture.root)
    try Data("root path is now a file".utf8).write(to: fixture.root)
    let plaintext = try fixture.makeAudio(Data("recoverable audio".utf8))

    #expect(throws: SecureStoreError.self) {
        try store.save(
            audioFileURL: plaintext,
            title: "",
            note: "",
            createdAt: Date(),
            duration: 1
        )
    }
    #expect(FileManager.default.fileExists(atPath: plaintext.path))
    #expect(try Data(contentsOf: fixture.root) == Data("root path is now a file".utf8))
}
