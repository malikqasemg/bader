# Bader — Backend Schema

Two parts: the iNetGenius licence server (shared with iNetBuzz) and local data on the
customer machine (Hermes state + Bader additions).

## A. Licence server (iNetGenius VPS) — reuse iNetBuzz `mcp-server`
Reuse as-is: `license.py` (HMAC-signed device token, `NETBUZZ_LICENSE_SECRET`
pattern), seat locks, usage events, Stripe webhook issue/renew, admin console tab.

Changes for Bader:
| Change | Detail |
|---|---|
| Product column | `customers.product` text: `inetbuzz` / `bader` (default `inetbuzz`) |
| Key prefix | `BADR-XXXX-XXXX` for Bader |
| Secret | own `BADER_LICENSE_SECRET` (rotating one product never logs out the other) |
| Entitlements | reuse `agent_entitlements`; `agent_id` = Bader skill pack id (`bader-core`, `bader-outlook`, `bader-webex`, `bader-briefs`) |
| Skill content | reuse `agents` table: `agent_id`, `skills` jsonb {path: SKILL.md content}, `content_version`, `channel` |
| Admin | "Bader" in Generate-codes product dropdown + Bader licences tab |

Existing tables used unchanged: `customers`, `agent_entitlements`, `seat_sightings`,
`seat_locks`, `usage_events`, `payments`, `agents`.

### Endpoints (Bader)
| Method | Path | Purpose |
|---|---|---|
| POST | /bader/activate | key + device → signed token |
| POST | /bader/skills | token → skill bundle for entitlements |
| POST | /bader/usage | token + tool name + state (no content) |
| GET | /bader/status | token → plan, expiry, entitled packs |

## B. Local (customer machine)
Hermes state DB stays as-is (sessions, memory, messages, cron). Bader adds:
| Store | Content |
|---|---|
| OS credential store | licence token, LLM key, Graph + Webex OAuth tokens, bot tokens |
| `bader/state.json` | brief times, VIPs, channel choice, language |
| `bader/audit.jsonl` | every action: tool, params summary, initiated_by (user/agent), approval, result |
| `bader/action_items` (state DB table) | text, owner, due_at, status, source item |

Rule: `initiated_by = user` runs at once; `agent` waits for approval.

## C. Shared memory (user's own Google Drive)
File `bader-memory.jsonl` (found by app property `bader=memory`). One JSON object per line, lines are only added:

| Field | Meaning |
|---|---|
| `id` | sha1("<ts>|<ask>") first 16 hex chars — the merge key |
| `ts` | Unix seconds |
| `device` | `pc` or `phone` |
| `channel` | island, telegram, phone… |
| `kind` | `ask` or `note` |
| `ask`, `answer` | cut to 400 / 800 characters |

Merge = union by id, newest 500 kept. Computer: `bader_sync.py memory()` every sync, mirror in `bader_shared.jsonl`.
Phone: `Memory.sync` on open and after each answer.

## D. Pairing code (QR, shown on the computer only on request)
`{"bader":1,"ai":<OpenRouter key>,"l1":"en","l2":"ar","g":{"id","s","r","a"}}` — g = Google client id, client secret,
refresh token, account. The phone keeps it in the Keychain (this device only).

## E. Display link switch (face protocol 3.1)
Same line protocol on USB serial and on Bluetooth LE (Nordic UART service, device name `Bader`).
New lines: board → app `AWAY`, reply `ERR away` to the side that does not own the screen.
The board owns x 36..84 of the top bar (its PC / PHONE button).

## F. Files on the computer as built (`~/.hermes/profiles/bader/`)
| File | Holds |
|---|---|
| `config.yaml`, `.env` | engine settings and keys (never in the repo) |
| `bader_prefs.json` | languages, voice choices, screen reply mode, character mode and spot |
| `bader_inbox.json` | snapshot: last 7 days of mail, calendar 3 days back to 7 ahead, recent history |
| `bader_journal.jsonl` | every ask/answer from the window |
| `bader_shared.jsonl` | mirror of the shared memory file in Drive |
| `google_token.json` | Google sign-in |
| `~/Library/Application Support/Bader/secrets.json` (Mac) | app secrets, readable only by the user |
