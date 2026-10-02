---
name: bader-recall
description: Remember earlier work — find what the user asked before and what Bader answered (meeting summaries, files made, decisions) on any channel. Use whenever the user refers to "yesterday", "last time", "the transcript/file I shared", "what did I ask".
---
# Bader recall

    python "$HERMES_HOME/skills/productivity/bader-recall/scripts/recall.py" "<keywords>" --days 30

- Keywords: 2–5 words from what the user is referring to, in English and Arabic if unsure (e.g. "meeting transcript summary اجتماع").
- No keywords → the latest 15 asks.
- Answer from the result. Never say you don't remember before running this.
