"""Speed of candidate engine models on OpenRouter (tokens/s, seconds, cost)."""
import json, time, urllib.request, pathlib, sys

env = dict(l.split("=", 1) for l in (pathlib.Path.home() / ".hermes/profiles/bader/.env").read_text().splitlines()
           if "=" in l and not l.startswith("#"))
k = env["OPENROUTER_API_KEY"].strip().strip('"')
models = sys.argv[1:] or ["minimax/minimax-m3", "anthropic/claude-haiku-4.5", "anthropic/claude-sonnet-4.5", "google/gemini-2.5-flash"]
for m in models:
    for _ in range(2):
        t = time.time()
        try:
            req = urllib.request.Request("https://openrouter.ai/api/v1/chat/completions",
                data=json.dumps({"model": m, "max_tokens": 700, "usage": {"include": True},
                                 "messages": [{"role": "user", "content": "Write a 450-word executive memo about a travel policy."}]}).encode(),
                headers={"Authorization": "Bearer " + k, "Content-Type": "application/json"})
            r = json.load(urllib.request.urlopen(req, timeout=150))
            dt = time.time() - t; n = r["usage"]["completion_tokens"]
            print(f"{m:32s} {dt:5.1f}s {n:4d} tok {n/dt:5.0f} tok/s cost={r['usage'].get('cost')} via {r.get('provider')}", flush=True)
        except Exception as e:
            print(m, "ERR", e, flush=True)
