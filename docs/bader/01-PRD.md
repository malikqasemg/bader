# Bader — Product Requirements Document

## 1. Overview
Bader is a personal AI assistant for executives and staff. It reads Outlook email and
calendar, Webex meetings and chat, then briefs the user and acts on command. The user
talks to Bader on Telegram or WhatsApp Business, in Arabic or English. Bader installs
like any desktop app (Windows first, macOS next). The customer decides where it runs:
a PC or an always-on server, same installer.

Business model: the app is free to install; skills are paid per licence key, served
from the iNetGenius licence server (same model as iNetBuzz).

## 2. Goals & Objectives
1. One-click install, no terminal, no config files.
2. Daily value: morning and evening briefs, meeting summaries, action items.
3. Acts on command: if the user asks, it is approved. Agent-initiated actions ask first.
4. Learns the user (priorities, VIPs, tone) — Hermes self-improving memory.
5. Easy to extend: a new connector = a new skill or MCP server, no core change.
6. Licensed: skills locked without a valid key; revenue per seat.

Out of scope v1: multi-user admin console, on-prem licence server, voice calls.

## 3. Requirements
| ID | Requirement | Priority |
|---|---|---|
| R1 | Branded installer "Bader" for Windows (MSIX/EXE); macOS DMG later | Must |
| R2 | First-run wizard: licence key → LLM key → Microsoft sign-in → Webex sign-in → chat channel | Must |
| R3 | Outlook mail + calendar via Microsoft Graph (read, draft, send, schedule) | Must |
| R4 | Webex meetings, transcripts, AI summaries, space messages | Must |
| R5 | Summaries + action items (owner, due date) from mail, chats, meetings | Must |
| R6 | Scheduled briefs by Telegram/WhatsApp message and/or email | Must |
| R7 | Channels: Telegram bot + WhatsApp Business Cloud API | Must |
| R8 | Arabic + English in/out, RTL UI, replies in the user's language | Must |
| R9 | Licence check against iNetGenius VPS; skills served per licensed call | Must |
| R10 | Approval rule: user command = run; agent idea = one-tap approve | Must |
| R11 | Audit log of every action | Must |
| R12 | Proactive alerts (VIP mail, overdue commitment, meeting soon) | Should |
| R13 | Delegate and chase replies on the user's behalf | Should |
| R14 | Auto-update from GitHub releases | Should |

## 4. User Experience
User installs Bader, enters a licence key, signs in to Microsoft and Webex, scans a
QR / opens a Telegram link. From then on Bader lives in the tray and in chat.
7:00 brief arrives; "brief me on the 10am"; after meetings a summary + "send
follow-up?"; voice notes in Arabic work; 18:00 end-of-day email.

## 5. Success Metrics
| Metric | Target (proposed) |
|---|---|
| Install to first brief | < 15 min, no support call |
| Daily active use | ≥ 80% of working days |
| Brief marked useful | ≥ 75% |
| Commands done first time | ≥ 90% |
| Wrong actions | ≤ 2% |
| Licence conversion after trial | [to set] |

## 6. Technical Requirements
- Engine: Hermes Agent (Python agent + gateway), runs hidden.
- Face: coucou-style Tauri shell (top-of-screen island + tray), MIT code only.
- Reused from Hermes: Telegram platform, WhatsApp Cloud adapter
  (`gateway/platforms/whatsapp_cloud.py`), MS Graph webhook adapter
  (`gateway/platforms/msgraph_webhook.py`), memory, cron, skills, `locales/ar.yaml`.
- New: Bader branding, `bader/license` client, licensed skill loader, Outlook skill,
  Webex skill/MCP server, Briefs skill, Arabic persona (`SOUL.md`).
- Licence server: iNetGenius VPS, reuse iNetBuzz `mcp-server/license.py`
  (HMAC-signed device token, seats, usage events), new product `bader`.
- Secrets: OS credential store (Windows Credential Manager / macOS Keychain).
- Customer brings own LLM key (or iNetGenius-provided endpoint, later).

### Bader on iPhone (added 2026-10-03)
- A second, standalone Bader on the iPhone: it keeps working when the computer is off.
- Does on the phone: mail, calendar, news, weather, voice in and out, camera ("look at this"), approvals, memory.
- Stays on the computer only: browser control, long meeting transcription, files on the computer.
- One shared memory: both Baders read and add to one file in the user's own Google Drive (no server of ours).
- Pairing: the computer shows a QR code once; the phone scans it and receives the keys. Nothing is typed.
- The small display works with either Bader: USB to the computer, Bluetooth to the phone, one tap on its PC / PHONE button to switch.

## 7. App Flow — see `02-APPFLOW.md`
## 8. UI/UX Design Brief — see `03-UIUX-BRIEF.md`
## 9. Backend Schema — see `04-BACKEND-SCHEMA.md`
## 10. Implementation Plan — see `05-IMPLEMENTATION-PLAN.md`
