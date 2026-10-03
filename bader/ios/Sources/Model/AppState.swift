import Combine
import SwiftUI
import UIKit

struct ChatMessage: Identifiable, Equatable {
    enum Role { case user, bader, problem }
    let id = UUID()
    let role: Role
    var text: String
    var image: UIImage?
}

/// Everything the screens show, and the actions behind the buttons.
@MainActor
final class AppState: ObservableObject {
    @Published var pairing: Pairing?
    @Published var messages: [ChatMessage] = []
    @Published var status = ""
    @Published var face = "idle"
    @Published var busy = false
    @Published var approval: Approval?
    @Published var unread: Int?
    @Published var speak = Prefs.speak { didSet { Prefs.speak = speak } }
    @Published var natural = Prefs.natural { didSet { Prefs.natural = natural } }

    @Published var display = Prefs.display {
        didSet {
            Prefs.display = display
            if display, pairing != nil { link.start() } else { link.stop() }
        }
    }

    let memory = Memory()
    let voice = Voice()
    let link = FaceLink()
    private lazy var screen = FaceScreen(link: link)
    private var watching: Set<AnyCancellable> = []
    private(set) var google: Google?
    private var turns: [[String: Any]] = []
    private var approvalAnswer: CheckedContinuation<Bool, Never>?
    private var work: Task<Void, Never>?

    init() {
        if let saved = Vault.load() { adopt(saved) }
        voice.onSilence = { [weak self] in self?.finishTalking() }
        screen.actions = FaceScreen.Actions(
            talk: { [weak self] in self?.toggleTalk() },
            ask: { [weak self] in self?.send($0) },
            cancel: { [weak self] in self?.cancel() },
            approve: { [weak self] in self?.answer(approval: $0) })
        // Whatever changes here is mirrored on the small display (only the parts that look different are sent).
        objectWillChange.merge(with: voice.objectWillChange)
            .debounce(for: .milliseconds(70), scheduler: RunLoop.main)
            .sink { [weak self] in self?.mirror() }
            .store(in: &watching)
        if display, pairing != nil { link.start() }
    }

    private func mirror() {
        screen.face = face
        screen.status = status
        screen.unread = unread
        screen.busy = busy
        screen.listening = voice.recording
        screen.speaking = voice.speaking
        screen.approval = approval
        screen.refresh()
    }

    private func adopt(_ p: Pairing) {
        pairing = p
        google = p.g.map { Google(keys: $0) }
        voice.cloudKey = p.ai
    }

    var languages: [String] {
        let first = pairing?.l1 ?? "en"
        let second = pairing?.l2 ?? "ar"
        return second == "none" || second == first ? [first] : [first, second]
    }

    // ── Pairing ──────────────────────────────────────────────────────────────

    /// Takes the text of a scanned pairing code. False when it is not a Bader code.
    func pair(with text: String) -> Bool {
        guard let p = Pairing.parse(text) else { return false }
        Vault.save(p)
        adopt(p)
        refresh()
        if display { link.start() }
        return true
    }

    func unpair() {
        cancel()
        Vault.clear()
        link.stop()
        memory.clearLocal()
        pairing = nil
        google = nil
        voice.cloudKey = nil
        messages = []
        turns = []
        unread = nil
    }

    /// Brings memory and the unread count up to date (app opened, or pulled down).
    func refresh() {
        Task {
            await memory.sync(google)
            unread = try? await google?.unreadCount()
        }
    }

    // ── Asking ───────────────────────────────────────────────────────────────

