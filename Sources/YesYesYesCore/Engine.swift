import Foundation

/// What was clicked, never the notification text.
public struct ClickEvent: Identifiable, Equatable {
    public let id = UUID()
    public let date: Date
    public let app: String
    public let action: String
    public let succeeded: Bool
}

public final class Engine {
    public var config: Config
    public var onEvent: ((ClickEvent) -> Void)?
    private var last: [String: Date] = [:]
    private let cooldown: TimeInterval
    private let now: () -> Date

    public init(config: Config, cooldown: TimeInterval = 3, now: @escaping () -> Date = Date.init) {
        self.config = config
        self.cooldown = cooldown
        self.now = now
    }

    /// One look at the notification center. Does nothing when paused or without a root.
    public func tick(root: UINode?) {
        guard config.enabled, let root else { return }
        for rule in config.rules where rule.isValid {
            if let t = last[rule.id], now().timeIntervalSince(t) < cooldown { continue }
            let outcome = Matcher.press(root, rule: rule)
            guard outcome != .notFound else { continue }
            last[rule.id] = now()
            onEvent?(ClickEvent(date: now(), app: rule.app, action: rule.action, succeeded: outcome == .pressed))
        }
    }
}
