import CoreBluetooth
import Foundation

// The "Nordic UART" service the display offers: one channel in, one out.
private let faceService = CBUUID(string: "6E400001-B5A3-F393-E0A9-E50E24DCCA9E")
private let faceToBoard = CBUUID(string: "6E400002-B5A3-F393-E0A9-E50E24DCCA9E")
private let faceFromBoard = CBUUID(string: "6E400003-B5A3-F393-E0A9-E50E24DCCA9E")

/// The Bluetooth line to Bader's small display. It speaks the same lines as the
/// USB cable does with the computer (see bader/face/device_e28/main.py).
@MainActor
final class FaceLink: NSObject, ObservableObject {
    enum Status: Equatable {
        case off, searching, connected, away
    }

    @Published private(set) var status: Status = .off
    /// Width and height of the display's picture, once it has said hello.
    @Published private(set) var size: (w: Int, h: Int)?

    var onReady: (() -> Void)?
    var onTouch: ((Int, Int) -> Void)?
    var onGone: (() -> Void)?

    nonisolated static let maxIn = 4000
    nonisolated static let maxOut = 240 * 16 * 2

    private var central: CBCentralManager?
    private var board: CBPeripheral?
    private var write: CBCharacteristic?
    private var inbox = Data()
    private var waiter: CheckedContinuation<Bool, Never>?
    private var waiterID = 0
    private var last: Task<Void, Never> = Task {}

    var usable: Bool { status == .connected && size != nil }

    func start() {
        guard central == nil else { return }
        status = .searching
        central = CBCentralManager(delegate: self, queue: .main)
    }

    func stop() {
        if let board { central?.cancelPeripheralConnection(board) }
        central?.stopScan()
        central = nil
        board = nil
        write = nil
        size = nil
        status = .off
        resolve(false)
    }

    // ── Sending ──────────────────────────────────────────────────────────────

    /// Jobs run one after another, so commands never interleave.
    func enqueue(_ job: @escaping @MainActor () async -> Void) {
        let previous = last
        last = Task { @MainActor in
            await previous.value
            await job()
        }
    }

    private func resolve(_ ok: Bool) {
        waiter?.resume(returning: ok)
        waiter = nil
    }

