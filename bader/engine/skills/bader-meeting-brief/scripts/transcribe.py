"""Transcribe a meeting recording with Bader's own speech-to-text.

    python transcribe.py <audio-or-video-file>

Prints the transcript (Arabic, English or mixed — language detected
automatically). Uses the engine's speech-to-text, so the Voice settings in
Bader (local / OpenAI, accuracy) apply.
"""

import json
import os
import subprocess
import sys
from pathlib import Path

CODE = r"""
import json, sys
from tools.transcription_tools import transcribe_audio
r = transcribe_audio(sys.argv[1], source="voice_mode")
if isinstance(r, str):
    r = json.loads(r)
print(json.dumps(r, ensure_ascii=False))
"""


def engine_python(root: Path) -> Path:
    for p in (root / "venv" / "bin" / "python", root / "venv" / "Scripts" / "python.exe"):
        if p.is_file():
            return p
    return Path(sys.executable)


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: transcribe.py <file>")
    src = Path(os.path.expanduser(sys.argv[1])).resolve()
    if not src.is_file():
        sys.exit(f"file not found: {src}")
    home = Path(os.environ.get("HERMES_HOME", Path.home() / ".hermes"))
    # Profile home is <hermes>/profiles/<name>; the engine lives in <hermes>/hermes-agent.
    hermes = home.parents[1] if home.parent.name == "profiles" else home
    root = hermes / "hermes-agent"
    out = subprocess.run(
        [str(engine_python(root)), "-c", CODE, str(src)],
        cwd=root, capture_output=True, text=True,
        env={**os.environ, "PYTHONPATH": str(root), "HERMES_HOME": str(home)},
    )
    line = next((l for l in reversed(out.stdout.splitlines()) if l.startswith("{")), "")
    try:
        r = json.loads(line)
    except ValueError:
        sys.exit("transcription failed: " + (out.stderr or out.stdout)[-400:])
    text = (r.get("transcript") or r.get("text") or "").strip()
    if not text:
        sys.exit("transcription failed: " + str(r.get("error") or "no speech found"))
    print(text)


if __name__ == "__main__":
    main()
