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
