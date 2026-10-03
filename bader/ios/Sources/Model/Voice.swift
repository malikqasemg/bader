import AVFoundation
import Foundation

/// Ears and mouth: records the user, turns speech into text, and reads answers aloud.
@MainActor
final class Voice: NSObject, ObservableObject, AVSpeechSynthesizerDelegate {
    @Published private(set) var recording = false
    @Published private(set) var speaking = false
    @Published private(set) var level: Double = 0

    /// Called when the user stops talking (or never started).
    var onSilence: (() -> Void)?

    private var recorder: AVAudioRecorder?
    private var meter: Timer?
    private var started = Date()
    private var lastLoud = Date()
    private var heard = false
    private let synth = AVSpeechSynthesizer()
    /// The AI service key. With it (and "Natural voice" on) answers are read by the cloud voice.
    var cloudKey: String?
    private var player: AVAudioPlayer?
    private var speakID = 0
    private var cloudBusy = false
    private let file = FileManager.default.temporaryDirectory.appendingPathComponent("bader-ask.wav")

    override init() {
        super.init()
        synth.delegate = self
    }

    private func session() throws {
        let s = AVAudioSession.sharedInstance()
        try s.setCategory(.playAndRecord, mode: .default, options: [.defaultToSpeaker, .allowBluetooth])
        try s.setActive(true)
    }

    static func allowed() async -> Bool {
        await withCheckedContinuation { done in
            AVAudioApplication.requestRecordPermission { done.resume(returning: $0) }
        }
    }

    // ── Listening ────────────────────────────────────────────────────────────

