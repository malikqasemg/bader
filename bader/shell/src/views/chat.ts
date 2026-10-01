// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, type ChatContext } from "../core/bridge";
import { Sound } from "../core/sound";
import { State, type ChatMessage } from "../core/state";
import type { ViewHost } from "./views";

let nextId = 1;

function bubble(message: ChatMessage): HTMLElement {
  if (message.role === "user") {
    return h(
      "div",
      { class: "chat-row user" },
      h("div", { class: "bubble", text: message.content, dir: "auto" }),
    );
  }
  return h("div", { class: "chat-row" }, h("div", { class: "reply", text: message.content, dir: "auto" }));
}

function typingDots(): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
  );
}

/** The coloured chip showing what the question is about (a dropped file). */
function contextChip(label: string): HTMLElement {
  const chip = h("div", { class: "chip" }, h("i", { class: "chip-dot" }), h("span", { text: label }));
  requestAnimationFrame(() => chip.classList.add("settled"));
  return chip;
}

export function buildPrompt(onHeightChange: () => void): ViewHost {
  const chipRow = h("div", { class: "chip-row" });
  const log = h("div", { class: "chat-log" });
  const input = h("input", {
    type: "text",
    class: "chat-input",
    placeholder: "Ask me anything…",
    spellcheck: "false",
    dir: "auto",
  }) as HTMLInputElement;
  const send = h("button", { class: "send-btn", title: "Send" }, svg(ICONS.arrowUp, 11));
  const mic = h("button", { class: "mic-btn", title: "Talk to Bader · تحدث مع بدر" }, svg(ICONS.mic, 13));
  const look = h("button", { class: "mic-btn", title: "Look at my screen · انظر إلى شاشتي" }, svg(ICONS.eye, 13));
  const bar = h("div", { class: "chat-bar" }, input, look, mic, send);

  const el = h(
    "div",
    { class: "view" },
    h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body" }, chipRow, log, bar)),
  );
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");

  let sending = false;
  let renderedCount = -1;

  // ── Voice: tap mic to talk, tap again to send. Replies are spoken back. ──
  let recording = false;
  let listening = false; // transcribing
  let speakReplies = true;
  let player: HTMLAudioElement | null = null;
  void Bridge.engineStatus().then((s) => {
    // The Voice setting "Spoken replies" decides; mic use always gets a spoken reply.
    if (s) speakReplies = (s.values["voice.auto_tts"] ?? "").toLowerCase() === "true";
  });

  function setMic() {
    mic.classList.toggle("recording", recording);
    mic.classList.toggle("busy", listening);
    clear(mic);
    mic.append(svg(recording ? ICONS.stop : ICONS.mic, recording ? 10 : 13));
  }

  async function speak(text: string) {
    try {
      const url = await Bridge.voiceSpeak(text);
      player?.pause();
      player = new Audio(url);
      void player.play();
    } catch (err) {
      console.error("[bader] speak failed", err);
    }
  }

  async function toggleMic() {
    if (sending || listening) return;
    if (!recording) {
      try {
        player?.pause();
        await Bridge.voiceStart();
        recording = true;
        input.placeholder = "Listening… tap ■ to send · أستمع…";
      } catch (err) {
        State.noteMessage = String(err).replace(/^Error:\s*/, "");
        State.view = "note";
        State.notify();
      }
      setMic();
      return;
    }
    recording = false;
    listening = true;
    input.placeholder = "Understanding… · جارٍ الفهم…";
    setMic();
    try {
      const heard = await Bridge.voiceStop();
      listening = false;
      setMic();
      if (heard.text) {
        input.value = heard.text;
        await submit(true);
      }
    } catch (err) {
      listening = false;
      setMic();
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      State.notify();
    }
  }
  mic.addEventListener("click", () => void toggleMic());

  // ── Look: the next question carries a screenshot of the screen. ──
  let looking = false;
  look.addEventListener("click", () => {
    looking = !looking;
    look.classList.toggle("look-on", looking);
  });

  async function submit(byVoice = false) {
    const query = input.value.trim();
    if (!query || sending) return;
    input.value = "";
    sending = true;
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();

    const file = State.droppedFile;
    let context: ChatContext | null =
      State.chatHistory.length === 1 && file ? { kind: "file", name: file.name, path: file.path } : null;
    if (looking) {
      try {
        context = { kind: "screen", path: await Bridge.captureScreen() };
      } catch (err) {
        State.noteMessage = String(err).replace(/^Error:\s*/, "");
      }
      looking = false;
      look.classList.remove("look-on");
    }

    try {
      const reply = await Bridge.chatSend(query, context);
      State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      State.stateOverride = null;
      Sound.play("finish");
      if (byVoice || speakReplies) void speak(reply.text);
    } catch (err) {
      State.stateOverride = null;
      State.noteMessage = String(err).replace(/^Error:\s*/, "");
      State.view = "note";
      Sound.play("error");
    } finally {
      sending = false;
      State.notify();
      onHeightChange();
      input.focus();
    }
  }

  send.addEventListener("click", () => void submit());
  input.addEventListener("keydown", (e) => {
    if ((e as KeyboardEvent).key === "Enter") {
      e.preventDefault();
      void submit();
    }
    e.stopPropagation(); // Escape closes the island, not the chat
  });

  return {
    el,
    sync() {
      const file = State.droppedFile;
      const wantChip = file?.name ?? "";
      if (chipRow.dataset.label !== wantChip) {
        chipRow.dataset.label = wantChip;
        clear(chipRow);
        if (wantChip) chipRow.append(contextChip(wantChip));
      }

      const thinking = State.stateOverride === "thinking";
      const count = State.chatHistory.length + (thinking ? 0.5 : 0);
      if (count !== renderedCount) {
        renderedCount = count;
        clear(log);
        for (const m of State.chatHistory) log.append(bubble(m));
        if (thinking) log.append(typingDots());
        log.scrollTop = log.scrollHeight;
      }

      if (!recording && !listening) {
        input.placeholder = State.chatHistory.length === 0 ? "Ask me anything… · اسألني أي شيء…" : "Continue… · تابع…";
      }
      input.disabled = sending;
    },
    focus() {
      input.focus();
      input.select();
    },
  };
}
