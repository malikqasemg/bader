# Bader — Handoff (2026-10-03, 12:50 Riyadh)

Paste the block below as the first message of a new session.

---

Continue the Bader project. Read these first, then carry on without asking me to repeat things:
`/Users/malekqasem/Bader/docs/bader/06-HANDOFF.md`, `01-PRD.md`, `05-IMPLEMENTATION-PLAN.md`.

**What Bader is:** my licensed personal assistant for a CEO. Arabic + English. Repo `/Users/malekqasem/Bader`
(GitHub `malikqasemg/bader`, PUBLIC). Faces: computer app (Tauri, Mac first, then Windows), iPhone app (SwiftUI),
Telegram bot, 2.8" ESP32 touch display. All share one memory file in my own Google Drive.

**Where things are**
- Engine: Hermes profile `~/.hermes/profiles/bader`, API 127.0.0.1:8642, model `anthropic/claude-haiku-4.5` via OpenRouter.
  Start: `HERMES_HOME=~/.hermes/profiles/bader hermes -p bader gateway run --replace`.
- Computer app: `bader/shell`. Build: `cd bader/shell && NODE_ENV=development npx tauri build --bundles app,dmg`,
  then `scripts/sign-mac.sh`, then `open target/release/bundle/macos/Bader.app`. Background long commands and poll
  (Desktop Commander has a 60 s limit). Log: `~/Library/Application Support/Bader/bader.log`.
- iPhone app: `bader/ios`. `xcodegen generate`; tests:
  `xcodebuild test -project Bader.xcodeproj -scheme Bader -destination 'platform=iOS Simulator,name=iPhone 17 Pro' -derivedDataPath build CODE_SIGNING_ALLOWED=NO`.
  Device build + install (test iPhone 12, id `28D9FA48-E6B2-5E48-99E3-9118CA68AC31`, team `VXUUYYRN92`, free team = 7-day expiry):
  `xcodebuild build -project Bader.xcodeproj -scheme Bader -destination 'id=<id>' -derivedDataPath build -allowProvisioningUpdates -allowProvisioningDeviceRegistration`
  then `xcrun devicectl device install app --device <id> build/Build/Products/Debug-iphoneos/Bader.app`
  and `xcrun devicectl device process launch --terminate-existing --device <id> com.malikqasem.bader`.
- Display: `bader/face/device_e28` (MicroPython, firmware 3.1, port `/dev/cu.usbserial-8310`).
  Update: quit the Bader app first (it holds the port), then `python3 push.py /dev/cu.usbserial-8310 main.py`.
  `ID` over serial reports Bluetooth state, owner (usb/ble), free memory.

**State right now**
- Built and working on the Mac: computer app, voice, hotkey (Control+Option), character, Telegram, display over USB,
  shared memory sync (107 entries), pairing QR in Settings.
- iPhone: app installed and paired. Natural cloud voice was added in the last build (commit `edb7650e`);
  I have not yet said whether it sounds good.
- NOT tested yet: the display over Bluetooth with the phone, the PC / PHONE button, the display's new top bar layout,
  approvals on the phone, camera "look at this", Arabic on the phone, the pairing QR on a second device.
- NOT built: all-in-one installer (size tier not picked), display over Bluetooth from the computer, morning brief
  notification on the phone, Outlook, Webex, WhatsApp, licence, code signing, Windows check of recent work,
  portable display board with mic/speaker/camera.

**Next steps, in order**
1. Ask me how the phone voice sounds and what I saw on the display after tapping PC / PHONE; fix what is wrong.
2. All-in-one installer for Mac and Windows.
3. Windows build check.
4. Portable display (ESP32-S3 board with mic, speaker, camera, battery) talking to the phone.

**Rules**
- Short, simple answers: what was done / did it work / next step. At most 2 options plus your pick. Do the work
  yourself; do not ask me to run things you can run. No clarifying questions when the ask is clear.
- Never type my passwords or keys. Never send messages or mail as me without my yes.
- The repo is public: no keys, no mail content in commits. Stress-test results stay in `~/Bader-stress-results/`.
- My Google client secret was pasted in a chat earlier: remind me to rotate it before any customer use.
- Do not stop SpecOps backend (8001) or frontend (3001) without asking.
- Keep `docs/bader/` updated. Use jev_decide at decision points. Save the session to my second-brain vault at the end.

---
