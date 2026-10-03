import SwiftUI

@main
struct BaderApp: App {
    @StateObject private var app = AppState()
    @Environment(\.scenePhase) private var phase

    var body: some Scene {
        WindowGroup {
            Group {
                if app.pairing == nil {
                    PairView()
                } else {
                    ChatView(voice: app.voice)
                }
            }
            .environmentObject(app)
            .preferredColorScheme(.dark)
            .onChange(of: phase) {
                if phase == .active, app.pairing != nil { app.refresh() }
            }
        }
    }
}
