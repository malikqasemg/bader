"""Bader's memory of earlier work: search past asks and answers on every channel.

    python recall.py "<keywords>" [--days 30] [--max 5]
    python recall.py                       -> the latest 15 asks

Reads the engine's conversation store and the app's quick-answer journal.
Prints JSON: [{"when", "channel", "ask", "answer"}], best match first.
"""
import argparse
import datetime as dt
import json
import os
import re
import sqlite3
import time

HOME = os.environ.get("HERMES_HOME", os.path.expanduser("~/.hermes"))


def turns(since):
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
                cur = {"sid": sid, "ts": ts, "channel": "app" if source == "api_server" else source,
                       "ask": text, "answer": ""}
            elif cur and cur["sid"] == sid and text.strip():
                cur["answer"] = text
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
                    j["channel"] = "app"
                    items.append(j)
    except OSError:
        pass
    return items


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("keywords", nargs="?", default="")
    ap.add_argument("--days", type=int, default=30)
    ap.add_argument("--max", type=int, default=5)
    a = ap.parse_args()
    items = turns(time.time() - a.days * 86400)
    words = [w.lower() for w in re.findall(r"\w{3,}", a.keywords)]
    if words:
        def score(i):
            hay = ((i.get("ask") or "") + " " + (i.get("answer") or "")).lower()
            return sum(1 for w in words if w in hay)
        # Skip turns that are themselves questions about the past with no real answer.
        blank = re.compile(r"don't have (access|any record|that)|do not have access|لا أتذكر|ليس لدي", re.I)
        items = [i for i in items if score(i) and len(i.get("answer") or "") > 80
                 and not blank.search((i.get("answer") or "")[:300])]
        items.sort(key=lambda i: (score(i), i.get("ts") or 0), reverse=True)
        limit, cut = a.max, 3500
    else:
        items.sort(key=lambda i: i.get("ts") or 0, reverse=True)
        limit, cut = 15, 300
    out = [{
        "when": dt.datetime.fromtimestamp(i["ts"]).astimezone().strftime("%a %Y-%m-%d %H:%M"),
        "channel": i.get("channel") or "",
        "ask": (i.get("ask") or "")[:400],
        "answer": (i.get("answer") or "")[:cut],
    } for i in items[:limit]]
    print(json.dumps(out, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
