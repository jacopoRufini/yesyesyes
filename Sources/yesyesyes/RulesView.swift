import SwiftUI
import YesYesYesCore

struct RulesView: View {
    @ObservedObject var model: AppModel
    @State private var app = ""
    @State private var action = ""
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Toggle("Active", isOn: $model.config.enabled)
            Text("Rules").font(.headline)
            Text("When a notification mentions the app, press the button with that exact label.")
                .font(.caption).foregroundStyle(.secondary)
            List {
                ForEach(model.config.rules) { rule in
                    HStack {
                        Text(rule.app).bold()
                        Text("→ \(rule.action)")
                        Spacer()
                        Button("Remove") { model.config.rules.removeAll { $0.id == rule.id } }
                    }
                }
            }
            .frame(minHeight: 110)
            HStack {
                TextField("App name, e.g. Claude", text: $app)
                TextField("Button label, e.g. Allow once", text: $action)
                Button("Add", action: add)
            }
            if let error { Text(error).foregroundStyle(.red).font(.caption) }
            Divider()
            Text("Recent clicks").font(.headline)
            List(model.events) { event in
                HStack {
                    Text(event.date, style: .time).monospacedDigit()
                    Text("\(event.app) → \(event.action)")
                    Spacer()
                    if !event.succeeded { Text("failed").foregroundStyle(.red) }
                }
            }
            .frame(minHeight: 110)
        }
        .padding()
        .frame(minWidth: 520, minHeight: 480)
    }

    private func add() {
        var updated = model.config
        switch updated.add(Rule(app: app, action: action)) {
        case .added:
            model.config = updated
            app = ""
            action = ""
            error = nil
        case .invalid: error = "App name and button label are both required."
        case .duplicate: error = "That rule already exists."
        }
    }
}
