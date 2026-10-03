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
    modelHint: "recommended: anthropic/claude-haiku-4.5 (fast)",
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
  const quick = h("input", { type: "checkbox" }) as HTMLInputElement;
  quick.checked = (status.values["bader.quick_lane"] ?? "true") !== "false";
  const quickModel = h("input", {
    type: "text",
    spellcheck: "false",
    value: status.values["bader.quick_model"] ?? "",
    placeholder: "default: anthropic/claude-haiku-4.5",
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
      "bader.quick_lane": quick.checked ? "true" : "false",
      "bader.quick_model": quickModel.value.trim(),
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
    h("div", { class: "row" }, h("label", { text: "Quick answers" }), quick),
    h("div", { class: "row" }, h("label", { text: "Quick model" }), quickModel),
    h("span", { class: "hint", text: "Quick answers: a fast model replies in 2–3 s; Bader's full engine takes over only when tools or actions are needed." }),
    h("div", { class: "row" }, apply),
    feedback,
  );
}

// ── Voice ─────────────────────────────────────────────────────────────────────

const AR_VOICES: [string, string][] = [
  ["ar-SA-HamedNeural", "Hamed — Saudi (male)"],
  ["ar-SA-ZariyahNeural", "Zariyah — Saudi (female)"],
  ["ar-AE-HamdanNeural", "Hamdan — Emirati (male)"],
  ["ar-AE-FatimaNeural", "Fatima — Emirati (female)"],
  ["ar-KW-FahedNeural", "Fahed — Kuwaiti (male)"],
  ["ar-QA-MoazNeural", "Moaz — Qatari (male)"],
  ["ar-EG-ShakirNeural", "Shakir — Egyptian (male)"],
];

const EN_VOICES: [string, string][] = [
  ["en-US-AndrewMultilingualNeural", "Andrew (male)"],
  ["en-US-AvaMultilingualNeural", "Ava (female)"],
  ["en-US-GuyNeural", "Guy (male)"],
  ["en-US-AriaNeural", "Aria (female)"],
  ["en-GB-RyanNeural", "Ryan — British (male)"],
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
      ["large-v3-turbo", "Most accurate, Arabic + English (needs a fast computer)"],
    ],
    status.values["stt.local.model"] || "small",
  );
  const voiceAr = select(AR_VOICES, status.values["bader.voice_ar"] || AR_VOICES[0][0]);
  const voice = select(EN_VOICES, status.values["bader.voice_en"] || status.values["tts.edge.voice"] || EN_VOICES[0][0]);
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
      "bader.voice_ar": voiceAr.value,
      "bader.voice_en": voice.value,
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
      text: "Bader understands Arabic and English, and answers each language in a native voice.",
    }),
    h("div", { class: "row" }, h("label", { text: "Listening" }), stt),
    sizeRow,
    keyRow,
    h("div", { class: "row" }, h("label", { text: "Arabic voice" }), voiceAr),
    h("div", { class: "row" }, h("label", { text: "English voice" }), voice),
    h("div", { class: "row" }, h("label", { text: "Spoken replies" }), spoken),
    h("div", { class: "row" }, apply),
    feedback,
  );
}

// ── Language & approvals ──────────────────────────────────────────────────────

const LANGS: [string, string][] = [
  ["auto", "Same as I write · نفس لغتي"],
  ["en", "English only"],
  ["ar", "العربية فقط · Arabic only"],
];

