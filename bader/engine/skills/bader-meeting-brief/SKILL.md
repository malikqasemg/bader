---
name: bader-meeting-brief
description: "Summarise a meeting (recording, transcript, notes or calendar event) into a brief: summary, decisions, action items with owner and due date, and a follow-up email draft — in the user's chosen summary language."
version: 1.0.0
author: iNetGenius (Bader)
license: Proprietary
platforms: [linux, macos, windows]
metadata:
  hermes:
    tags: [Meetings, Summary, Action-Items, Follow-Up, Arabic, English, Bader]
    related_skills: [meeting-action-items, google-workspace, docx]
---

# Bader Meeting Brief

Turn any meeting into a short executive brief and a clear list of actions.

## When to use
- "Summarise this meeting / recording / transcript."
- "What are the action items from today's meeting?"
- "لخص الاجتماع" / "ما هي المهام من الاجتماع؟"
- The user drops an audio or video file of a meeting, or pastes notes.

## Step 1 — Get the content
- **Audio or video file** (m4a, mp3, wav, mp4, webm, ogg): transcribe it with
  exactly this command (do not install anything else):
  `python "$HERMES_HOME/skills/productivity/bader-meeting-brief/scripts/transcribe.py" "<file>"`
  It prints the transcript. Meetings can be in Arabic, English or mixed — keep
  the original wording; speech-to-text can garble a few words, mark them "(unclear)".
- **Transcript or notes**: use as given.
- **Calendar event only**: say there is no recording/transcript yet and offer to
  summarise once one is shared. Never invent what was said.

## Step 2 — Pick the output language
Read `$HERMES_HOME/bader_prefs.json` → `summary_lang`:
- `en` → write the whole brief in English, even if the meeting was in Arabic.
- `ar` → write the whole brief in Arabic (formal), even if the meeting was in English.
- `auto` or missing → use the main language of the meeting.
If the user asks for a language in the message, that wins.
Keep names, company names, product names and numbers exactly as spoken.

## Step 3 — Write the brief (this exact shape)

**Meeting:** title · date · attendees (only names actually heard or given)

**Summary** — 3–5 short bullet points.

**Decisions** — bullets; "None recorded" if none.

**Action items**
| # | Action | Owner | Due |
|---|---|---|---|
Owner and Due only if stated; otherwise "TBD". Never guess people or dates.

**Open questions** — bullets, only if any.

**Follow-up email (draft)** — short, ready to send, same language as the brief.

## Step 4 — Offer next steps (ask first, never do them silently)
- Send the follow-up email (needs the user's approval).
- Save the brief as a Word document (docx skill) or add actions to the calendar.

## Rules
- Short and direct; no filler.
- Mark anything unclear from the audio as "(unclear)".
- Never include meeting passwords, host keys or dial-in PINs.
