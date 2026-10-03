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


_SERVE = {"on": False, "out": None}


def _emit(obj):
    if _SERVE["on"]:
        # Worker mode: one tagged line per answer (library chatter goes elsewhere).
        _SERVE["out"].write("@@" + json.dumps(obj, ensure_ascii=False) + "\n")
        _SERVE["out"].flush()
        return
    sys.stdout.write("\n" + json.dumps(obj, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def serve():
    """Long-lived worker: one JSON request per stdin line, one "@@" answer line each.
    Keeps the speech model loaded, so each voice note skips start-up and model load."""
    _SERVE["on"] = True
    _SERVE["out"] = os.fdopen(os.dup(1), "w", encoding="utf-8")
    sys.stdout = sys.stderr  # anything else printed must not corrupt the answer stream
    try:  # warm up: load the speech model and voice libraries now, not on the first voice note
        import tempfile
        import wave
        import edge_tts  # noqa: F401
        warm = os.path.join(tempfile.gettempdir(), "bader_warm.wav")
        with wave.open(warm, "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(16000)
            w.writeframes(b"\x00\x00" * 8000)
        provider, name = _stt_config()
        if provider == "local":
            _local_stt(warm, name)
        else:
            from tools.transcription_tools import transcribe_audio
            transcribe_audio(warm)
    except Exception:
        pass
    _emit({"ok": True, "ready": True})
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            op = req.get("op")
            if op == "stt":
                stt(req["path"])
            elif op == "tts":
                tts(req["text_path"], req["out"])
            elif op == "ping":
                _emit({"ok": True})
            else:
                _emit({"ok": False, "error": f"unknown op {op}"})
        except Exception as exc:
            _emit({"ok": False, "error": f"{type(exc).__name__}: {exc}"})


_STT = {"name": None, "model": None}

OPENROUTER = "https://openrouter.ai/api/v1"
OR_STT_MODEL = "openai/whisper-large-v3"
# Gemini TTS speaks Arabic and English in one voice; it only returns raw PCM (24 kHz mono).
OR_TTS_MODEL = "google/gemini-3.8-flash-lite-tts"
OR_TTS_VOICE = "Charon"


def _prefs():
    try:
        with open(os.path.join(os.environ.get("HERMES_HOME", ""), "bader_prefs.json"), encoding="utf-8") as f:
            return json.load(f)
    except Exception:
        return {}


def _env_key(name):
    """A key from the engine profile's .env (the worker's own environment may not have it)."""
    if os.environ.get(name):
        return os.environ[name]
    try:
        with open(os.path.join(os.environ.get("HERMES_HOME", ""), ".env"), encoding="utf-8") as f:
            for line in f:
                k, _, v = line.partition("=")
                if k.strip() == name and v.strip():
                    return v.strip().strip('"')
    except OSError:
        pass
    return ""


def _openrouter(path, body, timeout=60):
    import urllib.request

    key = _env_key("OPENROUTER_API_KEY")
    if not key:
        raise RuntimeError("no OpenRouter key")
    req = urllib.request.Request(
        OPENROUTER + path, data=json.dumps(body).encode(),
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read()


def _openrouter_stt(path):
    """Cloud speech-to-text with the OpenRouter key (same one that answers)."""
    import base64

    with open(path, "rb") as f:
        audio = base64.b64encode(f.read()).decode()
    fmt = os.path.splitext(path)[1].lstrip(".").lower() or "wav"
    allowed = _languages()

    def ask(language=None):
        body = {"model": _prefs().get("or_stt_model") or OR_STT_MODEL, "input_audio": {"data": audio, "format": fmt}, "response_format": "verbose_json"}
        if language:
            body["language"] = language
        return json.loads(_openrouter("/audio/transcriptions", body))

    out = ask(allowed[0] if len(allowed) == 1 else None)
    names = {"arabic": "ar", "english": "en"}
    language = names.get(str(out.get("language") or "").lower(), str(out.get("language") or "").lower())
    if len(allowed) > 1 and language and language not in allowed:
        # Heard as some other language: ask again in the main language.
        out = ask(allowed[0])
        language = allowed[0]
    return (out.get("text") or "").strip(), language or allowed[0]


def _openrouter_tts(text, out_path):
    """Cloud voice with the OpenRouter key. Returns the audio file (a .wav next to out_path)."""
    import wave

    prefs = _prefs()
    pcm = _openrouter("/audio/speech", {
        "model": prefs.get("or_tts_model") or OR_TTS_MODEL,
        "input": text,
        "voice": prefs.get("or_voice") or OR_TTS_VOICE,
        "response_format": "pcm",
    })
    wav = os.path.splitext(out_path)[0] + ".wav"
    with wave.open(wav, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(24000)
        w.writeframes(pcm)
    return wav


def _stt_config():
    """(provider, model name) from the engine profile's Voice settings."""
    try:
        import yaml
        with open(os.path.join(os.environ.get("HERMES_HOME", ""), "config.yaml"), encoding="utf-8") as f:
            cfg = (yaml.safe_load(f) or {}).get("stt") or {}
        return cfg.get("provider") or "local", (cfg.get("local") or {}).get("model") or "small"
    except Exception:
        return "local", "small"


def _languages():
    """The languages Bader listens for: main + second language from set-up."""
    main, second = "en", "ar"
    try:
        with open(os.path.join(os.environ.get("HERMES_HOME", ""), "bader_prefs.json"), encoding="utf-8") as f:
            prefs = json.load(f)
        main = prefs.get("primary_lang") or main
        second = prefs.get("second_lang") or second
    except Exception:
        pass
    langs = [l for l in (main, second) if l in ("ar", "en")]
    return list(dict.fromkeys(langs)) or ["en", "ar"]


def _local_stt(path, name):
    """On-device speech-to-text, Arabic or English only.

    Whisper left to itself sometimes "hears" Urdu or Persian in accented English
    or Arabic; here the language is the more likely of the two Bader speaks."""
    from faster_whisper import WhisperModel

    if _STT["name"] != name:
        threads = max(2, min(12, (os.cpu_count() or 4) - 2))
        _STT["model"] = WhisperModel(name, device="cpu", compute_type="float32", cpu_threads=threads)
        _STT["name"] = name
    model = _STT["model"]
    allowed = _languages()
    # The assistant's name, so "Bader" is not heard as "better".
    opts = {"beam_size": 1, "vad_filter": True, "hotwords": "Bader بدر"}
    if len(allowed) == 1:  # one language only: no guessing at all
        segments, info = model.transcribe(path, language=allowed[0], **opts)
        return " ".join(s.text.strip() for s in segments).strip(), allowed[0]
    segments, info = model.transcribe(path, **opts)
    language = info.language
    if language not in allowed:
        probs = dict(info.all_language_probs or [])
        language = max(allowed, key=lambda l: probs.get(l, 0.0))
        segments, info = model.transcribe(path, language=language, **opts)
    return " ".join(s.text.strip() for s in segments).strip(), language


def stt(path):
    provider, name = _stt_config()
    text, language, error = "", "", None
    if _prefs().get("stt_cloud") == "openrouter":
        try:
            text, language = _openrouter_stt(path)
            _emit({"ok": bool(text), "text": text, "language": language,
                   "error": None if text else "Nothing was heard."})
            return
        except Exception as exc:  # offline, no credit…: listen on this computer instead
            print("openrouter stt failed:", exc, file=sys.stderr)
    if provider == "local":
        try:
            text, language = _local_stt(path, name)
        except Exception as exc:  # model missing, no memory…: use the engine's own path
            error = f"{type(exc).__name__}: {exc}"
            print("local stt failed:", error, file=sys.stderr)
            provider = "engine"
    if provider != "local":
        from tools.transcription_tools import transcribe_audio

        result = transcribe_audio(path, source="voice_mode")
        if isinstance(result, str):
            result = json.loads(result)
        text = (result.get("transcript") or result.get("text") or "").strip()
        language = result.get("language") or ""
        error = result.get("error")
    _emit({
        "ok": bool(text),
        "text": text,
        "language": language,
        "error": None if text else (error or "Nothing was heard."),
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
    if _prefs().get("tts_cloud") == "openrouter":
        try:
            wav = _openrouter_tts(text, out_path)
            if os.path.getsize(wav) > 1000:
                _emit({"ok": True, "file": wav, "error": None})
                return
        except Exception as exc:  # fall back to the free online voice
            print("openrouter tts failed:", exc, file=sys.stderr)
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
        if len(sys.argv) >= 2 and sys.argv[1] == "serve":
            serve()
        elif len(sys.argv) >= 3 and sys.argv[1] == "stt":
            stt(sys.argv[2])
        elif len(sys.argv) >= 4 and sys.argv[1] == "tts":
            tts(sys.argv[2], sys.argv[3])
        else:
            _emit({"ok": False, "error": "usage: stt <file> | tts <text-file> <out-file>"})
    except Exception as exc:  # report, never crash silently
        _emit({"ok": False, "error": f"{type(exc).__name__}: {exc}"})


if __name__ == "__main__":
    main()
