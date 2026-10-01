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


AR_DEFAULT = "ar-SA-HamedNeural"
EN_DEFAULT = "en-US-AndrewMultilingualNeural"


def _voices():
    """Arabic + English voices: bader_voice.json in the engine profile, else defaults."""
    home = os.environ.get("HERMES_HOME", "")
    ar, en = AR_DEFAULT, None
    try:
        with open(os.path.join(home, "bader_voice.json"), encoding="utf-8") as f:
            cfg = json.load(f)
        ar = cfg.get("ar") or ar
        en = cfg.get("en") or None
    except Exception:
        pass
    if not en:
        try:
            from tools.tts_tool import _load_tts_config
            en = ((_load_tts_config() or {}).get("edge") or {}).get("voice") or EN_DEFAULT
        except Exception:
            en = EN_DEFAULT
    return ar, en


def _segments(text):
    """Splits text into sentences and gives each the voice of its main language:
    mostly-Arabic sentences get the Arabic voice (which also reads names like
    "Cisco" naturally), mostly-English sentences the English voice."""
    import re
    arabic = re.compile(r"[\u0600-\u06FF\u0750-\u077F\u08A0-\u08FF\uFB50-\uFDFF\uFE70-\uFEFF]")
    latin = re.compile(r"[A-Za-z]")
    parts = re.split(r"(?<=[.!?\u061F\n])\s+", text)
    out = []
    for part in parts:
        if not part.strip():
            continue
        is_ar = len(arabic.findall(part)) >= len(latin.findall(part))
        if out and out[-1][0] == is_ar:
            out[-1] = (is_ar, out[-1][1] + " " + part.strip())
        else:
            out.append((is_ar, part.strip()))
    return out


def _edge_tts(text, out_path):
    import asyncio
    import edge_tts

    ar_voice, en_voice = _voices()

    async def run():
        with open(out_path, "wb") as out:
            for is_ar, seg in _segments(text):
                voice = ar_voice if is_ar else en_voice
                async for chunk in edge_tts.Communicate(seg, voice).stream():
                    if chunk.get("type") == "audio":
                        out.write(chunk["data"])

    asyncio.run(run())
    return out_path


def tts(text_path, out_path):
    with open(text_path, encoding="utf-8") as f:
        text = f.read()
    try:
        from tools.tts_text_normalize import prepare_spoken_text
        text = prepare_spoken_text(text, max_chars=None)
    except Exception:
        text = text.strip()
    provider = "edge"
    try:
        from tools.tts_tool import _load_tts_config
        provider = (_load_tts_config() or {}).get("provider") or "edge"
    except Exception:
        pass
    if provider == "edge":
        # Native voice per language: Arabic parts in an Arabic voice, English in English.
        _edge_tts(text, out_path)
        ok = os.path.isfile(out_path) and os.path.getsize(out_path) > 0
        _emit({"ok": ok, "file": out_path, "error": None if ok else "No audio produced."})
        return
    from tools.tts_tool import text_to_speech_tool
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
