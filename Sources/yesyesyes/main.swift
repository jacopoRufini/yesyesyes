import ApplicationServices
import Cocoa
import Combine
import ServiceManagement
import SwiftUI

final class AppDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    private let model = AppModel()
    private let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
    private var window: NSWindow?
    private var timer: Timer?
    private var bag = Set<AnyCancellable>()

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        let menu = NSMenu()
        menu.delegate = self
        item.menu = menu
        model.$config.combineLatest(model.$trusted).sink { [weak self] config, trusted in
            self?.updateIcon(active: config.enabled && trusted)
        }.store(in: &bag)
        timer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [model] _ in model.tick() }
    }

    private func updateIcon(active: Bool) {
        let name = active ? "checkmark.circle.fill" : "pause.circle"
        item.button?.image = NSImage(systemSymbolName: name, accessibilityDescription: "yesyesyes")
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        let count = model.config.rules.count
        let status = !model.trusted ? "Needs Accessibility permission"
            : model.config.enabled ? "Active · \(count) rule\(count == 1 ? "" : "s")" : "Paused"
        let header = menu.addItem(withTitle: status, action: nil, keyEquivalent: "")
        header.isEnabled = false
        if !model.trusted {
            menu.addItem(withTitle: "Open Accessibility Settings…", action: #selector(openAccessibility), keyEquivalent: "").target = self
        }
        menu.addItem(.separator())
        menu.addItem(withTitle: model.config.enabled ? "Pause" : "Resume", action: #selector(toggle), keyEquivalent: "p").target = self
        menu.addItem(withTitle: "Rules & Log…", action: #selector(showWindow), keyEquivalent: ",").target = self
        let login = menu.addItem(withTitle: "Launch at Login", action: #selector(toggleLogin), keyEquivalent: "")
        login.target = self
        login.state = SMAppService.mainApp.status == .enabled ? .on : .off
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit yesyesyes", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
    }

    @objc private func toggle() { model.config.enabled.toggle() }

    @objc private func openAccessibility() {
        NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!)
    }

    @objc private func toggleLogin() {
        do {
            if SMAppService.mainApp.status == .enabled { try SMAppService.mainApp.unregister() }
            else { try SMAppService.mainApp.register() }
        } catch {
            let alert = NSAlert()
            alert.messageText = "Could not change Launch at Login"
            alert.informativeText = error.localizedDescription
            alert.runModal()
        }
    }

    @objc private func showWindow() {
        let view = NSHostingController(rootView: RulesView(model: model))
        if let window {
            window.contentViewController = view
        } else {
            let w = NSWindow(contentViewController: view)
            w.title = "yesyesyes"
            w.styleMask = [.titled, .closable, .resizable]
            w.isReleasedWhenClosed = false
            w.center()
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

let app = NSApplication.shared
let delegate = AppDelegate()
app.delegate = delegate
app.setActivationPolicy(.accessory)
app.run()
