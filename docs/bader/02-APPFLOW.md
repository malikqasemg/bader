# Bader — App Flow

## Flow 1 — Install and activate (once)
1. User runs `Bader-Setup.exe` (or MSIX). No admin rights, no terminal.
2. Bader opens the welcome screen (Arabic / English switch).
3. Enter licence key `BADR-XXXX-XXXX` → app sends key + device fingerprint
   (serial, MAC, hostname, OS) to the licence server → server returns an
   HMAC-signed token bound to that device. Invalid key = stop here.
4. Enter LLM provider key (stored in the OS credential store).
5. Sign in to Microsoft (Graph OAuth, delegated) and Webex (OAuth integration).
6. Pick chat channel: Telegram (bot link) and/or WhatsApp Business (number linked
   by the customer's WhatsApp Business account).
7. Bader backfills the last 30 days, sends a first brief, asks: brief times, VIPs,
   language.

## Flow 2 — Licensed skill load (every session start + every 24 h)
1. App presents the signed token to the licence server.
2. Server checks: key active, not expired, seat not locked, device matches.
3. Server returns the skill bundle for the key's entitlements (content + version).
4. Skills are kept in memory only (not written to disk). Expired token = paid skills
   unload, free core keeps working and shows "licence expired".

## Flow 3 — Ingest and brief
1. Graph change notifications / polling + Webex webhooks / polling bring new items.
2. Agent classifies priority, extracts action items, updates memory.
3. Cron builds briefs at the user's times and delivers by chat and/or email.

## Flow 4 — User command
1. Text or voice note in Telegram / WhatsApp.
2. Voice → speech-to-text; language detected.
3. Agent plans, calls tools, executes. Confirms in one line, in the user's language.
4. Logged to audit + usage event to licence server (tool name only, no content).

## Flow 5 — Agent-initiated action
1. Agent spots a need (unanswered VIP mail, meeting follow-up).
2. Sends proposal with buttons Approve / Edit / Skip.
3. Approve → run. The choice is stored so Bader learns the user.

## Flow 6 — Pair the iPhone (once)
1. Computer: Settings → "Bader on iPhone" → Show pairing code (a QR code, hidden again after 60 s).
2. iPhone: open Bader → Scan the pairing code.
3. The phone stores the keys in its Keychain and syncs the shared memory.

## Flow 7 — Ask on the iPhone
1. Tap the mic (or type, or take a photo).
2. Speech → text (cloud, Arabic/English) → the model answers, calling tools for mail / calendar / news / weather.
3. Sending a mail or creating a meeting shows an approval card: Yes / No.
4. The answer is shown, read aloud, added to shared memory, and synced to Drive.

## Flow 8 — Hand the display over
1. The display's top bar shows PC or PHONE (drawn by the display itself).
2. Tap it: the old owner is told AWAY, the new owner READY, and the new owner redraws everything.
3. A side that was never heard from since power-on gives the screen to the first side that says hello.

## Flow 9 — Ask at the computer (as built, 2026-10-03)
1. Hold Control+Option (Ctrl+Alt on Windows) and talk, or type in the window, or tap a button on the display.
2. Speech → text. Simple asks go to the quick lane (one model call); mail, calendar, news, search go to the engine.
3. The answer streams in, is spoken sentence by sentence, and a notification appears if the window is hidden.
4. The ask and the answer are written to the journal and to shared memory.

## Flow 10 — First run on the computer
1. Settings opens by itself until set-up is finished.
2. Pick main and second language (Arabic / English), paste the AI key (OpenRouter first), pick voice: on device or cloud.
3. Connect Google (browser sign-in). Optional: Telegram bot token + user ID.
