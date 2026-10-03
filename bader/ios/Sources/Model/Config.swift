import Foundation
import Security

/// The keys for the user's Google account, as handed over by the computer.
struct GoogleKeys: Codable, Equatable {
    var id: String
    var s: String
    var r: String
    var a: String?
}

/// What the pairing code holds. Short names keep the QR code easy to scan.
struct Pairing: Codable, Equatable {
    var bader: Int
    var ai: String
    var l1: String?
    var l2: String?
    var g: GoogleKeys?

    static func parse(_ text: String) -> Pairing? {
        guard let data = text.data(using: .utf8),
              let p = try? JSONDecoder().decode(Pairing.self, from: data),
              p.bader >= 1, !p.ai.isEmpty else { return nil }
        return p
    }
}

/// The pairing is kept in the iPhone's Keychain: locked with the phone, never in a backup of another device.
enum Vault {
    private static let service = "com.malikqasem.bader.pairing"
    private static let account = "pairing"

    private static var base: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    static func load() -> Pairing? {
        var query = base
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &out) == errSecSuccess,
              let data = out as? Data else { return nil }
        return try? JSONDecoder().decode(Pairing.self, from: data)
    }

    @discardableResult
    static func save(_ pairing: Pairing) -> Bool {
        guard let data = try? JSONEncoder().encode(pairing) else { return false }
        SecItemDelete(base as CFDictionary)
        var item = base
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(item as CFDictionary, nil) == errSecSuccess
    }

    static func clear() {
        SecItemDelete(base as CFDictionary)
    }
}

/// Small settings that are not secret.
enum Prefs {
    private static let d = UserDefaults.standard
    static var speak: Bool {
        get { d.object(forKey: "speak") as? Bool ?? true }
        set { d.set(newValue, forKey: "speak") }
    }
    static var display: Bool {
        get { d.object(forKey: "display") as? Bool ?? true }
        set { d.set(newValue, forKey: "display") }
    }
    static var model: String {
        get { d.string(forKey: "model") ?? "anthropic/claude-haiku-4.5" }
        set { d.set(newValue, forKey: "model") }
    }
}

struct BaderError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}
