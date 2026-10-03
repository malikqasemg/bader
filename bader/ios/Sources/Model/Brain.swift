import Foundation

/// Something Bader may only do after the user says yes.
struct Approval: Identifiable, Equatable {
    let id = UUID()
    let title: String
    let detail: String
}

/// The thinking part on the phone: one AI model with a small set of tools.
struct Brain {
    let key: String
    let google: Google?
    let memory: Memory
    let primary: String
    let second: String
    var status: @MainActor (String) -> Void
    var approve: @MainActor (Approval) async -> Bool

    static let endpoint = URL(string: "https://openrouter.ai/api/v1/chat/completions")!

    // ── Tools ────────────────────────────────────────────────────────────────

    private static func tool(_ name: String, _ what: String, _ props: [String: [String: Any]], required: [String] = []) -> [String: Any] {
        ["type": "function", "function": ["name": name, "description": what,
            "parameters": ["type": "object", "properties": props, "required": required] as [String: Any]] as [String: Any]]
    }

    static let tools: [[String: Any]] = [
        tool("list_mail", "List the user's mail, newest first. Returns id, sender, subject, date, unread flag and a snippet.", [
            "query": ["type": "string", "description": "Optional Gmail search, e.g. 'from:sara', 'budget', 'is:unread'."],
            "days": ["type": "integer", "description": "How many days back (default 2)."],
            "max": ["type": "integer", "description": "How many mails (default 20, at most 40)."],
        ]),
        tool("read_mail", "Read one mail in full.", ["id": ["type": "string"]], required: ["id"]),
        tool("list_meetings", "List calendar meetings.", [
            "days_back": ["type": "integer", "description": "Days before today to include (default 0)."],
            "days_ahead": ["type": "integer", "description": "Days after today to include (default 1; 0 means today only)."],
        ]),
        tool("news", "Latest news headlines from the internet. No topic gives the top stories.", [
            "topic": ["type": "string"],
            "language": ["type": "string", "enum": ["en", "ar"]],
        ]),
        tool("weather", "Current weather and a 3-day outlook for a city.", ["city": ["type": "string"]], required: ["city"]),
        tool("recall", "Search everything asked and answered before, on the phone and on the computer.", [
            "query": ["type": "string", "description": "A few words to look for."],
        ], required: ["query"]),
        tool("remember", "Save a lasting note (a preference, a fact, a decision) so every Bader knows it.", [
            "note": ["type": "string"],
        ], required: ["note"]),
        tool("send_mail", "Send an email from the user's account. The phone asks the user to approve first.", [
            "to": ["type": "string"], "subject": ["type": "string"], "body": ["type": "string"],
        ], required: ["to", "subject", "body"]),
        tool("create_meeting", "Create a calendar meeting. The phone asks the user to approve first.", [
            "title": ["type": "string"],
            "start": ["type": "string", "description": "Local start, e.g. 2026-10-04T15:00:00"],
            "end": ["type": "string", "description": "Local end, e.g. 2026-10-04T15:30:00"],
            "attendees": ["type": "array", "items": ["type": "string"], "description": "Email addresses."],
        ], required: ["title", "start", "end"]),
    ]

    /// Words shown while a tool runs. The user never sees tool names.
    static func label(_ tool: String) -> String {
        switch tool {
        case "list_mail", "read_mail": return "Reading your mail…"
        case "list_meetings": return "Checking your calendar…"
        case "news": return "Getting the news…"
        case "weather": return "Checking the weather…"
        case "recall": return "Looking back…"
        case "remember": return "Noting that…"
        case "send_mail": return "Preparing the mail…"
        case "create_meeting": return "Preparing the meeting…"
        default: return "Working…"
        }
    }

    private func needGoogle() throws -> Google {
        guard let google else {
            throw BaderError("The Google account is not connected. Connect Google on the computer, then pair the phone again.")
        }
        return google
    }

    private func run(_ name: String, _ a: [String: Any]) async -> String {
        do {
            switch name {
            case "list_mail":
                let days = max(1, min(a["days"] as? Int ?? 2, 60))
                let count = max(1, min(a["max"] as? Int ?? 20, 40))
                let extra = (a["query"] as? String ?? "").trimmingCharacters(in: .whitespaces)
                let query = "newer_than:\(days)d " + (extra.isEmpty ? "in:inbox -category:promotions -category:social" : extra)
                let mails = try await needGoogle().mails(query: query, max: count)
                if mails.isEmpty { return "No mail found." }
                return mails.map { "\($0.id) | \($0.from) | \($0.subject) | \($0.date) | \($0.unread ? "unread" : "read") | \($0.snippet.prefix(160))" }
                    .joined(separator: "\n")
            case "read_mail":
                return try await needGoogle().read(a["id"] as? String ?? "")
            case "list_meetings":
                let cal = Calendar.current
                let today = cal.startOfDay(for: Date())
                let from = cal.date(byAdding: .day, value: -(a["days_back"] as? Int ?? 0), to: today)!
                let to = cal.date(byAdding: .day, value: (a["days_ahead"] as? Int ?? 1) + 1, to: today)!
                let items = try await needGoogle().meetings(from: from, to: to)
                if items.isEmpty { return "No meetings in that period." }
                return items.map { "\($0.start) to \($0.end) | \($0.title) | \($0.place) | \($0.people.joined(separator: ", "))" }
                    .joined(separator: "\n")
            case "news":
                return try await Web.news(topic: a["topic"] as? String, arabic: (a["language"] as? String ?? primary) == "ar")
            case "weather":
                return try await Web.weather(city: a["city"] as? String ?? "Riyadh")
            case "recall":
                let found = await memory.search(a["query"] as? String ?? "")
                if found.isEmpty { return "Nothing found." }
                let f = DateFormatter()
                f.dateFormat = "EEE yyyy-MM-dd HH:mm"
                return found.map { "[\(f.string(from: Date(timeIntervalSince1970: TimeInterval($0.ts)))), \($0.device)] asked: \($0.ask ?? "") | answered: \($0.answer ?? "")" }
                    .joined(separator: "\n")
            case "remember":
                let note = a["note"] as? String ?? ""
                await memory.add(ask: "Remember: \(note.prefix(60))", answer: note, kind: "note")
                return "Saved."
            case "send_mail":
                let to = a["to"] as? String ?? "", subject = a["subject"] as? String ?? "", body = a["body"] as? String ?? ""
                let google = try needGoogle()
                guard await approve(Approval(title: "Send this mail?", detail: "To: \(to)\nSubject: \(subject)\n\n\(body)")) else {
                    return "The user said no. Nothing was sent."
                }
                try await google.send(to: to, subject: subject, body: body)
                return "Sent."
            case "create_meeting":
                let title = a["title"] as? String ?? "", start = a["start"] as? String ?? "", end = a["end"] as? String ?? ""
                let people = a["attendees"] as? [String] ?? []
                let google = try needGoogle()
                let who = people.isEmpty ? "" : "\nWith: \(people.joined(separator: ", "))"
                guard await approve(Approval(title: "Create this meeting?", detail: "\(title)\n\(start) → \(end)\(who)")) else {
                    return "The user said no. Nothing was created."
                }
                try await google.createMeeting(title: title, start: start, end: end, attendees: people, zone: TimeZone.current.identifier)
                return "Created."
            default:
                return "Unknown tool."
            }
        } catch {
            return "Error: \(error.localizedDescription)"
        }
    }

