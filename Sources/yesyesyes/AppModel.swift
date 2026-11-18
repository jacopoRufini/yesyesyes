import ApplicationServices
import Foundation
import YesYesYesCore

final class AppModel: ObservableObject {
    private let store = ConfigStore(url: ConfigStore.defaultURL)
    let engine: Engine

    @Published var config: Config {
        didSet {
            engine.config = config
            try? store.save(config)
        }
    }
    @Published var events: [ClickEvent] = []
    @Published var trusted = AXIsProcessTrusted()

    init() {
        let loaded = store.load()
        config = loaded
        engine = Engine(config: loaded)
        engine.onEvent = { [weak self] event in
            self?.events.insert(event, at: 0)
            if let self, self.events.count > 50 { self.events.removeLast() }
        }
    }

    func tick() {
        trusted = AXIsProcessTrusted()
        guard trusted else { return }
        engine.tick(root: AXNode.notificationCenter())
    }
}
