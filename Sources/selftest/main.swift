import Foundation

var failures = 0

func check(_ condition: @autoclosure () -> Bool, line: Int = #line) {
    if !condition() {
        failures += 1
        print("  FAILED at line \(line)")
    }
}

func run(_ name: String, _ body: () throws -> Void) {
    print(name)
    do { try body() } catch { failures += 1; print("  ERROR \(error)") }
}

import YesYesYesCore

final class FakeNode: UINode {
    var role: String?
    var labels: [String]
    var children: [UINode]
    var actionNames: [String]
    var performed: [String] = []
    var succeeds = true

    init(role: String? = nil, labels: [String] = [], actions: [String] = [], children: [UINode] = []) {
        self.role = role
        self.labels = labels
        self.actionNames = actions
        self.children = children
    }

    func perform(_ action: String) -> Bool {
        performed.append(action)
        return succeeds
    }
}

func banner(app: String = "Claude", action: String = "Allow once") -> FakeNode {
    FakeNode(labels: ["\(app), run it?"], actions: ["Name:Dismiss\nTarget:0x0", "Name:\(action)\nTarget:0x0"],
             children: [FakeNode(role: "AXStaticText", labels: [app])])
}

struct MatcherTests {
    func pressesCustomActionOfMatchingBanner() {
        let b = banner()
        let root = FakeNode(children: [b])
        check(Matcher.press(root, rule: Rule(app: "claude", action: "allow once")) == .pressed)
        check(b.performed == ["Name:Allow once\nTarget:0x0"])
    }

    func pressesButtonByLabel() {
        let button = FakeNode(role: "AXButton", labels: ["Merge"])
        let root = FakeNode(labels: ["contact-less"], children: [button])
        check(Matcher.press(root, rule: Rule(app: "contact-less", action: "Merge")) == .pressed)
        check(button.performed == ["AXPress"])
    }

    func otherAppIsLeftAlone() {
        let b = banner(app: "Slack")
        check(Matcher.press(FakeNode(children: [b]), rule: Rule(app: "Claude", action: "Allow once")) == .notFound)
        check(b.performed.isEmpty)
    }

    func otherActionIsLeftAlone() {
        let b = banner()
        check(Matcher.press(FakeNode(children: [b]), rule: Rule(app: "Claude", action: "Merge")) == .notFound)
        check(b.performed.isEmpty)
    }

    func failedPress() {
        let b = banner()
        b.succeeds = false
        check(Matcher.press(b, rule: Rule(app: "Claude", action: "Allow once")) == .failed)
    }
}

struct EngineTests {
    func pausedDoesNothing() {
        let b = banner()
        let engine = Engine(config: Config(enabled: false, rules: [Rule(app: "Claude", action: "Allow once")]))
        engine.tick(root: b)
        check(b.performed.isEmpty)
    }

    func cooldownAndEvent() {
        var time = Date(timeIntervalSince1970: 0)
        let b = banner()
        let engine = Engine(config: Config(rules: [Rule(app: "Claude", action: "Allow once")]), now: { time })
        var events: [ClickEvent] = []
        engine.onEvent = { events.append($0) }
        engine.tick(root: b)
        engine.tick(root: b)
        check(b.performed.count == 1)
        time = time.addingTimeInterval(4)
        engine.tick(root: b)
        check(b.performed.count == 2)
        check(events.map(\.succeeded) == [true, true])
        check(events.first?.app == "Claude")
    }

    func invalidRuleNeverMatchesEverything() {
        let b = banner()
        let engine = Engine(config: Config(rules: [Rule(app: "", action: "Allow once"), Rule(app: "Claude", action: "")]))
        engine.tick(root: b)
        check(b.performed.isEmpty)
    }
}

struct ConfigTests {
    func addRejectsInvalidAndDuplicate() {
        var c = Config()
        check(c.add(Rule(app: " ", action: "x")) == .invalid)
        check(c.add(Rule(app: "Claude", action: "Allow once")) == .added)
        check(c.add(Rule(app: "claude", action: "allow once")) == .duplicate)
        check(c.rules.count == 1)
    }

    func storeRoundTripAndMissingFile() throws {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("yyy-\(UUID().uuidString)/config.json")
        let store = ConfigStore(url: url)
        check(store.load() == Config())
        let c = Config(enabled: false, rules: [Rule(app: "Claude", action: "Allow once")])
        try store.save(c)
        check(store.load() == c)
    }

    func missingEnabledDefaultsToActive() throws {
        let c = try JSONDecoder().decode(Config.self, from: Data(#"{"rules":[]}"#.utf8))
        check(c.enabled)
    }
}


run("MatcherTests.pressesCustomActionOfMatchingBanner") { try MatcherTests().pressesCustomActionOfMatchingBanner() }
run("MatcherTests.pressesButtonByLabel") { try MatcherTests().pressesButtonByLabel() }
run("MatcherTests.otherAppIsLeftAlone") { try MatcherTests().otherAppIsLeftAlone() }
run("MatcherTests.otherActionIsLeftAlone") { try MatcherTests().otherActionIsLeftAlone() }
run("MatcherTests.failedPress") { try MatcherTests().failedPress() }
run("EngineTests.pausedDoesNothing") { try EngineTests().pausedDoesNothing() }
run("EngineTests.cooldownAndEvent") { try EngineTests().cooldownAndEvent() }
run("EngineTests.invalidRuleNeverMatchesEverything") { try EngineTests().invalidRuleNeverMatchesEverything() }
run("ConfigTests.addRejectsInvalidAndDuplicate") { try ConfigTests().addRejectsInvalidAndDuplicate() }
run("ConfigTests.storeRoundTripAndMissingFile") { try ConfigTests().storeRoundTripAndMissingFile() }
run("ConfigTests.missingEnabledDefaultsToActive") { try ConfigTests().missingEnabledDefaultsToActive() }

print(failures == 0 ? "All passed" : "\(failures) failure(s)")
exit(failures == 0 ? 0 : 1)
