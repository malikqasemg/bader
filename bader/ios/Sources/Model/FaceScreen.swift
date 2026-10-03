import UIKit

/// What the phone draws on Bader's small display, and what its touches do.
/// The display only shows pictures of regions, so all text (Arabic included) is drawn here.
@MainActor
final class FaceScreen {
    struct Button: Equatable {
        let label: String
        let color: UIColor
        let action: String
    }

    /// What the buttons need from the app.
    struct Actions {
        var talk: () -> Void
        var ask: (String) -> Void
        var cancel: () -> Void
        var approve: (Bool) -> Void
    }

    let link: FaceLink
    var actions: Actions?

    // What the app is doing right now (set by AppState).
    var face = "idle"
    var status = ""
    var unread: Int?
    var busy = false
    var listening = false
    var speaking = false
    var approval: Approval?

    private var answer: String?
    private var answerPage = 0
    private var answerPages = 1
    private var sent: [String: Data] = [:]
    private var sentFace = ""
    private var clock: Timer?

    private static let ink = UIColor(red: 0.92, green: 0.93, blue: 0.95, alpha: 1)
    private static let grey = UIColor(red: 0.59, green: 0.63, blue: 0.67, alpha: 1)
    private static let panel = UIColor(red: 0.086, green: 0.11, blue: 0.15, alpha: 1)
    private static let blue = UIColor(red: 0.16, green: 0.42, blue: 0.86, alpha: 1)
    private static let green = UIColor(red: 0.13, green: 0.62, blue: 0.33, alpha: 1)
    private static let red = UIColor(red: 0.80, green: 0.22, blue: 0.27, alpha: 1)
    private static let amber = UIColor(red: 1.0, green: 0.77, blue: 0.25, alpha: 1)
    private static let key = UIColor(red: 0.16, green: 0.20, blue: 0.27, alpha: 1)
    private static let barH = 28
    private static let linkEnd = 86  // the display draws its own PC / PHONE button up to here

