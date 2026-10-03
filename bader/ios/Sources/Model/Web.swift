import Foundation

struct Headline: Equatable {
    var title = ""
    var source = ""
    var date = ""
    var link = ""
}

/// Reads the headlines out of a news feed (RSS).
final class FeedParser: NSObject, XMLParserDelegate {
    private var items: [Headline] = []
    private var current: Headline?
    private var text = ""

    static func parse(_ data: Data, limit: Int = 12) -> [Headline] {
        let me = FeedParser()
        let parser = XMLParser(data: data)
        parser.delegate = me
        parser.parse()
        return Array(me.items.prefix(limit))
    }

    func parser(_ parser: XMLParser, didStartElement name: String, namespaceURI: String?, qualifiedName: String?, attributes: [String: String] = [:]) {
        if name == "item" { current = Headline() }
        text = ""
    }

    func parser(_ parser: XMLParser, foundCharacters string: String) { text += string }

    func parser(_ parser: XMLParser, didEndElement name: String, namespaceURI: String?, qualifiedName: String?) {
        guard current != nil else { return }
        let value = text.trimmingCharacters(in: .whitespacesAndNewlines)
        switch name {
        case "title": current?.title = value
        case "source": current?.source = value
        case "pubDate": current?.date = value
        case "link": current?.link = value
        case "item":
            if let c = current, !c.title.isEmpty { items.append(c) }
            current = nil
        default: break
        }
    }
}

enum Web {
    private static func get(_ url: String) async throws -> Data {
        guard let target = URL(string: url) else { throw BaderError("Bad address.") }
        var req = URLRequest(url: target, timeoutInterval: 25)
        req.setValue("Mozilla/5.0 (iPhone) Bader", forHTTPHeaderField: "User-Agent")
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard (resp as? HTTPURLResponse)?.statusCode == 200 else { throw BaderError("The website did not answer.") }
        return data
    }

    private static func q(_ text: String) -> String {
        text.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? text
    }

    /// Latest headlines; a topic searches the last three days, no topic gives the top stories.
    static func news(topic: String?, arabic: Bool) async throws -> String {
        let locale = arabic ? "hl=ar&gl=SA&ceid=SA:ar" : "hl=en&gl=SA&ceid=SA:en"
        let topic = topic?.trimmingCharacters(in: .whitespaces) ?? ""
        let url = topic.isEmpty
            ? "https://news.google.com/rss?\(locale)"
            : "https://news.google.com/rss/search?q=\(q(topic + " when:3d"))&\(locale)"
        let items = FeedParser.parse(try await get(url))
        if items.isEmpty { return "No headlines found." }
        return items.map { "- \($0.title) (\($0.source), \($0.date))" }.joined(separator: "\n")
    }

    /// Current weather and a three-day outlook.
    static func weather(city: String) async throws -> String {
        let geo = try JSONSerialization.jsonObject(with: try await get("https://geocoding-api.open-meteo.com/v1/search?count=1&name=\(q(city))")) as? [String: Any]
        guard let place = (geo?["results"] as? [[String: Any]])?.first,
              let lat = place["latitude"] as? Double, let lon = place["longitude"] as? Double else {
            return "City not found."
        }
        let url = "https://api.open-meteo.com/v1/forecast?latitude=\(lat)&longitude=\(lon)"
            + "&current=temperature_2m,apparent_temperature,relative_humidity_2m,weather_code,wind_speed_10m"
            + "&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max&timezone=auto&forecast_days=3"
        let text = String(decoding: try await get(url), as: UTF8.self)
        return "Place: \(place["name"] as? String ?? city), \(place["country"] as? String ?? ""). Celsius, km/h, WMO weather codes.\n\(text.prefix(1800))"
    }
}
