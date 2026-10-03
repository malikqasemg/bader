import SwiftUI
import UIKit

struct ChatView: View {
    @EnvironmentObject var app: AppState
    @ObservedObject var voice: Voice
    @State private var draft = ""
    @State private var showSettings = false
    @State private var showCamera = false
    @FocusState private var typing: Bool

    private let quick: [(String, String, String)] = [
        ("Brief", "sun.max", "Give me my brief: today's meetings, the important unread mail, and anything I should act on."),
        ("Mail", "envelope", "What are the important mails from today?"),
        ("Meetings", "calendar", "What meetings do I have today and tomorrow?"),
        ("News", "newspaper", "What are the top news headlines right now?"),
    ]

    var body: some View {
        VStack(spacing: 0) {
            header
            messages
            if let approval = app.approval { approvalCard(approval) }
            controls
        }
        .background(Theme.back.ignoresSafeArea())
        .sheet(isPresented: $showSettings) { SettingsView().environmentObject(app) }
        .fullScreenCover(isPresented: $showCamera) {
            CameraPicker { image in
                showCamera = false
                if let image { app.send(draft, image: image); draft = "" }
            }
            .ignoresSafeArea()
        }
    }

    // ── Top: Bader's face and what he is doing ───────────────────────────────

    private var header: some View {
        HStack(spacing: 12) {
            Theme.face(app.face).resizable().scaledToFit().frame(width: 76, height: 76)
                .scaleEffect(voice.recording ? 1 + voice.level * 0.12 : 1)
                .animation(.easeOut(duration: 0.12), value: voice.level)
                .onTapGesture { app.toggleTalk() }
            VStack(alignment: .leading, spacing: 3) {
                Text("Bader · بدر").font(.headline)
                Text(app.status.isEmpty ? idleLine : app.status)
                    .font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
            }
            Spacer()
            Button { showSettings = true } label: {
                Image(systemName: "gearshape").font(.title3).padding(8)
            }
            .tint(.secondary)
        }
        .padding(.horizontal, 16).padding(.vertical, 8)
        .background(Theme.card)
    }

    private var idleLine: String {
        if voice.speaking { return "Speaking… tap the mic to stop" }
        if let unread = app.unread { return unread == 0 ? "No unread mail" : "\(unread) unread mail" }
        return "Ready"
    }

    // ── Middle: the conversation ─────────────────────────────────────────────

    private var messages: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 10) {
                    if app.messages.isEmpty {
                        Text("Ask me anything, by voice or by typing.\nاسألني أي شيء، بالصوت أو بالكتابة.")
                            .multilineTextAlignment(.center).foregroundStyle(.secondary).padding(.top, 60)
                    }
                    ForEach(app.messages) { bubble($0).id($0.id) }
                    if app.busy {
                        HStack { ProgressView().tint(.secondary); Spacer() }.padding(.horizontal, 6).id("busy")
                    }
                }
                .padding(14)
            }
            .scrollDismissesKeyboard(.interactively)
            .refreshable { app.refresh() }
            .onChange(of: app.messages.count) {
                withAnimation { proxy.scrollTo(app.messages.last?.id, anchor: .bottom) }
            }
            .onChange(of: app.busy) {
                if app.busy { withAnimation { proxy.scrollTo("busy", anchor: .bottom) } }
            }
        }
    }

    private func bubble(_ m: ChatMessage) -> some View {
        let mine = m.role == .user
        return HStack {
            if mine { Spacer(minLength: 40) }
            VStack(alignment: .leading, spacing: 6) {
                if let image = m.image {
                    Image(uiImage: image).resizable().scaledToFit().frame(maxHeight: 180)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                }
                Text(m.text)
                    .multilineTextAlignment(Voice.isArabic(m.text) ? .trailing : .leading)
                    .frame(maxWidth: mine ? nil : .infinity, alignment: Voice.isArabic(m.text) ? .trailing : .leading)
                    .textSelection(.enabled)
            }
            .padding(.horizontal, 13).padding(.vertical, 9)
            .background(mine ? Theme.mine : (m.role == .problem ? Color.orange.opacity(0.22) : Theme.card),
                        in: RoundedRectangle(cornerRadius: 16))
            .contextMenu {
                Button("Copy") { UIPasteboard.general.string = m.text }
                if m.role == .bader { Button("Read aloud") { voice.speak(m.text) } }
            }
            if !mine { Spacer(minLength: 24) }
        }
    }

    // ── Approval: Bader wants to do something that cannot be undone ──────────

    private func approvalCard(_ a: Approval) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(a.title, systemImage: "hand.raised.fill").font(.headline)
            ScrollView { Text(a.detail).font(.subheadline).frame(maxWidth: .infinity, alignment: .leading) }
                .frame(maxHeight: 170)
            HStack(spacing: 12) {
                Button { app.answer(approval: false) } label: { Text("No").frame(maxWidth: .infinity).padding(.vertical, 6) }
                    .buttonStyle(.bordered)
                Button { app.answer(approval: true) } label: { Text("Yes, do it").frame(maxWidth: .infinity).padding(.vertical, 6) }
                    .buttonStyle(.borderedProminent).tint(.green)
            }
        }
        .padding(14)
        .background(Color.yellow.opacity(0.16), in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.yellow.opacity(0.6)))
        .padding(.horizontal, 14).padding(.bottom, 8)
    }

    // ── Bottom: quick buttons, typing, the mic ───────────────────────────────

    private var controls: some View {
        VStack(spacing: 10) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 8) {
                    ForEach(quick, id: \.0) { item in
                        Button { app.send(item.2) } label: { Label(item.0, systemImage: item.1).font(.subheadline) }
                            .buttonStyle(.bordered).buttonBorderShape(.capsule).disabled(app.busy)
                    }
                }
                .padding(.horizontal, 14)
            }
            HStack(spacing: 10) {
                Button { showCamera = true } label: { Image(systemName: "camera").font(.title3) }
                    .disabled(app.busy || !UIImagePickerController.isSourceTypeAvailable(.camera))
                TextField("Type to Bader…", text: $draft, axis: .vertical)
                    .lineLimit(1...4).focused($typing)
                    .padding(.horizontal, 12).padding(.vertical, 9)
                    .background(Theme.card, in: RoundedRectangle(cornerRadius: 18))
                    .onSubmit(sendDraft)
                if app.busy {
                    Button { app.cancel() } label: { Image(systemName: "stop.circle.fill").font(.system(size: 34)) }.tint(.red)
                } else if !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    Button(action: sendDraft) { Image(systemName: "arrow.up.circle.fill").font(.system(size: 34)) }
                } else {
                    Button { typing = false; app.toggleTalk() } label: {
                        Image(systemName: voice.recording ? "waveform.circle.fill" : (voice.speaking ? "speaker.slash.circle.fill" : "mic.circle.fill"))
                            .font(.system(size: 40))
                            .symbolEffect(.pulse, isActive: voice.recording)
                    }
                    .tint(voice.recording ? .red : Theme.accent)
                }
            }
            .padding(.horizontal, 14)
        }
        .padding(.top, 8).padding(.bottom, 10)
        .background(Theme.card.opacity(0.6))
    }

    private func sendDraft() {
        app.send(draft)
        draft = ""
    }
}

/// The iPhone camera, for "look at this".
struct CameraPicker: UIViewControllerRepresentable {
    let done: (UIImage?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(done: done) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let done: (UIImage?) -> Void
        init(done: @escaping (UIImage?) -> Void) { self.done = done }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            done(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { done(nil) }
    }
}