    // ── The conversation ─────────────────────────────────────────────────────

    private func system() async -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "EEEE yyyy-MM-dd HH:mm"
        let name = { (code: String) in code == "ar" ? "Arabic" : "English" }
        let languages = second == "none" || second == primary ? name(primary) : "\(name(primary)) (main) and \(name(second))"
        let earlier = await memory.recent(18)
        let account = await google?.account ?? ""
        return """
        You are Bader (بدر), the personal assistant of a busy executive. You are running on their iPhone; \
        another Bader runs on their computer and shares the same memory.

        Rules:
        - Answer in the language of the user's message. Languages: \(languages).
        - Be brief. Start with the answer. Short lines. No tables, no headings. Plain text, at most a few "-" bullets.
        - For mail, meetings, news and weather always use the tools. Never invent mail, meetings or news.
        - To send a mail or create a meeting, call the tool once the details are clear. The phone shows the user an \
        approve button, so do not ask "shall I send it?" in text.
        - Never mention tools, files, models or how you work inside.
        - Things only the computer can do (controlling a browser, files on the computer, transcribing recordings): \
        say that this one needs Bader on the computer.
        - When the user asks you to remember something, or states a lasting preference, save it with remember.
        - The list below is what was asked before on every device. Use it when the user refers to earlier work.

        Now: \(f.string(from: Date())), time zone \(TimeZone.current.identifier).\(account.isEmpty ? "" : " The user's mail account: \(account).")

        Earlier asks and notes (oldest first):
        \(earlier.isEmpty ? "(none yet)" : earlier)
        """
    }

    /// Runs one question to its final answer. `turns` are the earlier messages of this chat plus the new one.
    func answer(turns: [[String: Any]]) async throws -> String {
        var messages: [[String: Any]] = [["role": "system", "content": await system()]] + turns
        for _ in 0..<8 {
            try Task.checkCancellation()
            var req = URLRequest(url: Self.endpoint, timeoutInterval: 90)
            req.httpMethod = "POST"
            req.setValue("Bearer \(key)", forHTTPHeaderField: "Authorization")
            req.setValue("application/json", forHTTPHeaderField: "Content-Type")
            req.setValue("Bader", forHTTPHeaderField: "X-Title")
            req.httpBody = try JSONSerialization.data(withJSONObject: [
                "model": Prefs.model, "messages": messages, "tools": Self.tools, "max_tokens": 1400, "temperature": 0.3,
            ] as [String: Any])
            let (data, resp) = try await URLSession.shared.data(for: req)
            let code = (resp as? HTTPURLResponse)?.statusCode ?? 0
            let json = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
            guard code == 200, let message = ((json["choices"] as? [[String: Any]])?.first)?["message"] as? [String: Any] else {
                if code == 401 { throw BaderError("The AI key was rejected. Pair again from the computer.") }
                if code == 402 { throw BaderError("The AI account is out of credit.") }
                let why = (json["error"] as? [String: Any])?["message"] as? String
                throw BaderError("The AI service did not answer (\(code))\(why.map { ": \($0)" } ?? "").")
            }
            let calls = message["tool_calls"] as? [[String: Any]] ?? []
            if calls.isEmpty {
                return (message["content"] as? String ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            }
            var echo: [String: Any] = ["role": "assistant", "tool_calls": calls]
            echo["content"] = message["content"] as? String ?? ""
            messages.append(echo)
            for call in calls {
                let fn = call["function"] as? [String: Any] ?? [:]
                let name = fn["name"] as? String ?? ""
                let args = (fn["arguments"] as? String).flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) } as? [String: Any] ?? [:]
                await status(Self.label(name))
                let result = await run(name, args)
                messages.append(["role": "tool", "tool_call_id": call["id"] as? String ?? "", "content": String(result.prefix(12000))])
            }
            await status("Thinking…")
        }
        throw BaderError("That took too many steps. Try a simpler question.")
    }
}
