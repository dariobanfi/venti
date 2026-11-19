import Foundation
import Security

/// Secrets in the iPhone's Keychain: kept on this device only, not
/// synchronised to others, never written to the app's files.
enum Keychain {
    /// What identifies an item. Computed, because a dictionary of `Any`
    /// cannot be shared between threads.
    private static func query(service: String, account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }

    static func read(service: String, account: String) -> String? {
        var search = query(service: service, account: account)
        search[kSecReturnData as String] = true
        search[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(search as CFDictionary, &found) == errSecSuccess,
              let data = found as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Stores a value, replacing any earlier one. Returns whether it worked.
    @discardableResult
    static func save(_ value: String, service: String, account: String) -> Bool {
        remove(service: service, account: account)
        var item = query(service: service, account: account)
        item[kSecValueData as String] = Data(value.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func remove(service: String, account: String) {
        SecItemDelete(query(service: service, account: account) as CFDictionary)
    }
}

/// The pilot's key for their model provider.
///
/// It is read when a question is asked and handed to the core for that one
/// request, and is never sent anywhere but to the provider.
enum KeyStore {
    private static let service = "fly.venti.assistant"
    private static let account = "anthropic"

    static func read() -> String? {
        Keychain.read(service: service, account: account)
    }

    @discardableResult
    static func save(_ key: String) -> Bool {
        Keychain.save(key, service: service, account: account)
    }

    static func remove() {
        Keychain.remove(service: service, account: account)
    }
}

/// The token of the session with the pilot's Venti server. It is sent only
/// to that server, with the requests that act for the account.
enum SessionStore {
    private static let service = "fly.venti.session"
    private static let account = "token"

    static func read() -> String? {
        Keychain.read(service: service, account: account)
    }

    @discardableResult
    static func save(_ token: String) -> Bool {
        Keychain.save(token, service: service, account: account)
    }

    static func remove() {
        Keychain.remove(service: service, account: account)
    }
}
