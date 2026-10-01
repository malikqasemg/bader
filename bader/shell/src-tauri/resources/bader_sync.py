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


def main():
    result = {"updated": dt.datetime.now().astimezone().isoformat(timespec="minutes")}
    errors = []
    for key, fn in (("gmail", gmail), ("calendar", calendar)):
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
