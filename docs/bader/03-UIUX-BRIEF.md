# Bader — UI/UX Design Brief

## Identity
- Name: **Bader** (بدر) — "the personal assistant" / "المساعد الشخصي".
- Tone: calm, executive, short. No emoji, no filler.
- Character: Bader robot — gold body, cyan glowing face, red-and-white shemagh + black agal, headset.
- Assets in `bader/assets/`: `icon.ico` + `icon-*.png` (app/installer, dark background),
  `tray-*.png` (head only, transparent, for tray + island), `bader-character-512.png` (full body, transparent).
- Colours (from the art, approximate): navy background #121B26, gold body, cyan accents, shemagh red.

## Languages
- Arabic and English everywhere: installer, wizard, tray menu, chat.
- Arabic UI fully RTL; numbers and names kept as in source.
- Chat replies follow the language the user wrote in. Formal Arabic by default,
  Gulf dialect understood in voice notes.

## Desktop app — coucou-style shell (decision 2026-10-01)
Bader's face is a small always-on-top island at the top of the screen + tray icon,
modelled on coucou (MIT code; its name, Mochi character and sounds are NOT reused).
Hermes runs hidden underneath as the engine. The island shows status, approvals
(Approve / Skip), quick chat and the setup wizard; full settings open in a small window.

### Screens
| Screen | Purpose |
|---|---|
| Welcome + licence | Enter key, see plan and expiry |
| Connect accounts | Microsoft, Webex, Telegram, WhatsApp Business |
| Briefs | Times, channel per brief, sections on/off |
| People | VIPs and delegates |
| Memory | What Bader learned; delete any item |
| Activity | Every action, with undo where possible |
| Settings | LLM provider, language, updates |

Tray: Bader icon, status dot (green = running, amber = needs sign-in, red = licence).

## Chat (Telegram / WhatsApp)
- Brief: title → Meetings today / Needs you / Overdue → max 5 items each →
  "reply 'more'".
- Action card: one line + Approve / Edit / Skip buttons.
- Confirmation: ✓ + what + to whom, one line.
- Every brief ends with: useful / too long / missed something.

## iPhone app (2026-10-03)
- Dark, one screen: Bader's face and status on top, the conversation in the middle, quick buttons
  (Brief, Mail, Meetings, News) and one big mic at the bottom. Camera button next to the text box.
- Approval card: yellow, title + detail, "No" and green "Yes, do it".
- Arabic text aligns right; English left. Answers are read with the phone's built-in voices.
- Settings: read answers aloud, display on/off + status, shared memory (count, last sync, sync now), unpair.

## Display driven by the phone
- Same layout as from the computer: bar, face, one status line, buttons (Talk, Brief, Mail, Meet).
- After an answer: text pages with More / Home. Approval: Yes ✓ / No ✕. A pressed button turns amber.

## Computer app as built (2026-10-03)
- Window pinned to the very top of the screen, content below the menu bar; tabs Home / Chat, gear, ✕ to hide.
- Bader character on the desktop: drag to move (spot remembered), double-click opens chat, right-click gives
  Talk, Brief, Mail, Meetings, Show/hide window, Hide Bader, Quit. Mode: always / on events / off.
- Menu-bar menu: Open Bader, Show/hide window, ✓ Show Bader on the desktop, Settings, Pause, Quit.
- Display buttons: choice "Text only" or "Text + Voice" per press, or a default set in Settings.
- Rule: the user never sees engine, tool or file names.
