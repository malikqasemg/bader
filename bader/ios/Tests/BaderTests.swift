import XCTest
@testable import Bader

final class BaderTests: XCTestCase {
    func testEntryIdMatchesTheComputer() {
        // python: hashlib.sha1(b"1759480000|what is on my calendar").hexdigest()[:16]
        XCTAssertEqual(MemoryLogic.id(ts: 1_759_480_000, ask: "what is on my calendar"), "e4d9f6181d12a18e")
    }

    func testMergeKeepsFirstSeenAndSortsByTime() {
        let a = [Entry(id: "1", ts: 1, device: "pc"), Entry(id: "2", ts: 5, device: "pc")]
        let b = [Entry(id: "2", ts: 5, device: "phone"), Entry(id: "3", ts: 3, device: "phone")]
        let merged = MemoryLogic.merge([a, b])
        XCTAssertEqual(merged.map(\.id), ["1", "3", "2"])
        XCTAssertEqual(merged[2].device, "pc")
        XCTAssertEqual(MemoryLogic.merge([a, b], keep: 2).map(\.id), ["3", "2"])
    }

    func testMemoryTextRoundTripWithArabic() {
        let e = Entry(id: "abc", ts: 7, device: "phone", channel: "phone", kind: "ask", ask: "ما هي اجتماعاتي؟", answer: "لا شيء اليوم / nothing")
        let text = MemoryLogic.text([e])
        XCTAssertTrue(text.contains("ما هي اجتماعاتي؟"))
        XCTAssertEqual(MemoryLogic.parse(text), [e])
        // A line written by the computer, plus a broken line that must be skipped.
        let pc = "{\"id\": \"x1\", \"ts\": 9, \"device\": \"pc\", \"channel\": \"island\", \"kind\": \"ask\", \"ask\": \"hi\", \"answer\": \"hello\"}\nnot json\n"
        XCTAssertEqual(MemoryLogic.parse(pc).map(\.id), ["x1"])
    }

    func testPairingCode() {
        XCTAssertNil(Pairing.parse("https://example.com"))
        XCTAssertNil(Pairing.parse("{\"bader\":1,\"ai\":\"\"}"))
        let p = Pairing.parse("{\"bader\":1,\"ai\":\"k\",\"l1\":\"en\",\"l2\":\"ar\",\"g\":{\"id\":\"i\",\"s\":\"s\",\"r\":\"r\",\"a\":\"me@example.com\"}}")
        XCTAssertEqual(p?.g?.r, "r")
        XCTAssertEqual(Pairing.parse("{\"bader\":1,\"ai\":\"k\"}")?.g, nil)
    }

    func testFeed() {
        let rss = "<rss><channel><title>Top</title><item><title>First &amp; best</title><source url=\"x\">Paper</source><pubDate>Sat, 03 Oct 2026</pubDate></item><item><title>Second</title></item></channel></rss>"
        let items = FeedParser.parse(Data(rss.utf8))
        XCTAssertEqual(items.map(\.title), ["First & best", "Second"])
        XCTAssertEqual(items[0].source, "Paper")
    }

    func testSpeechHelpers() {
        XCTAssertTrue(Voice.isArabic("لديك ثلاثة اجتماعات اليوم"))
        XCTAssertFalse(Voice.isArabic("You have 3 meetings with أحمد"))
        XCTAssertEqual(Voice.speakable("**Two** things:\n- one\n- two https://x.y/z"), "Two things:\none\ntwo")
    }

    func testPictureEncodingMatchesTheDisplay() {
        // 5 equal pixels, then 3 different ones: one run + one literal block.
        var px = Data()
        for _ in 0..<5 { px.append(contentsOf: [0x12, 0x34]) }
        px.append(contentsOf: [0, 1, 0, 2, 0, 3])
        XCTAssertEqual([UInt8](FaceLink.rle16(px)), [131, 0x12, 0x34, 2, 0, 1, 0, 2, 0, 3])
        // Red, drawn with UIKit, comes out as RGB565 high byte first.
        let red = FaceScreen.paint(2, 1) { box in UIColor.red.setFill(); UIRectFill(box) }
        XCTAssertEqual([UInt8](red), [0xF8, 0x00, 0xF8, 0x00])
    }

    func testBase64URLAndHTML() {
        let data = Data([0xfb, 0xff, 0xfe])
        XCTAssertEqual(data.base64URL, "-__-")
        XCTAssertEqual(Data(base64URL: "-__-"), data)
        XCTAssertEqual("<p>Hello&nbsp;<b>there</b></p><style>x{}</style>".strippingHTML, "Hello there")
    }
}