    func send(_ text: String, image: UIImage? = nil, aloud: Bool? = nil) {
        let ask = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let pairing, !busy, !(ask.isEmpty && image == nil) else { return }
        voice.stopSpeaking()
        let shown = ask.isEmpty ? "What is this?" : ask
        messages.append(ChatMessage(role: .user, text: shown, image: image))

        var content: Any = shown
        if let image, let jpeg = image.fitted(1280).jpegData(compressionQuality: 0.7) {
            content = [
                ["type": "text", "text": shown],
                ["type": "image_url", "image_url": ["url": "data:image/jpeg;base64,\(jpeg.base64EncodedString())"]],
            ] as [[String: Any]]
        }
        let sending = turns + [["role": "user", "content": content]]
        // Later turns carry the words only: a photo is sent once.
        turns.append(["role": "user", "content": image == nil ? shown : "\(shown) [the user showed a photo]"])

        busy = true
        face = "thinking"
        status = "Thinking…"
        let brain = Brain(key: pairing.ai, google: google, memory: memory,
                          primary: pairing.l1 ?? "en", second: pairing.l2 ?? "ar",
                          status: { [weak self] in self?.status = $0 },
                          approve: { [weak self] in await self?.ask(approval: $0) ?? false })
        let speakIt = aloud ?? speak
        work = Task {
            do {
                let answer = try await brain.answer(turns: sending)
                guard !Task.isCancelled else { return }
                let text = answer.isEmpty ? "…" : answer
                messages.append(ChatMessage(role: .bader, text: text))
                turns.append(["role": "assistant", "content": text])
                if turns.count > 24 { turns.removeFirst(turns.count - 24) }
                memory.add(ask: shown, answer: text)
                face = "happy"
                if speakIt { voice.speak(text) }
                if link.usable { screen.show(answer: text) }
                Task { await memory.sync(google) }
            } catch is CancellationError {
            } catch {
                if !Task.isCancelled {
                    turns.removeLast()
                    messages.append(ChatMessage(role: .problem, text: error.localizedDescription))
                    face = "concerned"
                }
            }
            busy = false
            status = ""
            try? await Task.sleep(for: .seconds(3))
            if !busy, !voice.recording { face = "idle" }
        }
    }

    func cancel() {
        work?.cancel()
        work = nil
        answer(approval: false)
        if busy, !turns.isEmpty { turns.removeLast() }
        busy = false
        status = ""
        face = "idle"
        voice.stopSpeaking()
        if voice.recording { _ = voice.stop() }
    }

    // ── Approvals ────────────────────────────────────────────────────────────

    private func ask(approval request: Approval) async -> Bool {
        face = "surprised"
        status = "Waiting for you"
        approval = request
        return await withCheckedContinuation { approvalAnswer = $0 }
    }

    func answer(approval ok: Bool) {
        approval = nil
        approvalAnswer?.resume(returning: ok)
        approvalAnswer = nil
        if busy {
            face = "thinking"
            status = "Thinking…"
        }
    }

    // ── Talking ──────────────────────────────────────────────────────────────

    func toggleTalk() {
        if voice.recording { return finishTalking() }
        if voice.speaking { return voice.stopSpeaking() }
        guard !busy, pairing != nil else { return }
        Task {
            guard await Voice.allowed() else {
                messages.append(ChatMessage(role: .problem, text: "Allow the microphone for Bader in the iPhone's Settings."))
                return
            }
            do {
                try voice.start()
                face = "listening"
                status = "Listening…"
            } catch {
                messages.append(ChatMessage(role: .problem, text: error.localizedDescription))
            }
        }
    }

    private func finishTalking() {
        guard voice.recording, let pairing else { return }
        let file = voice.stop()
        guard let file else {
            face = "idle"
            status = ""
            return
        }
        face = "thinking"
        status = "Got it…"
        busy = true
        Task {
            do {
                let heard = try await voice.transcribe(file, key: pairing.ai, languages: languages)
                busy = false
                if heard.text.isEmpty {
                    face = "idle"
                    status = ""
                } else {
                    send(heard.text, aloud: true)
                }
            } catch {
                busy = false
                face = "concerned"
                status = ""
                messages.append(ChatMessage(role: .problem, text: error.localizedDescription))
            }
        }
    }
}

extension UIImage {
    /// The same picture, no larger than `side` on its long edge.
    func fitted(_ side: CGFloat) -> UIImage {
        let scale = min(1, side / max(size.width, size.height))
        guard scale < 1 else { return self }
        let target = CGSize(width: size.width * scale, height: size.height * scale)
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        return UIGraphicsImageRenderer(size: target, format: format).image { _ in
            draw(in: CGRect(origin: .zero, size: target))
        }
    }
}