    init(link: FaceLink) {
        self.link = link
        link.onReady = { [weak self] in self?.redrawAll() }
        link.onTouch = { [weak self] x, y in self?.touch(x, y) }
        link.onGone = { [weak self] in self?.sent.removeAll(); self?.sentFace = "" }
        clock = Timer.scheduledTimer(withTimeInterval: 20, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.drawBar() }
        }
    }

    // ── Layout: buttons along the bottom when upright, down the right side when on its side ──

    private struct Geo {
        let w: Int
        let land: Bool
        let info: CGRect
        let buttons: CGRect
        let page: CGRect
    }

    private var geo: Geo {
        let w = link.size?.w ?? 240
        let h = link.size?.h ?? 320
        return w > h
            ? Geo(w: w, land: true, info: CGRect(x: 0, y: 204, width: 240, height: 36),
                  buttons: CGRect(x: 240, y: 28, width: 80, height: 212), page: CGRect(x: 0, y: 28, width: 240, height: 212))
            : Geo(w: w, land: false, info: CGRect(x: 0, y: 204, width: 240, height: 60),
                  buttons: CGRect(x: 0, y: 264, width: 240, height: 56), page: CGRect(x: 0, y: 28, width: 240, height: 236))
    }

    // ── Painting ─────────────────────────────────────────────────────────────

    /// Draws with UIKit into a w×h picture and returns it as the display's pixels (RGB565, high byte first).
    nonisolated static func paint(_ w: Int, _ h: Int, _ draw: (CGRect) -> Void) -> Data {
        var rgba = [UInt8](repeating: 0, count: w * h * 4)
        rgba.withUnsafeMutableBytes { raw in
            guard let ctx = CGContext(data: raw.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w * 4,
                                      space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue) else { return }
            ctx.translateBy(x: 0, y: CGFloat(h))
            ctx.scaleBy(x: 1, y: -1)
            UIGraphicsPushContext(ctx)
            UIColor.black.setFill()
            UIRectFill(CGRect(x: 0, y: 0, width: w, height: h))
            draw(CGRect(x: 0, y: 0, width: w, height: h))
            UIGraphicsPopContext()
        }
        var out = Data(count: w * h * 2)
        out.withUnsafeMutableBytes { (dst: UnsafeMutableRawBufferPointer) in
            for i in 0..<(w * h) {
                let r = UInt16(rgba[i * 4]), g = UInt16(rgba[i * 4 + 1]), b = UInt16(rgba[i * 4 + 2])
                let v = ((r & 0xF8) << 8) | ((g & 0xFC) << 3) | (b >> 3)
                dst[i * 2] = UInt8(v >> 8)
                dst[i * 2 + 1] = UInt8(v & 0xFF)
            }
        }
        return out
    }

    private static func text(_ s: String, in rect: CGRect, size: CGFloat, bold: Bool = false, color: UIColor = .white,
                             align: NSTextAlignment = .center) {
        let style = NSMutableParagraphStyle()
        style.alignment = align
        style.lineBreakMode = .byTruncatingTail
        let attrs: [NSAttributedString.Key: Any] = [
            .font: UIFont.systemFont(ofSize: size, weight: bold ? .semibold : .medium),
            .foregroundColor: color, .paragraphStyle: style,
        ]
        let height = ceil((s as NSString).size(withAttributes: attrs).height)
        (s as NSString).draw(in: CGRect(x: rect.minX, y: rect.midY - height / 2, width: rect.width, height: height), withAttributes: attrs)
    }

    /// Sends a region only when its picture changed.
    private func put(_ key: String, _ r: CGRect, _ pixels: Data) {
        guard link.usable, sent[key] != pixels else { return }
        sent[key] = pixels
        let (x, y, w, h) = (Int(r.minX), Int(r.minY), Int(r.width), Int(r.height))
        link.enqueue { [link] in _ = await link.image(x: x, y: y, w: w, h: h, pixels: pixels) }
    }

    // ── The parts ────────────────────────────────────────────────────────────

    private func drawBar() {
        guard link.usable else { return }
        let g = geo
        let f = DateFormatter()
        f.dateFormat = "HH:mm"
        let time = f.string(from: Date())
        let mail = unread.map { "✉ \($0)" } ?? ""
        let unreadNow = unread ?? 0
        put("bar", CGRect(x: 0, y: 0, width: g.w, height: Self.barH), Self.paint(g.w, Self.barH) { _ in
            Self.panel.setFill()
            UIRectFill(CGRect(x: 0, y: 0, width: g.w, height: Self.barH - 2))
            Self.text("⟳", in: CGRect(x: 0, y: 0, width: 34, height: 26), size: 17, color: Self.grey)
            Self.text(time, in: CGRect(x: Self.linkEnd + 4, y: 0, width: 60, height: 26), size: 15, bold: true, align: .left)
            Self.text(mail, in: CGRect(x: g.w - 70, y: 0, width: 62, height: 26), size: 13, bold: true,
                      color: unreadNow > 0 ? Self.amber : Self.green, align: .right)
        })
    }

    private func drawFace() {
        guard link.usable, answer == nil, sentFace != face else { return }
        sentFace = face
        let name = face
        link.enqueue { [link] in _ = await link.command("FACE \(name)") }
    }

    private func drawInfo() {
        guard link.usable, answer == nil else { return }
        let r = geo.info
        let line: String
        let color: UIColor
        if approval != nil {
            (line, color) = ("Approve?  ·  موافق؟", Self.amber)
        } else if !status.isEmpty {
            (line, color) = (status, Self.ink)
        } else if speaking {
            (line, color) = ("Speaking…", Self.ink)
        } else {
            (line, color) = ("Ready  ·  جاهز", Self.grey)
        }
        let detail = approval?.title
        put("info", r, Self.paint(Int(r.width), Int(r.height)) { box in
            if let detail, box.height > 50 {
                Self.text(line, in: CGRect(x: 4, y: 2, width: box.width - 8, height: box.height / 2), size: 17, bold: true, color: color)
                Self.text(detail, in: CGRect(x: 4, y: box.height / 2, width: box.width - 8, height: box.height / 2 - 2), size: 14, color: Self.ink)
            } else {
                Self.text(line, in: box.insetBy(dx: 4, dy: 0), size: box.height > 50 ? 19 : 15, bold: true, color: color)
            }
        })
    }

    private var buttons: [Button] {
        if approval != nil {
            return [Button(label: "Yes ✓", color: Self.green, action: "yes"), Button(label: "No ✕", color: Self.red, action: "no")]
        }
        if listening {
            return [Button(label: "Send", color: Self.green, action: "talk"), Button(label: "Cancel", color: Self.red, action: "cancel")]
        }
        if busy || speaking { return [Button(label: "Stop", color: Self.red, action: "cancel")] }
        if answer != nil {
            let more = answerPage + 1 < answerPages ? [Button(label: "More ▸", color: Self.blue, action: "more")] : []
            return more + [Button(label: "Home", color: Self.key, action: "home")]
        }
        return [Button(label: "Talk", color: Self.blue, action: "talk"), Button(label: "Brief", color: Self.key, action: "brief"),
                Button(label: "Mail", color: Self.key, action: "mail"), Button(label: "Meet", color: Self.key, action: "meet")]
    }

    /// Where each button sits inside the button area.
    private func rects(_ count: Int) -> [CGRect] {
        let area = geo.buttons
        let gap: CGFloat = 4
        return (0..<count).map { i in
            if geo.land {
                let h = (area.height - gap * CGFloat(count + 1)) / CGFloat(count)
                return CGRect(x: gap, y: gap + CGFloat(i) * (h + gap), width: area.width - gap * 2, height: h)
            }
            let w = (area.width - gap * CGFloat(count + 1)) / CGFloat(count)
            return CGRect(x: gap + CGFloat(i) * (w + gap), y: gap, width: w, height: area.height - gap * 2)
        }
    }

    private func drawButtons(pressed: Int? = nil) {
        guard link.usable else { return }
        let area = geo.buttons
        let list = buttons
        let boxes = rects(list.count)
        put("buttons", area, Self.paint(Int(area.width), Int(area.height)) { _ in
            for (i, b) in list.enumerated() {
                (i == pressed ? Self.amber : b.color).setFill()
                UIBezierPath(roundedRect: boxes[i], cornerRadius: 9).fill()
                Self.text(b.label, in: boxes[i], size: list.count > 3 && !self.geo.land ? 15 : 17, bold: true,
                          color: i == pressed ? .black : .white)
            }
        })
    }

    // ── The answer page (covers the face) ────────────────────────────────────

    private func layout(_ textValue: String, in size: CGSize) -> (NSTextStorage, NSLayoutManager, [NSTextContainer]) {
        let style = NSMutableParagraphStyle()
        style.alignment = Voice.isArabic(textValue) ? .right : .left
        style.baseWritingDirection = Voice.isArabic(textValue) ? .rightToLeft : .leftToRight
        style.lineSpacing = 2
        let storage = NSTextStorage(string: textValue, attributes: [
            .font: UIFont.systemFont(ofSize: 16, weight: .medium), .foregroundColor: Self.ink, .paragraphStyle: style,
        ])
        let manager = NSLayoutManager()
        storage.addLayoutManager(manager)
        var boxes: [NSTextContainer] = []
        repeat {
            let box = NSTextContainer(size: size)
            box.lineFragmentPadding = 0
            manager.addTextContainer(box)
            boxes.append(box)
        } while manager.glyphRange(for: boxes.last!).upperBound < manager.numberOfGlyphs && boxes.count < 12
        return (storage, manager, boxes)  // the caller keeps the text alive while it draws
    }

    private func drawAnswer() {
        guard link.usable, let answer else { return }
        let r = geo.page
        let inner = CGSize(width: r.width - 16, height: r.height - 12)
        let (storage, manager, boxes) = layout(Voice.speakable(answer), in: inner)
        defer { withExtendedLifetime(storage) {} }
        answerPages = boxes.count
        answerPage = min(answerPage, boxes.count - 1)
        let range = manager.glyphRange(for: boxes[answerPage])
        let page = answerPage, pages = answerPages
        put("page", r, Self.paint(Int(r.width), Int(r.height)) { box in
            manager.drawGlyphs(forGlyphRange: range, at: CGPoint(x: 8, y: 4))
            if pages > 1 {
                Self.text("\(page + 1)/\(pages)", in: CGRect(x: box.width - 44, y: box.height - 16, width: 40, height: 14),
                          size: 11, color: Self.grey, align: .right)
            }
        })
    }

    /// Shows an answer on the display until Home is pressed.
    func show(answer text: String) {
        answer = text
        answerPage = 0
        link.enqueue { [link] in _ = await link.command("POSES off") }
        refresh()
    }

    private func home() {
        answer = nil
        sent["page"] = nil
        sent["info"] = nil
        sentFace = ""
        refresh()
    }

    // ── Keeping the display in step with the app ─────────────────────────────

    /// Call after any change: only the parts that look different are sent.
    func refresh() {
        guard link.usable else { return }
        drawBar()
        if answer != nil, !(busy || listening || approval != nil) {
            drawAnswer()
        } else {
            if answer != nil { answer = nil; sent["page"] = nil; sent["info"] = nil; sentFace = "" }
            drawFace()
            drawInfo()
        }
        drawButtons()
    }

    private func redrawAll() {
        sent.removeAll()
        sentFace = ""
        link.enqueue { [link] in _ = await link.command("POSES off") }
        refresh()
    }

    private func touch(_ x: Int, _ y: Int) {
        guard let actions else { return }
        if y < Self.barH {
            if x < 34 {
                sent.removeAll()
                sentFace = ""
                link.enqueue { [weak self] in
                    guard let self else { return }
                    if await self.link.command("ROT +") { self.redrawAll() }
                }
            }
            return
        }
        let area = geo.buttons
        // A little slack around the buttons: fingers are not styluses.
        guard area.insetBy(dx: -6, dy: -6).contains(CGPoint(x: x, y: y)) else { return }
        let list = buttons
        let local = CGPoint(x: CGFloat(x) - area.minX, y: CGFloat(y) - area.minY)
        let boxes = rects(list.count)
        guard let i = boxes.firstIndex(where: { $0.insetBy(dx: -3, dy: -6).contains(local) }) else { return }
        drawButtons(pressed: i)
        let action = list[i].action
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(180))
            switch action {
            case "talk": actions.talk()
            case "cancel": actions.cancel()
            case "yes": actions.approve(true)
            case "no": actions.approve(false)
            case "brief": actions.ask("Give me my brief: today's meetings, the important unread mail, and anything I should act on.")
            case "mail": actions.ask("What are the important mails from today?")
            case "meet": actions.ask("What meetings do I have today and tomorrow?")
            case "more": self.answerPage += 1
            case "home": self.home()
            default: break
            }
            self.sent["buttons"] = nil
            self.refresh()
        }
    }
}
