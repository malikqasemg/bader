"""bader-approvals — human-in-the-middle for actions done in the user's name.

Before Bader sends, replies to or deletes mail, or creates/deletes calendar
events, this hook escalates the tool call to the engine's human-approval gate.
The approval then appears on every surface the user has: the Bader island
(Approve / Deny), the Bader face screen (press = approve, hold = deny), and
Telegram / WhatsApp buttons.

Turned off with `"approvals": false` in $HERMES_HOME/bader_prefs.json.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
from typing import Any, Dict, Optional

# (regex on the command, human description builder)
_RULES = [
    (re.compile(r"google_api\.py\S*\s+gmail\s+send\b"), "Send email"),
    (re.compile(r"google_api\.py\S*\s+gmail\s+reply\b"), "Reply to email"),
    (re.compile(r"google_api\.py\S*\s+gmail\s+forward\b"), "Forward email"),
    (re.compile(r"google_api\.py\S*\s+gmail\s+(trash|delete)\b"), "Delete email"),
    (re.compile(r"google_api\.py\S*\s+calendar\s+create\b"), "Create calendar event"),
    (re.compile(r"google_api\.py\S*\s+calendar\s+delete\b"), "Delete calendar event"),
    (re.compile(r"google_api\.py\S*\s+drive\s+(delete|share)\b"), "Change a Drive file"),
    (re.compile(r"bader_outlook\.py\S*\s+(send|reply|forward|delete)\b"), "Outlook: {0}"),
    (re.compile(r"bader_outlook\.py\S*\s+event\s+(create|delete)\b"), "Outlook calendar: {0}"),
    (re.compile(r"\bhimalaya\b.*\b(send|write|reply|forward)\b"), "Send email"),
]


def _enabled() -> bool:
    home = os.environ.get("HERMES_HOME", os.path.expanduser("~/.hermes"))
    try:
        with open(os.path.join(home, "bader_prefs.json"), encoding="utf-8") as f:
            return json.load(f).get("approvals", True) is not False
    except Exception:
        return True


def _flag(command: str, name: str) -> str:
    try:
        parts = shlex.split(command)
    except ValueError:
        return ""
    for i, p in enumerate(parts):
        if p == name and i + 1 < len(parts):
            return parts[i + 1]
        if p.startswith(name + "="):
            return p.split("=", 1)[1]
    return ""


def _describe(command: str) -> Optional[str]:
    for pattern, label in _RULES:
        m = pattern.search(command)
        if not m:
            continue
        text = label.format(*(g for g in m.groups() if g)) if "{0}" in label else label
        to = _flag(command, "--to")
        subject = _flag(command, "--subject") or _flag(command, "--summary") or _flag(command, "--title")
        bits = [text]
        if to:
            bits.append(f"to {to[:60]}")
        if subject:
            bits.append(f"“{subject[:60]}”")
        return " ".join(bits)
    return None


def _on_pre_tool_call(tool_name: str = "", args: Any = None, **_: Any) -> Optional[Dict[str, str]]:
    if tool_name != "terminal" or not isinstance(args, dict) or not _enabled():
        return None
    command = args.get("command")
    if not isinstance(command, str):
        return None
    description = _describe(command)
    if not description:
        return None
    # One approval per distinct action, so approving one email never approves the next.
    key = hashlib.sha256(command.encode("utf-8")).hexdigest()[:16]
    return {"action": "approve", "message": description, "rule_key": f"bader:{key}"}


def register(ctx) -> None:
    ctx.register_hook("pre_tool_call", _on_pre_tool_call)
