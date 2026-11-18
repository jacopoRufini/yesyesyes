import Foundation

/// A node of the accessibility tree, abstracted so the matching can be tested.
public protocol UINode {
    var role: String? { get }
    var labels: [String] { get }
    var children: [UINode] { get }
    var actionNames: [String] { get }
    func perform(_ action: String) -> Bool
}

public enum Outcome: Equatable { case pressed, failed, notFound }

public enum Matcher {
    public static func texts(_ node: UINode) -> [String] {
        node.labels + node.children.flatMap(texts)
    }

    private static func findButton(_ node: UINode, label: String) -> UINode? {
        if node.role == "AXButton", node.labels.contains(where: { $0.lowercased() == label }) { return node }
        for child in node.children {
            if let found = findButton(child, label: label) { return found }
        }
        return nil
    }

    /// Presses `rule.action` in the smallest element whose text also mentions `rule.app`.
    public static func press(_ node: UINode, rule: Rule) -> Outcome {
        for child in node.children {
            let outcome = press(child, rule: rule)
            if outcome != .notFound { return outcome }
        }
        guard texts(node).contains(where: { $0.localizedCaseInsensitiveContains(rule.app) }) else { return .notFound }
        let want = rule.action.lowercased()
        if let button = findButton(node, label: want) {
            return button.perform("AXPress") ? .pressed : .failed
        }
        for name in node.actionNames where name.hasPrefix("Name:") {
            let label = name.dropFirst(5).split(separator: "\n").first.map(String.init) ?? ""
            if label.lowercased() == want { return node.perform(name) ? .pressed : .failed }
        }
        return .notFound
    }
}
