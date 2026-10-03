# 1. Bader — Product Requirements Document (PRD)

Last updated: 2026-10-03. Status words used here: **built** (works on the developer's Mac),
**partly** (some of it works), **planned** (not started).

## 2. Overview
Bader (بدر) is a personal AI assistant for a busy executive. It reads mail and calendar,
answers by voice or text in Arabic and English, briefs the user, and acts only after the
user approves anything that cannot be undone.

Bader has four faces, all sharing one memory:

| Face | What it is | Status |
|---|---|---|
| Computer app | Window at the top of the screen, menu-bar icon, a character on the desktop. Mac first, then Windows. | built (Mac); Windows build not re-checked for recent work |
| iPhone app | A Bader of its own. Works when the computer is off. | built, on the test phone; real-use testing just started |
| Telegram | Chat with Bader from any phone through a private bot. | built |
| Small display | 2.8" touch screen (ESP32) showing Bader's face, status and buttons. USB to the computer, Bluetooth to the phone. | built; Bluetooth side not yet tested with the phone |

Business model: the app is free to install; skills are paid per licence key served from the
iNetGenius licence server (same model as iNetBuzz). Licence part is **planned**.

## 3. Goals & Objectives
1. Install with one file. The user downloads no other tool. (**planned**: all-in-one installer)
2. Daily value: brief, mail, meetings, news, by voice, in under a few seconds for simple asks.
3. Safe by default: sending, deleting and calendar changes always ask first.
4. One Bader everywhere: the same history on computer, phone and Telegram.
5. Arabic and English as equals: understand both, answer in the language used.
6. Works away from the desk: phone app and a carry-anywhere display.
7. Licensed: skills locked without a valid key.

Out of scope for now: Linux, Android, multi-user admin console, languages other than Arabic/English.

## 4. Requirements
| ID | Requirement | Priority | Status |
|---|---|---|---|
| R1 | Mac app (DMG) and Windows app (setup.exe) | Must | Mac built; Windows partly |
| R2 | All-in-one installer: engine, Python, speech model inside one file | Must | planned |
| R3 | First-run wizard: main + second language, AI key, voice choice | Must | built |
| R4 | Gmail + Google Calendar: read, summarise, draft, send, schedule | Must | built |
| R5 | Outlook mail + calendar (Microsoft Graph) | Must | planned (needs Entra app) |
| R6 | Webex meetings, transcripts, messages | Must | planned |
| R7 | Voice in (Arabic/English) and voice out, on device or cloud | Must | built |
| R8 | Push-to-talk key: Control+Option (Mac), Ctrl+Alt (Windows) | Must | built (Mac); Windows not checked |
| R9 | Approvals for anything that cannot be undone | Must | built |
| R10 | Memory of earlier asks across all faces | Must | built |
| R11 | Telegram channel; WhatsApp later | Must | Telegram built; WhatsApp planned |
| R12 | News, web search, weather | Must | built |
| R13 | Small display: face, status, touch buttons, upright or sideways | Should | built |
| R14 | iPhone app: standalone, shared memory, pairing by QR code | Should | built |
| R15 | Display switch PC / PHONE with one tap | Should | built, phone side untested |
| R16 | One AI key for answers and voice (OpenRouter) | Should | built |
| R17 | Licence check + paid skills from the licence server | Must | planned |
| R18 | Morning / evening brief on a schedule, notification on the phone | Should | planned |
| R19 | Code signing: Apple Developer ID, Windows certificate | Must | planned |

## 5. User Experience
- **Set-up:** install, pick languages, paste one AI key, sign in to Google. Pair the phone by scanning a code.
- **At the desk:** hold Control+Option and talk; the answer is spoken and shown. Or click the character,
  or tap Talk / Brief / Mail / Meetings on the small display.
- **Away:** open Bader on the iPhone or message the Telegram bot. Same history.
- **Approvals:** a clear card with what will happen and two buttons, on whichever face is in use.
- **Tone:** short answers, answer first, no technical names shown to the user.
- Details: `03-UIUX-BRIEF.md`.

## 6. Success Metrics
| Metric | Target | Measured so far |
|---|---|---|
| Install to first answer | under 15 minutes, no support call | not measured (installer not built) |
| Simple question, first words | under 3 s | 1.4–2.4 s on the Mac |
| Tool task (mail, calendar, news) | under 20 s | 9–41 s; news 13–16 s |
| Speech understood correctly (Arabic/English) | 95% of asks | not measured; turbo model fixed the known misses |
| Wrong action without approval | 0 | 0 since the approval hook covers every tool |
| Idle memory on the computer | under 1 GB | about 0.6 GB (up to 2.6 GB with the speech model loaded) |
| Daily active use | 80% of working days | not measured |

## 7. Technical Requirements
- **Engine (computer):** Hermes Agent fork, profile `bader`, API on 127.0.0.1:8642. Model
  `anthropic/claude-haiku-4.5` through OpenRouter. A quick lane answers simple asks directly.
- **Computer app:** Tauri 2 (Rust + TypeScript). Build with `NODE_ENV=development npx tauri build`.
- **Voice (computer):** speech-to-text on device (faster-whisper `large-v3-turbo`) or OpenRouter;
  text-to-speech edge-tts or OpenRouter.
- **iPhone app:** SwiftUI, iOS 17+, built with xcodegen. Talks to OpenRouter and Google directly.
  Speech-to-text and natural voice through OpenRouter; Apple's voice as fallback.
- **Shared memory:** one file in the user's own Google Drive. No server of ours holds user data.
- **Display:** ESP32 (2.8" ILI9341 touch), MicroPython, line protocol 3.1 over USB serial and Bluetooth LE.
- **Secrets:** Mac: file readable only by the user; Windows: Credential Manager; iPhone: Keychain.
- **Recommended computer:** 4 cores, 8 GB RAM (16 GB with the accurate speech model), 5 GB free disk.
- **Constraints:** the public repo must never hold keys or the user's mail content.
- Details: `04-BACKEND-SCHEMA.md`.

## 8. App Flow — see `02-APPFLOW.md`
## 9. UI/UX Design Brief — see `03-UIUX-BRIEF.md`
## 10. Backend Schema — see `04-BACKEND-SCHEMA.md`
## 11. Implementation Plan — see `05-IMPLEMENTATION-PLAN.md`