    func start() throws {
        stopSpeaking()
        try session()
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM, AVSampleRateKey: 16000, AVNumberOfChannelsKey: 1,
            AVLinearPCMBitDepthKey: 16, AVLinearPCMIsFloatKey: false, AVLinearPCMIsBigEndianKey: false,
        ]
        let r = try AVAudioRecorder(url: file, settings: settings)
        r.isMeteringEnabled = true
        guard r.record() else { throw BaderError("The microphone is busy.") }
        recorder = r
        recording = true
        started = Date()
        lastLoud = Date()
        heard = false
        meter = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tick() }
        }
    }

    private func tick() {
        guard let recorder, recording else { return }
        recorder.updateMeters()
        let db = Double(recorder.averagePower(forChannel: 0))
        level = max(0, min(1, (db + 50) / 40))
        let now = Date()
        if db > -32 {
            heard = true
            lastLoud = now
        }
        let quietFor = now.timeIntervalSince(lastLoud)
        let total = now.timeIntervalSince(started)
        if (heard && quietFor > 1.7) || (!heard && total > 7) || total > 90 {
            onSilence?()
        }
    }

    /// Stops recording. Returns the sound file, or nil when nothing was said.
    func stop() -> URL? {
        meter?.invalidate()
        meter = nil
        recorder?.stop()
        recorder = nil
        recording = false
        level = 0
        return heard ? file : nil
    }

    /// Speech to text through the AI service (understands Arabic and English without being told which).
    func transcribe(_ url: URL, key: String, languages: [String]) async throws -> (text: String, language: String) {
        let audio = try Data(contentsOf: url).base64EncodedString()
        func ask(_ language: String?) async throws -> [String: Any] {
            var body: [String: Any] = ["model": "openai/whisper-large-v3", "response_format": "verbose_json",
                                       "input_audio": ["data": audio, "format": "wav"]]
            if let language { body["language"] = language }
            var req = URLRequest(url: URL(string: "https://openrouter.ai/api/v1/audio/transcriptions")!, timeoutInterval: 60)
            req.httpMethod = "POST"
            req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.httpBody = try JSONSerialization.data(withJSONObject: body)
            let (data, resp) = try await URLSession.shared.data(for: req)
            guard (resp as? HTTPURLResponse)?.statusCode == 200,
                  let json = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                throw BaderError("Could not understand the recording. Try again.")
            }
            return json
        }
        var out = try await ask(languages.count == 1 ? languages[0] : nil)
        let names = ["arabic": "ar", "english": "en"]
        var language = (out["language"] as? String ?? "").lowercased()
        language = names[language] ?? language
        if languages.count > 1, !language.isEmpty, !languages.contains(language) {
            // Heard as some other language: ask again in the main one.
            out = try await ask(languages[0])
            language = languages[0]
        }
        return ((out["text"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines), language.isEmpty ? languages[0] : language)
    }

    // ── Speaking ─────────────────────────────────────────────────────────────

    nonisolated static func isArabic(_ text: String) -> Bool {
        let letters = text.unicodeScalars.filter { CharacterSet.letters.contains($0) }
        guard !letters.isEmpty else { return false }
        let arabic = letters.filter { (0x0600...0x06FF).contains($0.value) || (0x0750...0x077F).contains($0.value) }
        return Double(arabic.count) / Double(letters.count) > 0.4
    }

    /// Text without the marks that would be read out loud ("star star", links…).
    nonisolated static func speakable(_ text: String) -> String {
        var s = text.replacingOccurrences(of: "https?://\\S+", with: "", options: .regularExpression)
        s = s.replacingOccurrences(of: "[*_#`>|]", with: "", options: .regularExpression)
        s = s.replacingOccurrences(of: "(?m)^\\s*[-•]\\s*", with: "", options: .regularExpression)
        return s.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func voice(arabic: Bool) -> AVSpeechSynthesisVoice? {
        let prefix = arabic ? "ar" : "en"
        let best = AVSpeechSynthesisVoice.speechVoices()
            .filter { $0.language.hasPrefix(prefix) && (arabic || $0.language == "en-US" || $0.language == "en-GB") }
            .max { $0.quality.rawValue < $1.quality.rawValue }
        return best ?? AVSpeechSynthesisVoice(language: arabic ? "ar-001" : "en-US")
    }

    /// The text in pieces short enough to start speaking quickly: the first sentence alone, then groups.
    nonisolated static func chunks(_ text: String, limit: Int = 260) -> [String] {
        var sentences: [String] = []
        var current = ""
        for ch in text {
            current.append(ch)
            if ".!?؟\n".contains(ch), current.trimmingCharacters(in: .whitespacesAndNewlines).count > 1 {
                sentences.append(current.trimmingCharacters(in: .whitespacesAndNewlines))
                current = ""
            }
        }
        let rest = current.trimmingCharacters(in: .whitespacesAndNewlines)
        if !rest.isEmpty { sentences.append(rest) }
        var out: [String] = []
        for s in sentences where !s.isEmpty {
            if let last = out.last, out.count > 1, last.count + s.count + 1 <= limit {
                out[out.count - 1] = last + " " + s
            } else {
                out.append(s)
            }
        }
        return out
    }

    /// Raw sound from the cloud voice (16-bit, 24 kHz, one channel) wrapped as a WAV file.
    nonisolated static func wav(_ pcm: Data, rate: UInt32 = 24000) -> Data {
        var d = Data()
        func le<T: FixedWidthInteger>(_ v: T) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
        d.append(contentsOf: Array("RIFF".utf8)); le(UInt32(36 + pcm.count)); d.append(contentsOf: Array("WAVEfmt ".utf8))
        le(UInt32(16)); le(UInt16(1)); le(UInt16(1)); le(rate); le(rate * 2); le(UInt16(2)); le(UInt16(16))
        d.append(contentsOf: Array("data".utf8)); le(UInt32(pcm.count)); d.append(pcm)
        return d
    }

    private nonisolated static func cloudSound(_ text: String, key: String) async -> Data? {
        var req = URLRequest(url: URL(string: "https://openrouter.ai/api/v1/audio/speech")!, timeoutInterval: 40)
        req.httpMethod = "POST"
        req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.httpBody = try? JSONSerialization.data(withJSONObject: [
            "model": "google/gemini-3.8-flash-lite-tts", "input": text, "voice": "Charon", "response_format": "pcm",
        ])
        guard let (data, resp) = try? await URLSession.shared.data(for: req),
              (resp as? HTTPURLResponse)?.statusCode == 200, data.count > 2000 else { return nil }
        return wav(data)
    }

    func speak(_ text: String) {
        let clean = Self.speakable(text)
        guard !clean.isEmpty else { return }
        stopSpeaking()
        try? session()
        guard Prefs.natural, let key = cloudKey else { return builtIn(clean) }
        let mine = speakID
        let parts = Self.chunks(clean)
        cloudBusy = true
        speaking = true
        Task { @MainActor in
            // Fetch one piece ahead, so the next one is ready when the current one ends.
            var next: Task<Data?, Never>? = Task.detached { await Self.cloudSound(parts[0], key: key) }
            for i in parts.indices {
                let sound = await next?.value
                next = nil
                if i + 1 < parts.count {
                    let following = parts[i + 1]
                    next = Task.detached { await Self.cloudSound(following, key: key) }
                }
                guard mine == speakID else { return }
                guard let sound, let p = try? AVAudioPlayer(data: sound) else {
                    // The cloud voice failed: finish with the phone's own voice.
                    cloudBusy = false
                    return builtIn(parts[i...].joined(separator: " "))
                }
                player = p
                p.play()
                while p.isPlaying, mine == speakID { try? await Task.sleep(for: .milliseconds(80)) }
                guard mine == speakID else { return }
            }
            cloudBusy = false
            speaking = false
        }
    }

    private func builtIn(_ clean: String) {
        let u = AVSpeechUtterance(string: clean)
        u.voice = Self.voice(arabic: Self.isArabic(clean))
        u.rate = AVSpeechUtteranceDefaultSpeechRate
        speaking = true
        synth.speak(u)
    }

    func stopSpeaking() {
        speakID += 1
        cloudBusy = false
        player?.stop()
        player = nil
        synth.stopSpeaking(at: .immediate)
        speaking = false
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        Task { @MainActor in self.speaking = self.synth.isSpeaking || self.cloudBusy }
    }

    nonisolated func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        Task { @MainActor in self.speaking = self.synth.isSpeaking || self.cloudBusy }
    }
}
