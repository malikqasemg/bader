import Foundation

struct Mail: Identifiable {
    let id: String
    let from: String
    let subject: String
    let date: String
    let snippet: String
    let unread: Bool
}

struct Meeting: Identifiable {
    let id: String
    let title: String
    let start: String
    let end: String
    let place: String
    let people: [String]
}

/// The user's Google account: mail, calendar and the shared memory file in Drive.
actor Google {
    private let keys: GoogleKeys
    private var access: String?
    private var expires = Date.distantPast

    init(keys: GoogleKeys) { self.keys = keys }

    var account: String { keys.a ?? "" }

    // ── Plumbing ─────────────────────────────────────────────────────────────

    private func token() async throws -> String {
        if let access, expires > Date().addingTimeInterval(60) { return access }
        var req = URLRequest(url: URL(string: "https://oauth2.googleapis.com/token")!)
        req.httpMethod = "POST"
        req.setValue("application/x-www-form-urlencoded", forHTTPHeaderField: "Content-Type")
        var form = URLComponents()
        form.queryItems = [
            URLQueryItem(name: "client_id", value: keys.id),
            URLQueryItem(name: "client_secret", value: keys.s),
            URLQueryItem(name: "refresh_token", value: keys.r),
            URLQueryItem(name: "grant_type", value: "refresh_token"),
        ]
        req.httpBody = form.percentEncodedQuery?.replacingOccurrences(of: "+", with: "%2B").data(using: .utf8)
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let value = json["access_token"] as? String else {
            throw BaderError("Google sign-in has expired. Pair again from the computer.")
        }
        access = value
        expires = Date().addingTimeInterval(TimeInterval(json["expires_in"] as? Int ?? 3000))
        return value
    }

    private func call(_ method: String, _ url: String, body: Data? = nil, type: String? = nil) async throws -> Data {
        guard let target = URL(string: url) else { throw BaderError("Bad address.") }
        var req = URLRequest(url: target, timeoutInterval: 40)
        req.httpMethod = method
        req.setValue("Bearer \(try await token())", forHTTPHeaderField: "Authorization")
        if let type { req.setValue(type, forHTTPHeaderField: "Content-Type") }
        req.httpBody = body
        let (data, resp) = try await URLSession.shared.data(for: req)
        let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(code) else {
            let why = ((try? JSONSerialization.jsonObject(with: data)) as? [String: Any])
                .flatMap { $0["error"] as? [String: Any] }.flatMap { $0["message"] as? String }
            throw BaderError("Google answered \(code)\(why.map { ": \($0)" } ?? "")")
        }
        return data
    }

    private func json(_ method: String, _ url: String, body: [String: Any]? = nil) async throws -> [String: Any] {
        let data = try await call(method, url, body: body.map { try! JSONSerialization.data(withJSONObject: $0) },
                                  type: body == nil ? nil : "application/json")
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    }

    private static func q(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? text
    }

    // ── Mail ─────────────────────────────────────────────────────────────────

    private static func header(_ payload: [String: Any], _ name: String) -> String {
        let headers = payload["headers"] as? [[String: Any]] ?? []
        return headers.first { ($0["name"] as? String)?.lowercased() == name.lowercased() }?["value"] as? String ?? ""
    }

    private func mail(_ id: String) async throws -> Mail {
        let m = try await json("GET", "https://gmail.googleapis.com/gmail/v1/users/me/messages/\(id)?format=metadata&metadataHeaders=From&metadataHeaders=Subject&metadataHeaders=Date")
        let payload = m["payload"] as? [String: Any] ?? [:]
        let labels = m["labelIds"] as? [String] ?? []
        return Mail(id: id, from: Self.header(payload, "From"), subject: Self.header(payload, "Subject"),
                    date: Self.header(payload, "Date"), snippet: (m["snippet"] as? String ?? "").decodingEntities,
                    unread: labels.contains("UNREAD"))
    }

    /// Mail matching a Gmail search, newest first.
    func mails(query: String, max: Int) async throws -> [Mail] {
        let list = try await json("GET", "https://gmail.googleapis.com/gmail/v1/users/me/messages?maxResults=\(max)&q=\(Self.q(query))")
        let ids = (list["messages"] as? [[String: Any]] ?? []).compactMap { $0["id"] as? String }
        return try await withThrowingTaskGroup(of: (Int, Mail).self) { group in
            for (n, id) in ids.enumerated() {
                group.addTask { (n, try await self.mail(id)) }
            }
            var out: [(Int, Mail)] = []
            for try await item in group { out.append(item) }
            return out.sorted { $0.0 < $1.0 }.map(\.1)
        }
    }

    func unreadCount() async throws -> Int {
        let label = try await json("GET", "https://gmail.googleapis.com/gmail/v1/users/me/labels/INBOX")
        return label["messagesUnread"] as? Int ?? 0
    }

    private static func bodyText(_ part: [String: Any]) -> String? {
        let mime = part["mimeType"] as? String ?? ""
        if let parts = part["parts"] as? [[String: Any]] {
            let plain = parts.first { ($0["mimeType"] as? String) == "text/plain" }
            for p in ([plain].compactMap { $0 } + parts) {
                if let text = bodyText(p), !text.isEmpty { return text }
            }
            return nil
        }
        guard mime.hasPrefix("text/"),
              let raw = (part["body"] as? [String: Any])?["data"] as? String,
              let data = Data(base64URL: raw),
              let text = String(data: data, encoding: .utf8) else { return nil }
        return mime == "text/html" ? text.strippingHTML : text
    }

    /// One mail in full (plain text, cut to a sensible length).
    func read(_ id: String) async throws -> String {
        let m = try await json("GET", "https://gmail.googleapis.com/gmail/v1/users/me/messages/\(id)?format=full")
        let payload = m["payload"] as? [String: Any] ?? [:]
        let body = Self.bodyText(payload) ?? (m["snippet"] as? String ?? "")
        return """
        From: \(Self.header(payload, "From"))
        To: \(Self.header(payload, "To"))
        Date: \(Self.header(payload, "Date"))
        Subject: \(Self.header(payload, "Subject"))

        \(body.prefix(6000))
        """
    }

    func send(to: String, subject: String, body: String) async throws {
        let b64 = { (s: String) in Data(s.utf8).base64EncodedString() }
        let lines = [
            "To: \(to)",
            "Subject: =?UTF-8?B?\(b64(subject))?=",
            "MIME-Version: 1.0",
            "Content-Type: text/plain; charset=UTF-8",
            "Content-Transfer-Encoding: base64",
            "",
            Data(body.utf8).base64EncodedString(options: [.lineLength76Characters, .endLineWithCarriageReturn, .endLineWithLineFeed]),
        ]
        let raw = Data(lines.joined(separator: "\r\n").utf8).base64URL
        _ = try await json("POST", "https://gmail.googleapis.com/gmail/v1/users/me/messages/send", body: ["raw": raw])
    }

    // ── Calendar ─────────────────────────────────────────────────────────────

    func meetings(from: Date, to: Date) async throws -> [Meeting] {
        let iso = ISO8601DateFormatter()
        let url = "https://www.googleapis.com/calendar/v3/calendars/primary/events?singleEvents=true&orderBy=startTime&maxResults=50"
            + "&timeMin=\(Self.q(iso.string(from: from)))&timeMax=\(Self.q(iso.string(from: to)))"
        let list = try await json("GET", url)
        return (list["items"] as? [[String: Any]] ?? []).map { e in
            let time = { (key: String) -> String in
                let t = e[key] as? [String: Any] ?? [:]
                return t["dateTime"] as? String ?? t["date"] as? String ?? ""
            }
            let place = e["hangoutLink"] as? String ?? e["location"] as? String ?? ""
            let people = (e["attendees"] as? [[String: Any]] ?? []).compactMap { $0["email"] as? String }
            return Meeting(id: e["id"] as? String ?? UUID().uuidString, title: e["summary"] as? String ?? "(no title)",
                           start: time("start"), end: time("end"), place: place, people: Array(people.prefix(8)))
        }
    }

    func createMeeting(title: String, start: String, end: String, attendees: [String], zone: String) async throws {
        var event: [String: Any] = [
            "summary": title,
            "start": ["dateTime": start, "timeZone": zone],
            "end": ["dateTime": end, "timeZone": zone],
        ]
        if !attendees.isEmpty { event["attendees"] = attendees.map { ["email": $0] } }
        _ = try await json("POST", "https://www.googleapis.com/calendar/v3/calendars/primary/events?sendUpdates=all", body: event)
    }

    // ── Shared memory file in Drive ──────────────────────────────────────────

    private var memoryID: String?

    func memoryFile() async throws -> String {
        if let memoryID { return memoryID }
        let query = "name = 'bader-memory.jsonl' and trashed = false and appProperties has { key='bader' and value='memory' }"
        let found = try await json("GET", "https://www.googleapis.com/drive/v3/files?spaces=drive&fields=files(id)&q=\(Self.q(query))")
        if let id = (found["files"] as? [[String: Any]])?.first?["id"] as? String {
            memoryID = id
            return id
        }
        let made = try await json("POST", "https://www.googleapis.com/drive/v3/files?fields=id", body: [
            "name": "bader-memory.jsonl", "mimeType": "text/plain", "appProperties": ["bader": "memory"],
            "description": "Bader's shared memory. Bader keeps this up to date - please do not edit or delete.",
        ])
        guard let id = made["id"] as? String else { throw BaderError("Could not create the memory file.") }
        memoryID = id
        return id
    }

    func download(_ id: String) async throws -> String {
        String(decoding: try await call("GET", "https://www.googleapis.com/drive/v3/files/\(id)?alt=media"), as: UTF8.self)
    }

    func upload(_ id: String, text: String) async throws {
        _ = try await call("PATCH", "https://www.googleapis.com/upload/drive/v3/files/\(id)?uploadType=media",
                           body: Data(text.utf8), type: "text/plain; charset=utf-8")
    }
}

extension Data {
    init?(base64URL text: String) {
        var s = text.replacingOccurrences(of: "-", with: "+").replacingOccurrences(of: "_", with: "/")
        while s.count % 4 != 0 { s += "=" }
        self.init(base64Encoded: s)
    }

    var base64URL: String {
        base64EncodedString().replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
    }
}

extension String {
    var decodingEntities: String {
        var s = self
        for (a, b) in [("&#39;", "'"), ("&quot;", "\""), ("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&nbsp;", " ")] {
            s = s.replacingOccurrences(of: a, with: b)
        }
        return s
    }

    var strippingHTML: String {
        var s = replacingOccurrences(of: "<(style|script)[^>]*>[\\s\\S]*?</(style|script)>", with: " ", options: [.regularExpression, .caseInsensitive])
        s = s.replacingOccurrences(of: "<br[^>]*>|</p>|</div>|</tr>", with: "\n", options: [.regularExpression, .caseInsensitive])
        s = s.replacingOccurrences(of: "<[^>]+>", with: " ", options: .regularExpression).decodingEntities
        s = s.replacingOccurrences(of: "[ \\t]+", with: " ", options: .regularExpression)
        return s.replacingOccurrences(of: "\\n\\s*\\n+", with: "\n\n", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
