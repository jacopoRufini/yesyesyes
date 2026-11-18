import Foundation

/// "When a banner mentions `app`, press its `action` button." Both are always required.
public struct Rule: Codable, Equatable, Hashable, Identifiable {
    public var app: String
    public var action: String

    public var id: String { app.lowercased() + "|" + action.lowercased() }
    public var isValid: Bool { !app.isEmpty && !action.isEmpty }

    public init(app: String, action: String) {
        self.app = app.trimmingCharacters(in: .whitespacesAndNewlines)
        self.action = action.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

public struct Config: Codable, Equatable {
    public var enabled: Bool
    public var rules: [Rule]

    public init(enabled: Bool = true, rules: [Rule] = []) {
        self.enabled = enabled
        self.rules = rules
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        rules = try c.decodeIfPresent([Rule].self, forKey: .rules) ?? []
    }

    public enum AddResult: Equatable { case added, invalid, duplicate }

    @discardableResult
    public mutating func add(_ rule: Rule) -> AddResult {
        guard rule.isValid else { return .invalid }
        guard !rules.contains(where: { $0.id == rule.id }) else { return .duplicate }
        rules.append(rule)
        return .added
    }
}

public final class ConfigStore {
    public let url: URL

    public static var defaultURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("yesyesyes/config.json")
    }

    public init(url: URL) { self.url = url }

    /// A missing or unreadable file gives the default (active, no rules).
    public func load() -> Config {
        guard let data = try? Data(contentsOf: url),
              let config = try? JSONDecoder().decode(Config.self, from: data) else { return Config() }
        return config
    }

    public func save(_ config: Config) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(config).write(to: url, options: .atomic)
    }
}
