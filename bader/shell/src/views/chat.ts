// Chat view — DOM port of PromptView / ChatBubble / TypingDotsView from
// IslandViewContent.swift.

import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, onEvent, type ChatContext, type RunEvent, type SnapshotInfo } from "../core/bridge";
import { idleLines, initFace, setFace, showAnswer, stripIdle, stripNow, toolLabel } from "./facescreen";
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
  return h("div", { class: "chat-row" }, h("div", { class: "reply", text: plain(message.content), dir: "auto" }));
}

function typingDots(status: string): HTMLElement {
  return h(
    "div",
    { class: "chat-row" },
    h("div", { class: "typing" }, h("i"), h("i"), h("i")),
    status ? h("div", { class: "run-status", text: status, dir: "auto" }) : null,
  );
}

/** Replies are shown as plain text: drop markdown emphasis and headings. */
function plain(text: string): string {
  return text
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/__(.+?)__/g, "$1")
    .replace(/^#{1,6}\s+/gm, "")
    .replace(/`([^`]+)`/g, "$1");
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
  let runStatus = "";
  const approvalRow = h("div", { class: "approval-row" });
  approvalRow.style.display = "none";
  log.after(approvalRow);

  // ── Live run events: what Bader is doing, and approvals ──
  function showApproval(what: string) {
    clear(approvalRow);
    const yes = h("button", { class: "approve-btn", text: "Approve · موافق" }) as HTMLButtonElement;
    const no = h("button", { class: "deny-btn", text: "Deny · رفض" }) as HTMLButtonElement;
    const answer = async (choice: "once" | "deny") => {
      yes.disabled = no.disabled = true;
      try {
        await Bridge.runApprove(choice);
      } catch (err) {
        console.error("[bader] approval failed", err);
      }
    };
    yes.addEventListener("click", () => void answer("once"));
    no.addEventListener("click", () => void answer("deny"));
    approvalRow.append(
      h("div", { class: "approval-text" },
        h("b", { text: "Bader needs your OK · بدر يحتاج موافقتك" }),
        h("span", { text: what, dir: "auto" }),
        h("small", { text: "Or press the button on Bader's screen (hold = deny)" }),
      ),
      h("div", { class: "approval-actions" }, yes, no),
    );
    approvalRow.style.display = "";
    onHeightChange();
    setFace("approval");
    stripNow([
      { text: what, color: "#ffc440", size: 14 },
      { text: "Press = Yes · Hold = No", color: "#96a0aa", size: 13 },
    ]);
    void Bridge.faceLed(255, 150, 0, true);
    Sound.play("approval");
  }

  function hideApproval(choice?: string | null) {
    if (approvalRow.style.display === "none") return;
    approvalRow.style.display = "none";
    clear(approvalRow);
    onHeightChange();
    void Bridge.faceLed(0, 0, 0);
    if (choice === "deny") {
      setFace("concerned", 3);
      stripNow([{ text: "Cancelled · أُلغي", color: "#f4505e" }]);
    } else if (choice) {
      setFace("working");
      stripNow([{ text: "Approved · تمت الموافقة", color: "#50dc78" }]);
    }
  }

  void onEvent<RunEvent>("bader-run", (ev) => {
    if (ev.kind === "tool") {
      const label = toolLabel(ev.tool, ev.text);
      runStatus = label.en;
      setFace("working");
      stripNow([
        { text: label.en, color: "#ffc440", size: 15 },
        { text: label.ar, color: "#ebeef2", size: 15 },
      ]);
      renderedCount = -1;
      State.notify();
    } else if (ev.kind === "approval") {
      showApproval(ev.text ?? "An action needs your approval");
    } else if (ev.kind === "approval-resolved") {
      hideApproval(ev.text);
    }
  });

  // ── Idle screen: next meeting + unread mail, refreshed by the background sync ──
  const paintIdle = (info: SnapshotInfo | null) => stripIdle(idleLines(info), info?.unread ?? 0);
  void Bridge.snapshotInfo().then((i) => paintIdle(i ?? null));
  void onEvent<SnapshotInfo>("bader-snapshot", (i) => paintIdle(i));
  window.setInterval(() => void Bridge.snapshotInfo().then((i) => paintIdle(i ?? null)), 60_000);

  // ── The button on Bader's screen: short press = talk to Bader ──
  void onEvent<string>("face-button", (kind) => {
    if (kind === "short") void toggleMic();
  });

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

  // ── Speech: sentences are voiced as soon as they are complete, while the
  // rest of the answer is still streaming; the next one is prepared during playback.
  let streamText = "";
  let spokenUpTo = 0;
  let speechOn = false;
  let streaming = false;
  let playing = false;
  let speechGen = 0;
  const synthQueue: Promise<string | null>[] = [];

  function stopSpeech() {
    speechGen++;
    synthQueue.length = 0;
    player?.pause();
    player = null;
    playing = false;
  }

  function playUrl(url: string, gen: number): Promise<void> {
    return new Promise((resolve) => {
      if (gen !== speechGen) return resolve();
      player = new Audio(url);
      player.onended = () => resolve();
      player.onpause = () => resolve();
      player.onerror = () => resolve();
      void player.play().catch(() => resolve());
    });
  }

  async function playLoop() {
    if (playing) return;
    playing = true;
    const gen = speechGen;
    setFace("speaking");
    while (synthQueue.length && gen === speechGen) {
      const url = await synthQueue.shift()!;
      if (url && gen === speechGen) await playUrl(url, gen);
    }
    if (gen === speechGen) {
      playing = false;
      if (!streaming) setFace("happy", 3);
    }
  }

  function enqueueSpeech(segment: string) {
    const t = plain(segment).trim();
    if (!t) return;
    synthQueue.push(Bridge.voiceSpeak(t).catch(() => null));
    void playLoop();
  }

  /** Voices every complete sentence received so far (or the rest, when final). */
  function speakProgress(final: boolean) {
    if (!speechOn) return;
    const rest = streamText.slice(spokenUpTo);
    if (final) {
      spokenUpTo = streamText.length;
      enqueueSpeech(rest);
      return;
    }
    let cut = -1;
    const re = /(?<!\d)[.!?؟](?=\s|$)|\n/g; // "1." list numbers are not sentence ends
    let m: RegExpExecArray | null;
    while ((m = re.exec(rest))) {
      if (m.index + 1 >= 24) cut = m.index + 1;
    }
    if (cut > 0) {
      spokenUpTo += cut;
      enqueueSpeech(rest.slice(0, cut));
    }
  }

  void onEvent<string>("bader-delta", (d) => {
    if (!sending) return;
    streamText += d;
    streaming = true;
    renderedCount = -1;
    State.notify();
    speakProgress(false);
  });
  void onEvent<null>("bader-delta-reset", () => {
    stopSpeech();
    streamText = "";
    spokenUpTo = 0;
    renderedCount = -1;
    State.notify();
  });

  async function toggleMic() {
    if (sending || listening) return;
    if (!recording) {
      try {
        stopSpeech();
        await Bridge.voiceStart();
        recording = true;
        setFace("listening");
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
    setFace("thinking");
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

  // ── Keep the island open while typing, waiting, talking or reading ──
  let readUntil = 0;
  State.chatHold = () =>
    sending || recording || listening || playing ||
    (document.activeElement === input && input.value.trim() !== "") ||
    performance.now() < readUntil;

  // ── Push-to-talk: hold Control+Option (Mac) / Ctrl+Alt (Windows) ──
  // ── Touch screen: Talk / Send / Cancel / Stop / Approve / tap a mail or meeting ──
  initFace({
    talk: () => {
      window.dispatchEvent(new Event("bader-open-chat"));
      void toggleMic();
    },
    cancelTalk: () => {
      if (!recording) return;
      recording = false;
      void Bridge.voiceCancel();
      input.placeholder = "";
      setMic();
      setFace("idle");
    },
    stopSpeaking: () => {
      stopSpeech();
      setFace("idle");
    },
    approve: (choice) => void Bridge.runApprove(choice).catch((err) => console.error("[bader] approval failed", err)),
    ask: (query) => {
      if (sending || recording || listening) return;
      window.dispatchEvent(new Event("bader-open-chat"));
      input.value = query;
      void submit(true);
    },
    busy: () => sending || recording || listening,
  });

  let pttStarted = false;
  void onEvent<string>("ptt", (kind) => {
    if (kind === "down") {
      window.dispatchEvent(new Event("bader-open-chat"));
      if (!recording) {
        pttStarted = true;
        void toggleMic();
      }
    } else if (kind === "up") {
      if (pttStarted && recording) void toggleMic();
      pttStarted = false;
    } else if (kind === "tap") {
      window.dispatchEvent(new Event("bader-open-chat"));
      void toggleMic();
    }
  });

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
    stopSpeech();
    streamText = "";
    spokenUpTo = 0;
    streaming = false;
    speechOn = byVoice || speakReplies;
    Sound.play("send");

    State.chatHistory.push({ id: nextId++, role: "user", content: query });
    State.stateOverride = "thinking";
    State.notify();
    onHeightChange();
    runStatus = "";
    setFace("thinking");

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
      readUntil = performance.now() + 30_000;
      showAnswer(plain(reply.text));
      if (State.mode !== "expanded" || !document.hasFocus()) {
        const first = plain(reply.text).replace(/\s+/g, " ").trim();
        void Bridge.notify("Bader", first.length > 140 ? first.slice(0, 137) + "…" : first).catch(() => {});
      }
      window.dispatchEvent(new Event("bader-open-chat"));
      State.chatHistory.push({ id: nextId++, role: "assistant", content: reply.text });
      State.stateOverride = null;
      Sound.play("finish");
      runStatus = "";
      hideApproval(null);
      streaming = false;
      if (speechOn) {
        const finalText = reply.text;
        if (spokenUpTo > 0 && finalText.trim().startsWith(streamText.slice(0, spokenUpTo).trim())) {
          streamText = finalText; // finish from where the streamed speech stopped
          speakProgress(true);
        } else if (spokenUpTo === 0) {
          streamText = finalText;
          speakProgress(true);
        } else {
          speakProgress(true);
        }
        if (!playing && !synthQueue.length) setFace("happy", 3);
      } else {
        setFace("happy", 3);
      }
      streamText = "";
    } catch (err) {
      runStatus = "";
      hideApproval(null);
      setFace("concerned", 6);
      stripNow([{ text: String(err).replace(/^Error:\s*/, "").slice(0, 60), color: "#f4505e", size: 13 }]);
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
        if (thinking && streamText.trim()) {
          log.append(h("div", { class: "chat-row" }, h("div", { class: "reply streaming", text: plain(streamText), dir: "auto" })));
        } else if (thinking) log.append(typingDots(runStatus));
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
