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
    # No script prefix required: the agent sometimes calls the script through a
    # shell variable ($GAPI gmail send …) or a helper script it wrote itself.
    (re.compile(r"\bgmail\s+send\b"), "Send email"),
    (re.compile(r"\bgmail\s+reply\b"), "Reply to email"),
    (re.compile(r"\bgmail\s+forward\b"), "Forward email"),
    (re.compile(r"\bgmail\s+(trash|delete)\b"), "Delete email"),
    (re.compile(r"\bgmail\b[^\n]*\bmodify\b[^\n]*\bTRASH\b", re.S), "Delete email"),
    (re.compile(r"""['"]gmail['"]\s*,\s*['"](send|reply|forward|trash|delete)['"]"""), "Email action: {0}"),
    (re.compile(r"""['"]calendar['"]\s*,\s*['"](create|delete)['"]"""), "Calendar action: {0}"),
    (re.compile(r"""['"]modify['"][^\n]*['"]TRASH['"]"""), "Delete email"),
    (re.compile(r"\bcalendar\s+create\b"), "Create calendar event"),
    (re.compile(r"\bcalendar\s+delete\b"), "Delete calendar event"),
    (re.compile(r"\bdrive\s+(delete|share)\b"), "Change a Drive file"),
    (re.compile(r"\bbader_outlook\.py\S*\s+(send|reply|forward|delete)\b"), "Outlook: {0}"),
    (re.compile(r"\bbader_outlook\.py\S*\s+event\s+(create|delete)\b"), "Outlook calendar: {0}"),
    (re.compile(r"\bhimalaya\b.*\b(send|write|reply|forward)\b"), "Send email"),
    # Raw API calls inside scripts.
    (re.compile(r"\.messages\(\)\s*\.\s*(send|trash|delete)\("), "Email action: {0}"),
    (re.compile(r"\.events\(\)\s*\.\s*(insert|delete)\("), "Calendar action: {0}"),
]

_SCRIPT = re.compile(r"""[^\s'"]+\.(?:py|sh)\b""")


def _with_scripts(command: str) -> str:
    """The command plus the text of any local script it runs (agent-written helpers)."""
    text = command
    for path in _SCRIPT.findall(command):
        p = os.path.expanduser(os.path.expandvars(path))
        if p.endswith("google_api.py") or not os.path.isfile(p):
            continue
        try:
            if os.path.getsize(p) < 300_000:
                with open(p, encoding="utf-8", errors="ignore") as f:
                    text += "\n" + f.read()
        except OSError:
            pass
    return text


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
        to = _flag(command, "--to") if "\n" not in command else ""
        subject = (_flag(command, "--subject") or _flag(command, "--summary") or _flag(command, "--title")) if "\n" not in command else ""
        bits = [text]
        if to:
            bits.append(f"to {to[:60]}")
        if subject:
            bits.append(f"“{subject[:60]}”")
        return " ".join(bits)
    return None


def _strings(value: Any, out: list) -> list:
    if isinstance(value, str):
        out.append(value)
    elif isinstance(value, dict):
        for v in value.values():
            _strings(v, out)
    elif isinstance(value, (list, tuple)):
        for v in value:
            _strings(v, out)
    return out


def _on_pre_tool_call(tool_name: str = "", args: Any = None, **_: Any) -> Optional[Dict[str, str]]:
    # Every tool, not just `terminal`: code can also run through browser_exec,
    # execute_code, write-then-run scripts, MCP shells, etc.
    if not _enabled() or args is None:
        return None
    command = "\n".join(_strings(args, []))
    if not command:
        return None
    description = _describe(_with_scripts(command))
    if not description:
        return None
    # One approval per distinct action, so approving one email never approves the next.
    key = hashlib.sha256((tool_name + command).encode("utf-8")).hexdigest()[:16]
    return {"action": "approve", "message": description, "rule_key": f"bader:{key}"}


def register(ctx) -> None:
    ctx.register_hook("pre_tool_call", _on_pre_tool_call)
