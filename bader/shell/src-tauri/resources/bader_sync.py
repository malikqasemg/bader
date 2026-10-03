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


def _raw_history(days=7):
    """Every ask/answer on this computer (engine sessions + the window's journal)."""
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
    return items


def history(days=7, limit=60):
    """What the user asked and what Bader answered, across every channel and device."""
    import time
    since = time.time() - days * 86400
    items = _raw_history(days)
    known = {_entry_id(i.get("ts"), re.sub(r"\[Reply in [^\]]*\]|\[أجب[^\]]*\]", "", i.get("ask") or "").strip()) for i in items}
    try:
        with open(SHARED, encoding="utf-8") as f:
            for e in _read_lines(f.read()):
                # Things asked on the phone (or any other Bader) that this computer never saw.
                if e.get("device") != "pc" and e["id"] not in known and (e.get("ts") or 0) > since:
                    items.append({"ts": e["ts"], "channel": e.get("device") or "phone",
                                  "ask": e.get("ask") or ("Note: " if e.get("kind") == "note" else ""),
                                  "answer": e.get("answer") or ""})
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


# ── Shared memory ────────────────────────────────────────────────────────────
# One small file in the user's own Google Drive that every Bader (this computer,
# the phone) reads and adds to. Each line is one ask/answer or one note:
#   {"id", "ts", "device", "channel", "kind", "ask", "answer"}
# Lines are only ever added, so two devices merge by id without conflicts.
SHARED = os.path.join(HOME, "bader_shared.jsonl")
MEMORY_NAME = "bader-memory.jsonl"
MEMORY_KEEP = 500


def _entry_id(ts, ask):
    import hashlib
    return hashlib.sha1(f"{int(ts or 0)}|{ask or ''}".encode("utf-8")).hexdigest()[:16]


def _read_lines(text):
    out = []
    for line in (text or "").splitlines():
        try:
            j = json.loads(line)
        except ValueError:
            continue
        if isinstance(j, dict) and j.get("id"):
            out.append(j)
    return out


def merge_memory(*groups, keep=MEMORY_KEEP):
    """Union by id (first one seen wins), newest `keep` entries, oldest first."""
    seen = {}
    for group in groups:
        for e in group:
            seen.setdefault(e["id"], e)
    return sorted(seen.values(), key=lambda e: e.get("ts") or 0)[-keep:]


def _google_access():
    import urllib.parse
    import urllib.request
    with open(os.path.join(HOME, "google_token.json"), encoding="utf-8") as f:
        t = json.load(f)
    body = urllib.parse.urlencode({
        "client_id": t["client_id"], "client_secret": t["client_secret"],
        "refresh_token": t["refresh_token"], "grant_type": "refresh_token",
    }).encode()
    with urllib.request.urlopen(urllib.request.Request(t.get("token_uri") or "https://oauth2.googleapis.com/token", data=body), timeout=30) as r:
        return json.load(r)["access_token"]


def _drive(access, method, url, data=None, ctype=None):
    import urllib.request
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Authorization", f"Bearer {access}")
    if ctype:
        req.add_header("Content-Type", ctype)
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def _local_entries(days=30):
    items = []
    for h in _raw_history(days):
        ask = re.sub(r"\[Reply in [^\]]*\]|\[أجب[^\]]*\]", "", h.get("ask") or "").strip()
        if not ask or h.get("device"):
            continue
        items.append({
            "id": _entry_id(h.get("ts"), ask), "ts": int(h.get("ts") or 0), "device": "pc",
            "channel": h.get("channel") or "", "kind": "ask",
            "ask": _clean(ask, 400), "answer": _clean(h.get("answer"), 800),
        })
    return items


def memory():
    """Two-way sync of the shared memory file. Returns how many entries it holds."""
    import urllib.parse
    if not os.path.isfile(os.path.join(HOME, "google_token.json")):
        return None
    access = _google_access()
    q = urllib.parse.quote(f"name = '{MEMORY_NAME}' and trashed = false and appProperties has {{ key='bader' and value='memory' }}")
    found = json.loads(_drive(access, "GET", f"https://www.googleapis.com/drive/v3/files?q={q}&fields=files(id)&spaces=drive"))
    files = found.get("files") or []
    remote = []
    if files:
        fid = files[0]["id"]
        remote = _read_lines(_drive(access, "GET", f"https://www.googleapis.com/drive/v3/files/{fid}?alt=media").decode("utf-8", "replace"))
    else:
        meta = json.dumps({"name": MEMORY_NAME, "mimeType": "text/plain", "appProperties": {"bader": "memory"},
                           "description": "Bader's shared memory. Bader keeps this up to date - please do not edit or delete."}).encode()
        fid = json.loads(_drive(access, "POST", "https://www.googleapis.com/drive/v3/files?fields=id", meta, "application/json"))["id"]
    try:
        with open(SHARED, encoding="utf-8") as f:
            mirror = _read_lines(f.read())
    except OSError:
        mirror = []
    merged = merge_memory(remote, mirror, _local_entries())
    text = "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in merged)
    if {e["id"] for e in merged} != {e["id"] for e in remote}:
        _drive(access, "PATCH", f"https://www.googleapis.com/upload/drive/v3/files/{fid}?uploadType=media",
               text.encode("utf-8"), "text/plain; charset=utf-8")
    tmp = SHARED + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, SHARED)
    return len(merged)


def main():
    result = {"updated": dt.datetime.now().astimezone().isoformat(timespec="minutes")}
    errors = []
    try:
        result["memory"] = memory()  # first, so history below includes what the phone did
    except Exception as exc:
        errors.append(f"memory: {exc}")
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
                      "events": len(result.get("calendar") or []), "memory": result.get("memory"), "errors": errors}))


if __name__ == "__main__":
    main()
