"""Bader background sync — runs in the engine's Python every few minutes.

Fetches the last 7 days of mail and the calendar from 3 days back to 7 days ahead from the connected
accounts and writes a compact snapshot to $HERMES_HOME/bader_inbox.json.
The island answers mail/calendar questions from this snapshot in one model
call instead of a multi-step tool loop.

    python bader_sync.py            -> {"ok": true, "mails": N, "events": M}
"""

import datetime as dt
import json
import os
import re
import subprocess
import sys

sys.path.insert(0, os.getcwd())

HOME = os.environ.get("HERMES_HOME", os.path.expanduser("~/.hermes"))
GAPI = os.path.join(HOME, "skills", "productivity", "google-workspace", "scripts", "google_api.py")
OUT = os.path.join(HOME, "bader_inbox.json")

INVISIBLE = re.compile(r"[͏​-‏ - ﻿­]+")


def _clean(text, limit):
    text = INVISIBLE.sub("", text or "")
    text = re.sub(r"\s+", " ", text).strip()
    return text[:limit]


def _gapi(*args, timeout=90):
    out = subprocess.run(
        [sys.executable, GAPI, *args], capture_output=True, text=True, timeout=timeout,
        env={**os.environ, "PYTHONPATH": os.getcwd()},
    )
    if out.returncode != 0:
        raise RuntimeError((out.stderr or out.stdout).strip()[-300:])
    return json.loads(out.stdout or "[]")


def gmail():
    if not os.path.isfile(os.path.join(HOME, "google_token.json")):
        return None
    # Everything from the last 2 days; for days 3-7 only primary/important mail,
    # so a week fits in the snapshot without a wall of newsletters.
    base = "in:inbox -category:promotions -category:social"
    items = _gapi("gmail", "search", f"newer_than:2d {base}", "--max", "80")
    seen = {m.get("id") for m in items}
    older = _gapi("gmail", "search",
                  f"newer_than:7d older_than:2d {base} {{category:primary is:important is:starred}}", "--max", "100")
    items += [m for m in older if m.get("id") not in seen]
    mails = []
    for n, m in enumerate(items):
        labels = m.get("labels") or []
        mails.append({
            "id": m.get("id"),
            "from": _clean(m.get("from"), 80),
            "subject": _clean(m.get("subject"), 140),
            "date": m.get("date"),
            # Full snippet for the newest mail, shorter for older ones (keeps the snapshot small).
            "snippet": _clean(m.get("snippet"), 200 if n < 40 else 110),
            "unread": "UNREAD" in labels,
            "important": "IMPORTANT" in labels,
        })
    return mails


def calendar():
    if not os.path.isfile(os.path.join(HOME, "google_token.json")):
        return None
    now = dt.datetime.now(dt.timezone.utc)
    today = now.replace(hour=0, minute=0, second=0, microsecond=0) - dt.timedelta(hours=3)
    start = today - dt.timedelta(days=3)
    end = today + dt.timedelta(days=7, hours=3)
    items = _gapi("calendar", "list", "--start", start.isoformat(), "--end", end.isoformat(), "--max", "60")
    events = []
    for e in items:
        loc = e.get("location") or ""
        events.append({
            "title": _clean(e.get("summary"), 120),
            "start": e.get("start"),
            "end": e.get("end"),
            # Links only — descriptions can hold meeting passwords and host keys.
            "where": loc if loc.startswith("http") else _clean(loc, 80),
        })
    return events


def history(days=7, limit=60):
    """What the user asked and what Bader answered, across every channel."""
    import sqlite3
    import time
    since = time.time() - days * 86400
    items = []
    db_path = os.path.join(HOME, "state.db")
    if os.path.isfile(db_path):
        db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True, timeout=5)
        rows = db.execute(
            "select m.session_id, s.source, m.role, m.content, m.timestamp from messages m "
            "join sessions s on s.id = m.session_id "
            "where m.timestamp > ? and m.role in ('user','assistant') order by m.session_id, m.id", (since,)
        ).fetchall()
        db.close()
        cur = None
        for sid, source, role, content, ts in rows:
            text = content if isinstance(content, str) else ""
            if role == "user":
                if cur:
                    items.append(cur)
                cur = {"sid": sid, "ts": ts, "channel": "island" if source == "api_server" else source,
                       "ask": text, "answer": ""}
            elif cur and cur["sid"] == sid and text.strip():
                cur["answer"] = text  # the last assistant text of the turn is the answer
        if cur:
            items.append(cur)
    try:
        with open(os.path.join(HOME, "bader_journal.jsonl"), encoding="utf-8") as f:
            for line in f:
                try:
                    j = json.loads(line)
                except ValueError:
                    continue
                if j.get("ts", 0) > since:
                    items.append(j)
    except OSError:
        pass
    items.sort(key=lambda i: i.get("ts") or 0, reverse=True)
    out = []
    for i in items[:limit]:
        ask = re.sub(r"\[Reply in [^\]]*\]|\[أجب[^\]]*\]", "", i.get("ask") or "")
        out.append({
            "when": dt.datetime.fromtimestamp(i["ts"]).astimezone().strftime("%a %Y-%m-%d %H:%M"),
            "channel": i.get("channel") or "",
            "ask": _clean(ask, 160),
            "answer": _clean(i.get("answer"), 260),
        })
    return out


def main():
    result = {"updated": dt.datetime.now().astimezone().isoformat(timespec="minutes")}
    errors = []
    for key, fn in (("history", history), ("calendar", calendar), ("gmail", gmail)):
        try:
            data = fn()
            if data is not None:
                result[key] = data
        except Exception as exc:  # keep the other source
            errors.append(f"{key}: {exc}")
    if errors:
        result["errors"] = errors
    tmp = OUT + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(result, f, ensure_ascii=False, indent=1)
    os.replace(tmp, OUT)
    print(json.dumps({"ok": not errors, "mails": len(result.get("gmail") or []),
                      "events": len(result.get("calendar") or []), "errors": errors}))


if __name__ == "__main__":
    main()
