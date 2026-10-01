---
name: bader-news
description: Fast news headlines on any company, person or topic (Arabic or English) in about one second. Use for "latest news about X", "what happened with X this week", "أخبار X".
---
# Bader news

Run once, then answer from the result:

    python "$HERMES_HOME/skills/productivity/bader-news/scripts/news.py" "<topic>" --days 7 --max 8

- Topic in the user's language (Arabic topic → Arabic sources). `--lang en|ar` forces one.
- `--days 1` for today, `--days 30` for the month.
- Reply with 4–6 headlines: one line each, with source and date. No browser, no page extraction.
- Only if the user asks for detail on one headline, run again with `--links` and open that link with `web_extract`.
