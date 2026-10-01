# Bader — Implementation Plan

Durations are estimates.

## Phase 0 — Fork and docs (done 2026-10-01)
- [x] Fork NousResearch/hermes-agent → malikqasemg/bader, clone to ~/Bader
- [x] Docs 00–05 in `docs/bader/`

## Phase 1 — Rebrand (week 1)
- [ ] Desktop shell: port coucou's Windows Tauri app (`coucou/windows`) into `bader/shell/`;
      replace Claude Code hooks with a client for the Hermes gateway API; new Bader icon/character
- [ ] Bundle Hermes engine headless inside the installer (runs as background process)
- [ ] CLI/command name `bader` alongside `hermes` (keep `hermes` internally for upstream merges)
- [ ] `SOUL.md` → Bader persona, Arabic + English
- [ ] Arabic strings: review `locales/ar.yaml`, add Bader-specific keys
- [ ] Build Windows installer from `bader/shell` (Tauri NSIS setup.exe)

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
