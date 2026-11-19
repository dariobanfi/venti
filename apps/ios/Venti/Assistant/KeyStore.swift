import Foundation
import Security

/// The pilot's key for their model provider, kept in the iPhone's Keychain.
///
/// It is read when a question is asked and handed to the core for that one
/// request. It is not synchronised to other devices, never written to the
/// app's files, and never sent anywhere but to the provider.
enum KeyStore {
    /// What identifies the key in the Keychain. Computed, because a
    /// dictionary of `Any` cannot be shared between threads.
    private static var query: [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "fly.venti.assistant",
            kSecAttrAccount as String: "anthropic",
        ]
    }

    static func read() -> String? {
        var search = Self.query
        search[kSecReturnData as String] = true
        search[kSecMatchLimit as String] = kSecMatchLimitOne
        var found: CFTypeRef?
        guard SecItemCopyMatching(search as CFDictionary, &found) == errSecSuccess,
              let data = found as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    /// Stores the key, replacing any earlier one. Returns whether it worked.
    @discardableResult
    static func save(_ key: String) -> Bool {
        remove()
        var item = Self.query
        item[kSecValueData as String] = Data(key.utf8)
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func remove() {
        SecItemDelete(Self.query as CFDictionary)
    }
}
