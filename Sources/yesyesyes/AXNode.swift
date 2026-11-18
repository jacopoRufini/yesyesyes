import ApplicationServices
import Cocoa
import YesYesYesCore

struct AXNode: UINode {
    let element: AXUIElement

    private func attr(_ name: String) -> AnyObject? {
        var value: CFTypeRef?
        return AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success ? value : nil
    }

    var role: String? { attr("AXRole") as? String }
    var labels: [String] { ["AXTitle", "AXDescription", "AXValue"].compactMap { attr($0) as? String } }
    var children: [UINode] { ((attr("AXChildren") as? [AXUIElement]) ?? []).map(AXNode.init) }

    var actionNames: [String] {
        var names: CFArray?
        guard AXUIElementCopyActionNames(element, &names) == .success else { return [] }
        return (names as? [String]) ?? []
    }

    func perform(_ action: String) -> Bool {
        AXUIElementPerformAction(element, action as CFString) == .success
    }

    static func notificationCenter() -> AXNode? {
        NSRunningApplication.runningApplications(withBundleIdentifier: "com.apple.notificationcenterui").first
            .map { AXNode(element: AXUIElementCreateApplication($0.processIdentifier)) }
    }
}
