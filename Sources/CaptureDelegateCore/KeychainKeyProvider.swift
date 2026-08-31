import CryptoKit
import Foundation
import Security

public enum SecureStoreError: Error, Equatable {
    case keyUnavailable(String)
    case ioFailure(String)
    case corruptOrUndecryptable
    case sessionNotFound
}

public protocol SymmetricKeyProviding: Sendable {
    func key() throws -> SymmetricKey
}

public final class KeychainKeyProvider: SymmetricKeyProviding {
    private enum KeychainReadResult {
        case success(Data)
        case failure(OSStatus)
    }

    private let service: String
    private let account: String

    public init(
        service: String = "CaptureDelegate.SecureStore",
        account: String = "session-master-key"
    ) {
        self.service = service
        self.account = account
    }

    public func key() throws -> SymmetricKey {
        switch readKeyData() {
        case .success(let data):
            return try symmetricKey(from: data)
        case .failure(errSecItemNotFound):
            return try createKey()
        case .failure(let status):
            throw keychainError(operation: "read", status: status)
        }
    }

    private func createKey() throws -> SymmetricKey {
        let newKey = SymmetricKey(size: .bits256)
        let keyData = newKey.withUnsafeBytes { Data($0) }
        let addQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
            kSecValueData as String: keyData,
        ]
        let status = SecItemAdd(addQuery as CFDictionary, nil)

        if status == errSecSuccess {
            return newKey
        }
        if status == errSecDuplicateItem {
            switch readKeyData() {
            case .success(let existingData):
                return try symmetricKey(from: existingData)
            case .failure(let readStatus):
                throw keychainError(operation: "re-read after duplicate", status: readStatus)
            }
        }
        throw keychainError(operation: "create", status: status)
    }

    private func readKeyData() -> KeychainReadResult {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        guard status == errSecSuccess else {
            return .failure(status)
        }
        guard let data = item as? Data else {
            return .failure(errSecDecode)
        }
        return .success(data)
    }

    private func symmetricKey(from data: Data) throws -> SymmetricKey {
        guard data.count == 32 else {
            throw SecureStoreError.keyUnavailable(
                "Keychain read returned \(data.count) bytes instead of 32 (OSStatus \(errSecDecode))"
            )
        }
        return SymmetricKey(data: data)
    }

    private func keychainError(operation: String, status: OSStatus) -> SecureStoreError {
        let description = SecCopyErrorMessageString(status, nil) as String? ?? "unknown error"
        return .keyUnavailable(
            "Keychain \(operation) failed with OSStatus \(status): \(description)"
        )
    }
}