export function languageSection(status: EngineStatus): HTMLElement {
  const answer = select(LANGS, status.values["bader.answer_lang"] || "auto");
  const summary = select(LANGS, status.values["bader.summary_lang"] || "auto");
  const approvals = h("input", { type: "checkbox" }) as HTMLInputElement;
  approvals.checked = (status.values["bader.approvals"] ?? "true") !== "false";
  const screenReply = select(
    [
      ["ask", "Ask me each time · اسألني كل مرة"],
      ["text", "Text only · نص فقط"],
      ["voice", "Text + voice · نص وصوت"],
    ],
    status.values["bader.screen_reply"] || "ask",
  );
  const feedback = h("div", {});
  const apply = h("button", { class: "primary", text: "Apply" }) as HTMLButtonElement;
  apply.addEventListener("click", async () => {
    apply.disabled = true;
    notice(feedback, true, "Saving…");
    try {
      await Bridge.engineApply(
        {
          "bader.answer_lang": answer.value,
          "bader.summary_lang": summary.value,
          "bader.approvals": approvals.checked ? "true" : "false",
          "bader.screen_reply": screenReply.value,
        },
        {},
        true,
      );
      notice(feedback, true, "Saved. The island uses it now; Telegram / WhatsApp after the engine restarts (a few seconds).");
    } catch (err) {
      notice(feedback, false, `Could not save: ${String(err)}`);
    } finally {
      apply.disabled = false;
    }
  });
  return h(
    "section",
    {},
    h("h2", {}, dot(status.found), h("span", { text: "Language & approvals  ·  اللغة والموافقات" })),
    h("span", { class: "hint", text: "Ask in Arabic, get the answer in English (or the other way round)." }),
    h("div", { class: "row" }, h("label", { text: "Answers" }), answer),
    h("div", { class: "row" }, h("label", { text: "Meeting summaries" }), summary),
    h("div", { class: "row" }, h("label", { text: "Screen buttons answer" }), screenReply),
    h("span", { class: "hint", text: "For Bader's touch screen: what happens when you tap Brief, an email or a meeting." }),
    h("div", { class: "row" }, h("label", { text: "Ask before sending" }), approvals),
    h("span", { class: "hint", text: "When on, Bader asks you before sending or deleting mail and changing your calendar." }),
    h("div", { class: "row" }, apply),
    feedback,
  );
}

// ── Phone (Telegram) ──────────────────────────────────────────────────────────

/** Talk to Bader from a phone through a private Telegram bot. */
export function phoneSection(status: EngineStatus): HTMLElement {
  const hasToken = status.keys["TELEGRAM_BOT_TOKEN"] ?? false;
  const hasUser = status.keys["TELEGRAM_ALLOWED_USERS"] ?? false;
  const token = h("input", {
    type: "password",
    autocomplete: "off",
    spellcheck: "false",
    placeholder: hasToken ? "••••••••••••  (saved)" : "123456789:AA…  (from @BotFather)",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const user = h("input", {
    type: "text",
    inputmode: "numeric",
    spellcheck: "false",
    placeholder: hasUser ? "(saved)" : "your Telegram user ID, e.g. 123456789",
    style: "flex:1 1 auto;min-width:0",
  }) as HTMLInputElement;
  const feedback = h("div", {});
  const apply = h("button", { class: "primary", text: "Connect & restart engine" }) as HTMLButtonElement;
  apply.addEventListener("click", async () => {
    const secrets: Record<string, string> = {};
    if (token.value.trim()) secrets["TELEGRAM_BOT_TOKEN"] = token.value.trim();
    const ids = user.value.replace(/\s+/g, "");
    if (ids) {
      if (!/^\d+(,\d+)*$/.test(ids)) {
        notice(feedback, false, "The user ID is digits only (several IDs: separate with commas).");
        return;
      }
      secrets["TELEGRAM_ALLOWED_USERS"] = ids;
    }
    if (!Object.keys(secrets).length) {
      notice(feedback, false, "Nothing to save.");
      return;
    }
    if (!hasUser && !secrets["TELEGRAM_ALLOWED_USERS"]) {
      notice(feedback, false, "Add your Telegram user ID too — without it nobody is allowed to talk to the bot.");
      return;
    }
    apply.disabled = true;
    notice(feedback, true, "Saving…");
    try {
      await Bridge.engineApply({}, secrets, true);
      token.value = "";
      notice(feedback, true, "Saved. Open your bot in Telegram and send it a message.");
    } catch (err) {
      notice(feedback, false, `Could not save: ${String(err)}`);
    } finally {
      apply.disabled = false;
    }
  });
  return h(
    "section",
    {},
    h("h2", {}, dot(hasToken && hasUser), h("span", { text: "Phone (Telegram)  ·  الهاتف" })),
    h("span", {
      class: "hint",
      text: "Chat with Bader from your phone. 1) In Telegram, message @BotFather → /newbot → copy the token. 2) Message @userinfobot → copy your ID. 3) Paste both here. Only the IDs you list can talk to Bader.",
    }),
    h("div", { class: "row" }, h("label", { text: "Bot token" }), token),
    h("div", { class: "row" }, h("label", { text: "Your user ID" }), user),
    h("div", { class: "row" }, apply),
    feedback,
  );
}
