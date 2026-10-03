import CryptoKit
import Foundation

/// One line of the shared memory: something asked and answered, or a note.
/// The computer's Bader writes the same lines (bader_sync.py), so the format must match.
struct Entry: Codable, Identifiable, Equatable {
    var id: String
    var ts: Int
    var device: String
    var channel: String?
    var kind: String?
    var ask: String?
    var answer: String?
}

enum MemoryLogic {
    static let keep = 500

    /// Same recipe as the computer: sha1("<ts>|<ask>"), first 16 hex characters.
    static func id(ts: Int, ask: String) -> String {
        let digest = Insecure.SHA1.hash(data: Data("\(ts)|\(ask)".utf8))
        return String(digest.map { String(format: "%02x", $0) }.joined().prefix(16))
    }

    static func parse(_ text: String) -> [Entry] {
        let decoder = JSONDecoder()
        return text.split(whereSeparator: \.isNewline).compactMap { line in
            guard let e = try? decoder.decode(Entry.self, from: Data(line.utf8)), !e.id.isEmpty else { return nil }
            return e
        }
    }

    static func text(_ entries: [Entry]) -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.withoutEscapingSlashes]
        return entries.compactMap { e in
            (try? encoder.encode(e)).flatMap { String(data: $0, encoding: .utf8) }
        }.map { $0 + "\n" }.joined()
    }

    /// Union by id (the first one seen wins), newest `keep`, oldest first.
    static func merge(_ groups: [[Entry]], keep: Int = MemoryLogic.keep) -> [Entry] {
        var seen: [String: Entry] = [:]
        for group in groups {
            for e in group where seen[e.id] == nil { seen[e.id] = e }
        }
        let sorted = seen.values.sorted { ($0.ts, $0.id) < ($1.ts, $1.id) }
        return Array(sorted.suffix(keep))
    }
}

@MainActor
final class Memory: ObservableObject {
    @Published private(set) var entries: [Entry] = []
    @Published private(set) var lastSync: Date?
    @Published private(set) var syncError: String?

    private let file: URL = {
        let dir = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir.appendingPathComponent("bader-memory.jsonl")
    }()

    init() {
        if let text = try? String(contentsOf: file, encoding: .utf8) {
            entries = MemoryLogic.parse(text)
        }
    }

    private func store() {
        try? MemoryLogic.text(entries).write(to: file, atomically: true, encoding: .utf8)
    }

    func add(ask: String, answer: String, kind: String = "ask") {
        let ts = Int(Date().timeIntervalSince1970)
        let entry = Entry(id: MemoryLogic.id(ts: ts, ask: ask), ts: ts, device: "phone", channel: "phone",
                          kind: kind, ask: String(ask.prefix(400)), answer: String(answer.prefix(800)))
        entries = MemoryLogic.merge([entries, [entry]])
        store()
    }

    func clearLocal() {
        entries = []
        try? FileManager.default.removeItem(at: file)
    }

    /// Two-way sync with the file in the user's own Google Drive.
    func sync(_ google: Google?) async {
        guard let google else { return }
        do {
            let fileID = try await google.memoryFile()
            let remote = MemoryLogic.parse(try await google.download(fileID))
            let merged = MemoryLogic.merge([remote, entries])
            if Set(merged.map(\.id)) != Set(remote.map(\.id)) {
                try await google.upload(fileID, text: MemoryLogic.text(merged))
            }
            entries = merged
            store()
            lastSync = Date()
            syncError = nil
        } catch {
            syncError = error.localizedDescription
        }
    }

    /// The latest asks and notes, newest last, as plain lines for the model.
    func recent(_ count: Int) -> String {
        let f = DateFormatter()
        f.dateFormat = "EEE yyyy-MM-dd HH:mm"
        return entries.suffix(count).map { e in
            let when = f.string(from: Date(timeIntervalSince1970: TimeInterval(e.ts)))
            let place = e.device == "pc" ? "computer" : e.device
            if e.kind == "note" { return "- [\(when), note] \(e.answer ?? e.ask ?? "")" }
            return "- [\(when), \(place)] asked: \(e.ask ?? "") | answered: \((e.answer ?? "").prefix(260))"
        }.joined(separator: "\n")
    }

    /// Entries that mention every word of the query, newest first.
    func search(_ query: String, limit: Int = 12) -> [Entry] {
        let words = query.lowercased().split(separator: " ").map(String.init).filter { $0.count > 1 }
        guard !words.isEmpty else { return Array(entries.suffix(limit).reversed()) }
        return entries.reversed().filter { e in
            let hay = ((e.ask ?? "") + " " + (e.answer ?? "")).lowercased()
            return words.allSatisfy { hay.contains($0) }
        }.prefix(limit).map { $0 }
    }
}
