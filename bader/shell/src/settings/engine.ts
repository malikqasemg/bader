// Settings sections for the Bader engine: AI provider/model/key and voice
// (speech-to-text + spoken replies, Arabic and English). Values are written
// into the engine profile by Rust; API keys never come back to this page.

import { Bridge, type EngineStatus } from "../core/bridge";
import { h, clear } from "../views/dom";

function dot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

function select(options: [string, string][], value: string): HTMLSelectElement {
  const el = h("select", {}) as HTMLSelectElement;
  for (const [v, label] of options) el.append(h("option", { value: v, text: label }));
  if (value && !options.some(([v]) => v === value)) el.append(h("option", { value, text: value }));
  el.value = value || options[0][0];
  return el;
}

function notice(box: HTMLElement, ok: boolean, text: string) {
  clear(box);
  box.append(h("div", { class: ok ? "notice ok" : "notice err", text }));
}

// ── AI ────────────────────────────────────────────────────────────────────────

interface ProviderDef {
  label: string;
  envKey: string;
  baseUrl: string;
  keyHint: string;
  modelHint: string;
}

const PROVIDERS: Record<string, ProviderDef> = {
  openrouter: {
    label: "OpenRouter (any model)",
    envKey: "OPENROUTER_API_KEY",
    baseUrl: "https://openrouter.ai/api/v1",
    keyHint: "sk-or-...",
    modelHint: "e.g. minimax/minimax-m3",
  },
  openai: {
    label: "OpenAI",
    envKey: "OPENAI_API_KEY",
    baseUrl: "https://api.openai.com/v1",
    keyHint: "sk-...",
    modelHint: "OpenAI model name",
  },
  anthropic: {
    label: "Anthropic (Claude)",
    envKey: "ANTHROPIC_API_KEY",
    baseUrl: "https://api.anthropic.com",
    keyHint: "sk-ant-...",
    modelHint: "Claude model name",
  },
};

