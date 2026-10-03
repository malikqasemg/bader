import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var app: AppState
    @Environment(\.dismiss) private var dismiss
    @State private var confirmUnpair = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Voice") {
                    Toggle("Read answers aloud", isOn: $app.speak)
                }
                DisplaySection(link: app.link, on: $app.display)
                MemorySection(memory: app.memory) { app.refresh() }
                Section("Account") {
                    row("Google", app.pairing?.g?.a ?? (app.pairing?.g == nil ? "not connected" : "connected"))
                    row("Languages", app.languages.map { $0 == "ar" ? "العربية" : "English" }.joined(separator: " + "))
                    Button("Unpair this iPhone", role: .destructive) { confirmUnpair = true }
                }
            }
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .confirmationDialog("Remove the keys and memory copy from this iPhone?", isPresented: $confirmUnpair, titleVisibility: .visible) {
                Button("Unpair", role: .destructive) {
                    app.unpair()
                    dismiss()
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private func row(_ name: String, _ value: String) -> some View {
        HStack {
            Text(name)
            Spacer()
            Text(value).foregroundStyle(.secondary)
        }
    }
}

private struct DisplaySection: View {
    @ObservedObject var link: FaceLink
    @Binding var on: Bool

    var body: some View {
        Section {
            Toggle("Use Bader's display", isOn: $on)
            if on {
                LabeledContent("Status", value: {
                    switch link.status {
                    case .off: return "off"
                    case .searching: return "looking for it…"
                    case .connected: return "connected"
                    case .away: return "the computer has it"
                    }
                }())
            }
        } header: {
            Text("Display")
        } footer: {
            Text("The small screen connects over Bluetooth. Tap PC / PHONE on its top bar to hand it to this iPhone or back to the computer.")
        }
    }
}

private struct MemorySection: View {
    @ObservedObject var memory: Memory
    let sync: () -> Void

    var body: some View {
        Section {
            LabeledContent("Entries", value: "\(memory.entries.count)")
            LabeledContent("Last sync", value: memory.lastSync.map { $0.formatted(date: .omitted, time: .shortened) } ?? "not yet")
            if let problem = memory.syncError {
                Text(problem).font(.footnote).foregroundStyle(.orange)
            }
            Button("Sync now", action: sync)
        } header: {
            Text("Shared memory")
        } footer: {
            Text("What you ask here and on the computer is kept in one file in your own Google Drive, so both know the same history.")
        }
    }
}
