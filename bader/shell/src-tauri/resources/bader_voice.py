"""Bader voice helper — runs inside the Bader engine's Python environment.

Uses the engine's own speech-to-text and text-to-speech, so the Voice settings
(Listening provider, accuracy, Bader's voice) apply to the island too.

    python bader_voice.py stt <audio-file>             -> {"ok": true, "text": "...", "language": "ar"}
    python bader_voice.py tts <text-file> <out-file>   -> {"ok": true, "file": "<path>"}

Prints exactly one JSON line on the last line of stdout.
"""

import json
import os
import sys

# Run with the engine folder as working directory so its modules import.
sys.path.insert(0, os.getcwd())


def _emit(obj):
    sys.stdout.write("\n" + json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def stt(path):
    from tools.transcription_tools import transcribe_audio

    result = transcribe_audio(path, source="voice_mode")
    if isinstance(result, str):
        result = json.loads(result)
    text = (result.get("transcript") or result.get("text") or "").strip()
    ok = bool(result.get("success", bool(text))) and bool(text)
    _emit({
        "ok": ok,
        "text": text,
        "language": result.get("language") or "",
        "error": None if ok else (result.get("error") or "Nothing was heard."),
    })


def tts(text_path, out_path):
    from tools.tts_tool import text_to_speech_tool

    with open(text_path, encoding="utf-8") as f:
        text = f.read()
    raw = text_to_speech_tool(text, output_path=out_path)
    result = json.loads(raw) if isinstance(raw, str) else raw
    file = result.get("file_path") or result.get("path") or result.get("output_path") or out_path
    ok = bool(result.get("success", True)) and bool(file)
    _emit({"ok": ok, "file": file, "error": None if ok else result.get("error")})


def main():
    try:
        if len(sys.argv) >= 3 and sys.argv[1] == "stt":
            stt(sys.argv[2])
        elif len(sys.argv) >= 4 and sys.argv[1] == "tts":
            tts(sys.argv[2], sys.argv[3])
        else:
            _emit({"ok": False, "error": "usage: stt <file> | tts <text-file> <out-file>"})
    except Exception as exc:  # report, never crash silently
        _emit({"ok": False, "error": f"{type(exc).__name__}: {exc}"})


if __name__ == "__main__":
    main()