export function aiSection(status: EngineStatus): HTMLElement {
  const current = status.values["model.provider"] || "openrouter";
  const provider = select(
    Object.entries(PROVIDERS).map(([id, p]) => [id, p.label]),
    current,
  );
  const key = h("input", {
    type: "password",
    autocomplete: "off",
    spellcheck: "false",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const model = h("input", {
    type: "text",
    spellcheck: "false",
    value: status.values["model.default"] ?? "",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const state = h("span", { class: "hint" });
  const head = dot(false);
  const feedback = h("div", {});

  const refreshHints = () => {
    const p = PROVIDERS[provider.value];
    if (!p) return;
    const has = status.keys[p.envKey] ?? false;
    key.placeholder = has ? "••••••••••••  (stored in the engine)" : p.keyHint;
    model.placeholder = p.modelHint;
    head.style.background = has && status.running ? "#22c55e" : "#f4505e";
    state.textContent = !status.found
      ? "Engine not found on this computer."
      : !has
        ? `No ${p.label} key yet.`
        : status.running
          ? "Engine running with this provider."
          : "Key saved — engine is not running.";
  };
  provider.addEventListener("change", refreshHints);
  refreshHints();

  const apply = h("button", { class: "primary", text: "Apply & restart engine" }) as HTMLButtonElement;
  apply.addEventListener("click", async () => {
    const p = PROVIDERS[provider.value];
    const values: Record<string, string> = {
      "model.provider": provider.value,
      "model.base_url": p.baseUrl,
    };
    if (model.value.trim()) values["model.default"] = model.value.trim();
    const secrets: Record<string, string> = {};
    if (key.value.trim()) secrets[p.envKey] = key.value.trim();
    apply.disabled = true;
    notice(feedback, true, "Saving and restarting the engine…");
    try {
      await Bridge.engineApply(values, secrets, true);
      key.value = "";
      if (secrets[p.envKey]) status.keys[p.envKey] = true;
      status.values["model.provider"] = provider.value;
      notice(feedback, true, "Saved. The engine restarts in a few seconds.");
      window.setTimeout(async () => {
        const s = await Bridge.engineStatus();
        if (s) {
          status.running = s.running;
          refreshHints();
        }
      }, 8000);
    } catch (err) {
      notice(feedback, false, `Could not save: ${String(err)}`);
    } finally {
      apply.disabled = false;
      refreshHints();
    }
  });

  return h(
    "section",
    {},
    h("h2", {}, head, h("span", { text: "AI  ·  الذكاء الاصطناعي" })),
    state,
    h("div", { class: "row" }, h("label", { text: "Provider" }), provider),
    h("div", { class: "row" }, h("label", { text: "API key" }), key),
    h("div", { class: "row" }, h("label", { text: "Model" }), model),
    h("div", { class: "row" }, apply),
    feedback,
  );
}

// ── Voice ─────────────────────────────────────────────────────────────────────

const VOICES: [string, string][] = [
  ["en-US-AndrewMultilingualNeural", "Andrew — Arabic + English (male)"],
  ["en-US-AvaMultilingualNeural", "Ava — Arabic + English (female)"],
  ["ar-SA-HamedNeural", "Hamed — Saudi Arabic (male)"],
  ["ar-SA-ZariyahNeural", "Zariyah — Saudi Arabic (female)"],
  ["en-US-GuyNeural", "Guy — English (male)"],
  ["en-US-AriaNeural", "Aria — English (female)"],
];

export function voiceSection(status: EngineStatus): HTMLElement {
  const stt = select(
    [
      ["local", "On this computer (free, private)"],
      ["openai", "OpenAI Whisper (cloud, needs key)"],
    ],
    status.values["stt.provider"] || "local",
  );
  const size = select(
    [
      ["base", "Fast (base)"],
      ["small", "Better Arabic (small)"],
      ["medium", "Best Arabic (medium, slower)"],
    ],
    status.values["stt.local.model"] || "small",
  );
  const voice = select(VOICES, status.values["tts.edge.voice"] || VOICES[0][0]);
  const spoken = h("input", { type: "checkbox" }) as HTMLInputElement;
  spoken.checked = (status.values["voice.auto_tts"] ?? "false") === "true";
  const openaiKey = h("input", {
    type: "password",
    autocomplete: "off",
    spellcheck: "false",
    placeholder: status.keys.VOICE_TOOLS_OPENAI_KEY ? "••••••••••••  (stored)" : "sk-... (only for OpenAI Whisper)",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const sizeRow = h("div", { class: "row" }, h("label", { text: "Accuracy" }), size);
  const keyRow = h("div", { class: "row" }, h("label", { text: "OpenAI key" }), openaiKey);
  const syncRows = () => {
    sizeRow.style.display = stt.value === "local" ? "" : "none";
    keyRow.style.display = stt.value === "openai" ? "" : "none";
  };
  stt.addEventListener("change", syncRows);
  syncRows();

  const feedback = h("div", {});
  const apply = h("button", { class: "primary", text: "Apply & restart engine" }) as HTMLButtonElement;
  apply.addEventListener("click", async () => {
    const values: Record<string, string> = {
      "stt.enabled": "true",
      "stt.provider": stt.value,
      // Empty = detect Arabic or English for each message.
      "stt.language": "",
      "tts.provider": "edge",
      "tts.edge.voice": voice.value,
      "voice.auto_tts": spoken.checked ? "true" : "false",
    };
    if (stt.value === "local") values["stt.local.model"] = size.value;
    const secrets: Record<string, string> = {};
    if (openaiKey.value.trim()) secrets.VOICE_TOOLS_OPENAI_KEY = openaiKey.value.trim();
    apply.disabled = true;
    notice(feedback, true, "Saving and restarting the engine…");
    try {
      await Bridge.engineApply(values, secrets, true);
      openaiKey.value = "";
      notice(feedback, true, "Saved. Voice notes in Arabic and English are understood automatically.");
    } catch (err) {
      notice(feedback, false, `Could not save: ${String(err)}`);
    } finally {
      apply.disabled = false;
    }
  });

  return h(
    "section",
    {},
    h("h2", {}, dot(status.found), h("span", { text: "Voice  ·  الصوت" })),
    h("span", {
      class: "hint",
      text: "Bader understands voice notes in Arabic and English, and can answer with a voice.",
    }),
    h("div", { class: "row" }, h("label", { text: "Listening" }), stt),
    sizeRow,
    keyRow,
    h("div", { class: "row" }, h("label", { text: "Bader's voice" }), voice),
    h("div", { class: "row" }, h("label", { text: "Spoken replies" }), spoken),
    h("div", { class: "row" }, apply),
    feedback,
  );
}
