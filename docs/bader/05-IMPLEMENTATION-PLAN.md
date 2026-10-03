# Bader — Implementation Plan

Durations are estimates.

## Phase 0 — Fork and docs (done 2026-10-01)
- [x] Fork NousResearch/hermes-agent → malikqasemg/bader, clone to ~/Bader
- [x] Docs 00–05 in `docs/bader/`

## Phase 1 — Rebrand (week 1)
- [x] Desktop shell: coucou Windows Tauri app ported into `bader/shell/`; Bader art; chat → Hermes
      API server (127.0.0.1:8642); licence + engine keys in settings (2026-10-01)
- [x] CI: `.github/workflows/bader-shell-windows.yml` builds `Bader-Windows-<v>-setup.exe` (2.1 MB)
- [ ] Bundle Hermes engine headless inside the installer (runs as background process)
- [ ] CLI/command name `bader` alongside `hermes` (keep `hermes` internally for upstream merges)
- [x] `SOUL.md` → Bader persona, Arabic + English
- [ ] Arabic strings: review `locales/ar.yaml`, add Bader-specific keys
- [x] Build Windows installer from `bader/shell` (Tauri NSIS setup.exe) — v0.1.0
- [x] macOS build of the shell (DMG) for testing
- [x] Settings: AI (provider, key, model) and Voice (Arabic + English STT, voice, spoken replies) → engine profile
- [x] Connector pills: Outlook, Webex, Gmail
- [x] Island mic: talk in Arabic/English (engine STT, auto language), spoken replies (engine TTS)
- [x] Look: attach a screenshot of the screen to a question (macOS)

## Phase 3d — Speed, skills, language, approvals (done 2026-10-01)
- [x] Background sync (5 min) of Gmail + Calendar → bader_inbox.json; mail/calendar questions answered from it (~40 s → ~7–12 s)
- [x] Chat via engine runs stream: live tool progress, approvals, final answer
- [x] Skills: bader-meeting-brief (+ transcribe), docx/xlsx/powerpoint/pdf, web search (ddgs), browser, computer use, Agent-Reach
- [x] Language: answers and meeting summaries Auto / English only / Arabic only
- [x] Approvals plugin (bader-approvals): send/reply/delete mail, calendar changes → Approve/Deny in island, screen button, Telegram
- [x] Screen v2: poses rotate when idle, next meeting + unread, live tool text, approval screen + amber light

## Phase 3c — Live face screen (done 2026-10-01)
- [x] ESP32-C6-LCD-1.47 runs MicroPython + `bader/face/device`; 8 faces with English/Arabic labels
- [x] App auto-finds the screen on USB and shows listening / thinking / speaking / done / problem
- [ ] "New message" (surprised) when background mail sync finds important mail

## Phase 3b — Screen buddy (ideas from Clicky, MIT: farzaa/clicky, openclicky, clicky_windows)
- [x] Push-to-talk hotkey: Control+Option (Mac) / Ctrl+Alt (Windows)
- [x] Character on the desktop: drag, double-click, right-click quick actions
- [ ] Point at things: cursor overlay flies to the button Bader talks about
- [ ] Proactive suggestions: notice repeated work and offer to do it (ask first)
- [ ] Look on Windows (screen capture)

- [ ] Settings + wizard in Arabic (RTL); Bader tray icon (head) instead of app icon

## Phase 3e — Bader on iPhone + display switch (started 2026-10-03)
- [x] Shared memory file in Drive, two-way sync on the computer (`bader_sync.py`).
- [x] Pairing QR in Settings (`pair.rs`, `iphoneSection`).
- [x] iPhone app `bader/ios` (SwiftUI, xcodegen): chat, voice, camera, mail, calendar, news, weather, approvals, memory.
- [x] Display firmware 3.1: Bluetooth + PC / PHONE button; computer side understands AWAY.
- [x] Phone draws the display over Bluetooth (`FaceLink`, `FaceScreen`).
- [x] Installed on the test iPhone (free Apple team: expires after 7 days); natural cloud voice added.
- [ ] Real-use test on the iPhone: voice quality, brief, approvals, display over Bluetooth.
- [ ] Display on Bluetooth from the computer (no cable) — not built.
- [ ] Morning brief notification on the phone; Outlook on the phone; App Store / TestFlight (needs paid Apple developer account).

## Phase 3f — done between 2026-10-01 and 2026-10-03
- [x] Touch display 2.8" (ESP32-32E): faces, status, buttons, lists, answer pages, upright/sideways, calibration
- [x] Telegram channel; engine names hidden from the user
- [x] Memory of earlier asks (history in the snapshot + journal + recall skill)
- [x] News (Google News feed), weather (Open-Meteo), 7-day mail window
- [x] OpenRouter key does voice too; speech model `large-v3-turbo`, Arabic/English only
- [x] No keychain prompts on Mac (file store); first-run wizard

## Next up (in this order)
1. Real-use test of the iPhone app and the display's PC / PHONE switch; fix what the test finds.
2. All-in-one installer for Mac and Windows (size tier still to pick: about 0.8 / 1.05 / 2 GB download).
3. Confirm the Windows build of recent work (character, hotkey, display, secrets, pairing).
4. Portable display: board with mic, speaker, camera, battery (ESP32-S3 class), talking to the phone.
5. Outlook + Webex connectors; licence client and server; code signing.
## Phase 2 — Licence (week 2)
- [ ] Licence server: add `product` column + `/bader/*` endpoints to iNetBuzz `mcp-server`
- [ ] `bader/license/` client: activate, refresh every 24 h, status, usage
- [ ] Licensed skill loader: fetch bundle into memory, unload on expiry
- [ ] Wizard step "Enter licence key" in Desktop
- [ ] Admin console: Bader product + licences tab; Stripe link for Bader

## Phase 3 — Skills (weeks 3–5), served from VPS
- [ ] `bader-outlook`: Graph mail + calendar (reuse `gateway/platforms/msgraph_webhook.py`)
- [ ] `bader-webex`: meetings, transcripts, summaries, messages (new MCP server)
- [ ] `bader-briefs`: morning / evening brief, meeting follow-up, action items
- [ ] `bader-core`: approval rule, audit, Arabic/English behaviour
- [ ] `bader-gmail`: Gmail read/summarise/draft (reuse engine google-workspace skill)
- [ ] Channels: enable Telegram + WhatsApp Cloud adapter in the wizard

## Phase 4 — Pilot (weeks 6–8)
- [ ] Install at the first customer CEO; measure metrics in `01-PRD.md`
- [ ] Code-sign Windows installer [certificate needed]
- [ ] Auto-update channel from GitHub releases

## Blockers / inputs needed
- Microsoft Entra app registration (client ID) for Outlook sign-in
- Google Cloud OAuth client (google_client_secret.json) for Gmail sign-in
- Bader logo and colours
- Microsoft Entra app registration + Webex integration (iNetGenius-owned, multi-tenant)
- Windows code-signing certificate
- Bader price (Stripe)