    /// Sends one command (and its picture bytes) and waits for the display's OK.
    @discardableResult
    func command(_ head: String, payload: Data? = nil) async -> Bool {
        guard let board, let write, board.state == .connected else { return false }
        var data = Data((head + "\n").utf8)
        if let payload { data.append(payload) }
        let step = max(20, board.maximumWriteValueLength(for: .withoutResponse))
        var at = 0
        while at < data.count {
            var waited = 0
            while !board.canSendWriteWithoutResponse {
                try? await Task.sleep(for: .milliseconds(3))
                waited += 3
                if board.state != .connected || waited > 3000 { return false }
            }
            let end = min(at + step, data.count)
            board.writeValue(data.subdata(in: at..<end), for: write, type: .withoutResponse)
            at = end
        }
        waiterID += 1
        let mine = waiterID
        return await withCheckedContinuation { done in
            waiter = done
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(4))
                if self.waiterID == mine, self.waiter != nil { self.resolve(false) }
            }
        }
    }

    /// Draws a picture (RGB565, high byte first) at a place on the display, in bands the display can take.
    @discardableResult
    func image(x: Int, y: Int, w: Int, h: Int, pixels: Data) async -> Bool {
        guard usable, pixels.count == w * h * 2 else { return false }
        let row = w * 2
        let maxRows = max(1, Self.maxOut / row)
        var top = 0
        var allOK = true
        while top < h {
            var rows = min(maxRows, h - top)
            var packed = Self.rle16(pixels.subdata(in: top * row..<(top + rows) * row))
            while packed.count > Self.maxIn, rows > 1 {
                rows /= 2
                packed = Self.rle16(pixels.subdata(in: top * row..<(top + rows) * row))
            }
            let head = "IMG \(x) \(y + top) \(w) \(rows) \(packed.count)"
            var ok = await command(head, payload: packed)
            if !ok, usable { ok = await command(head, payload: packed) }
            if !usable { return false }
            allOK = allOK && ok
            top += rows
        }
        return allOK
    }

    /// PackBits on 16-bit pixels: c < 128 → c+1 literal pixels; c ≥ 128 → the next pixel × (c-126).
    nonisolated static func rle16(_ px: Data) -> Data {
        let b = [UInt8](px)
        let n = b.count / 2
        var out = Data(capacity: b.count / 4)
        func same(_ i: Int, _ j: Int) -> Bool { b[i * 2] == b[j * 2] && b[i * 2 + 1] == b[j * 2 + 1] }
        var i = 0
        while i < n {
            var run = 1
            while i + run < n, run < 129, same(i + run, i) { run += 1 }
            if run >= 2 {
                out.append(UInt8(run + 126))
                out.append(b[i * 2])
                out.append(b[i * 2 + 1])
                i += run
                continue
            }
            let start = i
            i += 1
            while i < n, i - start < 128, !(i + 1 < n && same(i + 1, i)) { i += 1 }
            out.append(UInt8(i - start - 1))
            out.append(contentsOf: b[start * 2..<i * 2])
        }
        return out
    }

    // ── Receiving ────────────────────────────────────────────────────────────

    private func line(_ l: String) {
        if l == "OK" {
            resolve(true)
        } else if l.hasPrefix("PONG") {
            let parts = l.split(separator: " ")
            if parts.count >= 5, let w = Int(parts[3]), let h = Int(parts[4]) { size = (w, h) }
            resolve(true)
        } else if l == "AWAY" || l == "ERR away" {
            status = .away
            if l != "AWAY" { resolve(false) }
        } else if l.hasPrefix("ERR") {
            resolve(false)
        } else if l == "READY" {
            status = .connected
            hello()
        } else if l.hasPrefix("TOUCH ") {
            let parts = l.split(separator: " ")
            if parts.count >= 3, let x = Int(parts[1]), let y = Int(parts[2]) { onTouch?(x, y) }
        }
    }

    /// Asks the display for its size, then lets the app draw.
    private func hello() {
        enqueue { [weak self] in
            guard let self else { return }
            if await self.command("PING"), self.status == .connected { self.onReady?() }
        }
    }

    fileprivate func received(_ data: Data) {
        inbox.append(data)
        while let nl = inbox.firstIndex(of: 0x0A) {
            let text = String(decoding: inbox[inbox.startIndex..<nl], as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
            inbox.removeSubrange(inbox.startIndex...nl)
            if !text.isEmpty { line(text) }
        }
        if inbox.count > 400 { inbox.removeAll() }
    }

    fileprivate func found(_ peripheral: CBPeripheral) {
        guard board == nil else { return }
        board = peripheral
        peripheral.delegate = self
        central?.stopScan()
        central?.connect(peripheral)
    }

    fileprivate func writable(_ characteristic: CBCharacteristic?) {
        write = characteristic
    }

    /// The display's answers now reach us: say hello.
    fileprivate func linked() {
        guard write != nil else { return }
        inbox.removeAll()
        status = .connected
        hello()
    }

    fileprivate func lost() {
        write = nil
        size = nil
        resolve(false)
        onGone?()
        guard let central, let board else { return }
        status = .searching
        central.connect(board)  // waits, however long, until the display is back
    }

    fileprivate func powered(_ on: Bool) {
        guard let central else { return }
        if on {
            status = .searching
            central.scanForPeripherals(withServices: [faceService])
        } else {
            board = nil
            write = nil
            size = nil
            status = .searching
        }
    }
}

// CoreBluetooth calls these on the main queue (asked for in start()).
extension FaceLink: CBCentralManagerDelegate, CBPeripheralDelegate {
    nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
        let on = central.state == .poweredOn
        MainActor.assumeIsolated { powered(on) }
    }

    nonisolated func centralManager(_ central: CBCentralManager, didDiscover peripheral: CBPeripheral,
                                    advertisementData: [String: Any], rssi RSSI: NSNumber) {
        MainActor.assumeIsolated { found(peripheral) }
    }

    nonisolated func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        peripheral.discoverServices([faceService])
    }

    nonisolated func centralManager(_ central: CBCentralManager, didFailToConnect peripheral: CBPeripheral, error: Error?) {
        MainActor.assumeIsolated { lost() }
    }

    nonisolated func centralManager(_ central: CBCentralManager, didDisconnectPeripheral peripheral: CBPeripheral, error: Error?) {
        MainActor.assumeIsolated { lost() }
    }

    nonisolated func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        for s in peripheral.services ?? [] where s.uuid == faceService {
            peripheral.discoverCharacteristics(nil, for: s)
        }
    }

    nonisolated func peripheral(_ peripheral: CBPeripheral, didDiscoverCharacteristicsFor service: CBService, error: Error?) {
        var out: CBCharacteristic?
        for c in service.characteristics ?? [] {
            if c.uuid == faceFromBoard { peripheral.setNotifyValue(true, for: c) }
            if c.uuid == faceToBoard { out = c }
        }
        MainActor.assumeIsolated { writable(out) }
    }

    nonisolated func peripheral(_ peripheral: CBPeripheral, didUpdateNotificationStateFor characteristic: CBCharacteristic, error: Error?) {
        guard characteristic.isNotifying else { return }
        MainActor.assumeIsolated { linked() }
    }

    nonisolated func peripheral(_ peripheral: CBPeripheral, didUpdateValueFor characteristic: CBCharacteristic, error: Error?) {
        guard let data = characteristic.value else { return }
        MainActor.assumeIsolated { received(data) }
    }
}
