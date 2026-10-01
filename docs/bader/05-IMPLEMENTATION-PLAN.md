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
- [ ] `SOUL.md` → Bader persona, Arabic + English
- [ ] Arabic strings: review `locales/ar.yaml`, add Bader-specific keys
- [x] Build Windows installer from `bader/shell` (Tauri NSIS setup.exe) — v0.1.0
- [x] macOS build of the shell (DMG) for testing
- [x] Settings: AI (provider, key, model) and Voice (Arabic + English STT, voice, spoken replies) → engine profile
- [x] Connector pills: Outlook, Webex, Gmail
- [ ] Settings + wizard in Arabic (RTL); Bader tray icon (head) instead of app icon

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
- Bader logo and colours
- Microsoft Entra app registration + Webex integration (iNetGenius-owned, multi-tenant)
- Windows code-signing certificate
- Bader price (Stripe)
