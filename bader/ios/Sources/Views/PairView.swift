import SwiftUI
import VisionKit

/// First screen: point the camera at the code shown by Bader on the computer.
struct PairView: View {
    @EnvironmentObject var app: AppState
    @State private var scanning = false
    @State private var problem: String?

    var body: some View {
        VStack(spacing: 18) {
            Spacer()
            Theme.face("pose_waving").resizable().scaledToFit().frame(height: 190)
            Text("Hello, I'm Bader").font(.largeTitle.bold())
            Text("مرحباً، أنا بدر").font(.title2).foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 10) {
                step(1, "On your computer, open Bader's Settings.")
                step(2, "Under “Bader on iPhone”, press “Show pairing code”.")
                step(3, "Press the button below and point the phone at the code.")
            }
            .padding()
            .background(Theme.card, in: RoundedRectangle(cornerRadius: 16))
            if let problem {
                Text(problem).font(.footnote).foregroundStyle(.orange).multilineTextAlignment(.center)
            }
            Spacer()
            Button {
                problem = nil
                if DataScannerViewController.isSupported {
                    scanning = true
                } else {
                    problem = "This iPhone cannot scan codes."
                }
            } label: {
                Label("Scan the pairing code", systemImage: "qrcode.viewfinder")
                    .font(.headline).frame(maxWidth: .infinity).padding(.vertical, 14)
            }
            .buttonStyle(.borderedProminent)
            .tint(Theme.accent)
        }
        .padding(24)
        .background(Theme.back.ignoresSafeArea())
        .fullScreenCover(isPresented: $scanning) {
            ZStack(alignment: .top) {
                Scanner { text in
                    if app.pair(with: text) {
                        scanning = false
                    } else {
                        problem = "That is not a Bader pairing code."
                        scanning = false
                    }
                }
                .ignoresSafeArea()
                HStack {
                    Text("Point at the code on your computer").font(.headline)
                    Spacer()
                    Button("Cancel") { scanning = false }
                }
                .padding()
                .background(.ultraThinMaterial)
            }
        }
    }

    private func step(_ n: Int, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Text("\(n)").font(.subheadline.bold()).frame(width: 24, height: 24)
                .background(Theme.accent, in: Circle()).foregroundStyle(.white)
            Text(text).font(.subheadline)
        }
    }
}

/// The camera, looking for a QR code. Reports the first one it reads.
struct Scanner: UIViewControllerRepresentable {
    let found: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(found: found) }

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])],
                                                qualityLevel: .accurate, isHighlightingEnabled: true)
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {
        if !scanner.isScanning { try? scanner.startScanning() }
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let found: (String) -> Void
        private var done = false
        init(found: @escaping (String) -> Void) { self.found = found }

        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems: [RecognizedItem]) {
            guard !done else { return }
            for item in addedItems {
                if case .barcode(let code) = item, let text = code.payloadStringValue {
                    done = true
                    dataScanner.stopScanning()
                    found(text)
                    return
                }
            }
        }
    }
}

enum Theme {
    /// One of Bader's faces (plain picture files in the app).
    static func face(_ name: String) -> Image {
        Image(uiImage: UIImage(named: name) ?? UIImage(named: "idle") ?? UIImage())
    }

    static let back = Color(red: 0.05, green: 0.06, blue: 0.09)
    static let card = Color(red: 0.10, green: 0.12, blue: 0.16)
    static let accent = Color(red: 0.25, green: 0.56, blue: 0.96)
    static let mine = Color(red: 0.16, green: 0.36, blue: 0.68)
}
