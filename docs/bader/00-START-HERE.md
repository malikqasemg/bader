# Bader — Start Here

Bader is a personal AI assistant (Arabic + English) sold by iNetGenius under licence.
It is a rebranded fork of Hermes Agent (Nous Research, MIT).

## Read in order
1. `01-PRD.md` — what Bader is and must do
2. `02-APPFLOW.md` — install, licence, daily use
3. `03-UIUX-BRIEF.md` — look, language, chat behaviour
4. `04-BACKEND-SCHEMA.md` — licence server + local data
5. `05-IMPLEMENTATION-PLAN.md` — phases and tasks

## Repo facts
- Repo: https://github.com/malikqasemg/bader (fork of NousResearch/hermes-agent)
- Local: /Users/malekqasem/Bader
- Remotes: `origin` = Bader fork, `upstream` = Hermes. Pull upstream fixes with
  `git fetch upstream && git merge upstream/main`.
- Bader-specific code lives in clearly named folders (`bader/`, `skills/bader/`,
  `docs/bader/`) so upstream merges stay easy.

## Licence rules
- Hermes code is MIT: keep `LICENSE` and the Nous Research copyright notice.
- Do not use the Hermes name or logo in the shipped app. Product name = **Bader**.
- Paid skills are NOT stored in this repo. They live on the iNetGenius VPS and are
  served per licensed call, same model as iNetBuzz.
