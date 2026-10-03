// Bader's face screen (an ESP32 display on USB). Two kinds:
//
//   small  (172×320, one button): a face picture + a text strip at the bottom.
//   touch  (240×320, touch): status bar, face, live info, touch buttons and
//          pages (today's meetings, inbox, the full answer).
//
// The island draws all text on a canvas — so Arabic and English both render
// properly — and sends it as pictures (RGB565); the board reports touches.
//
// Touch layout (y): 0–27 status bar · 28–203 face (or a page down to 263)
//                   204–263 what Bader is doing · 264–319 buttons

import { Bridge, onEvent, type FaceInfo, type FaceName, type SnapshotLists } from "../core/bridge";

export interface StripLine {
  text: string;
  color?: string;
  size?: number;
  bold?: boolean;
}

/** What the chat lets the touch screen do. */
export interface FaceActions {
  talk: () => void;
  cancelTalk: () => void;
  stopSpeaking: () => void;
  approve: (choice: "once" | "deny") => void;
  ask: (query: string) => void;
  busy: () => boolean;
}

const FONT = '-apple-system, "SF Pro Rounded", "Segoe UI", "SF Arabic", Tahoma, sans-serif';
const INK = "#ebeef2";
const GREY = "#96a0aa";
const CYAN = "#40e0ff";
const AMBER = "#ffc440";
const GREEN = "#50dc78";
const RED = "#f4505e";
const PANEL = "#161c26";

let dev: FaceInfo | null = null;
const isTouch = () => !!dev && dev.proto >= 3 && dev.touch;

// ── Pixels ───────────────────────────────────────────────────────────────────

function toBase64(bytes: Uint8Array): string {
  let s = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    s += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(s);
}

/** Draws on a black w×h canvas and returns it as RGB565 (big-endian), base64. */
function paint(w: number, h: number, draw: (ctx: CanvasRenderingContext2D) => void): string {
  const c = document.createElement("canvas");
  c.width = w;
  c.height = h;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, w, h);
  ctx.textBaseline = "middle";
  draw(ctx);
  const px = ctx.getImageData(0, 0, w, h).data;
  const out = new Uint8Array(w * h * 2);
  for (let i = 0, j = 0; i < px.length; i += 4, j += 2) {
    const v = ((px[i] & 0xf8) << 8) | ((px[i + 1] & 0xfc) << 3) | (px[i + 2] >> 3);
    out[j] = v >> 8;
    out[j + 1] = v & 0xff;
  }
  return toBase64(out);
}

function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let t = text;
  while (t.length > 1 && ctx.measureText(t + "…").width > max) t = t.slice(0, -1);
  return t + "…";
}

const font = (size: number, bold = false) => `${bold ? 600 : 500} ${size}px ${FONT}`;
const isArabic = (s: string) => /[؀-ۿ]/.test(s);

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
  ctx.fill();
}

// ── Small screen (v2): one text strip ────────────────────────────────────────

const V2_W = 172;
const V2_Y = 266;
const V2_H = 54;

function v2Strip(lines: StripLine[]): string {
  return paint(V2_W, V2_H, (ctx) => {
    ctx.textAlign = "center";
    const rows = lines.slice(0, 2);
    rows.forEach((l, i) => {
      ctx.font = font(l.size ?? (rows.length === 1 ? 17 : 15), l.bold);
      ctx.fillStyle = l.color ?? INK;
      const y = rows.length === 1 ? V2_H / 2 : 14 + i * 26;
      ctx.fillText(fit(ctx, l.text, V2_W - 10), V2_W / 2, y);
    });
  });
}

// ── Touch screen (v3) ────────────────────────────────────────────────────────

const W = 240;
const BAR_H = 28;
const FACE_Y = 28;
const INFO_Y = 204;
const INFO_H = 60;
const BTN_Y = 264;
const BTN_H = 56;
const PAGE_H = INFO_Y + INFO_H - FACE_Y; // 236: a page covers the face and the info
const ROW_H = 40;
const HEAD_H = 30;
const ROWS = 5;

type Page = "home" | "agenda" | "inbox" | "answer";
interface Button {
  label: string;
  sub?: string;
  color: string;
  run: () => void;
}

const LABELS: Record<string, [string, string, string]> = {
  listening: ["Listening", "أستمع", CYAN],
  thinking: ["Thinking", "أفكر", AMBER],
  working: ["Working", "أعمل", AMBER],
  speaking: ["Speaking", "أتكلم", CYAN],
  happy: ["Done", "تم", GREEN],
  concerned: ["Problem", "في مشكلة", RED],
  surprised: ["New message", "رسالة جديدة", AMBER],
  celebrating: ["Hello!", "أهلاً", GREEN],
  approval: ["Approve?", "موافقة؟", AMBER],
  neutral: ["Ready", "جاهز", GREY],
};

const ui = {
  mode: "idle" as FaceName,
  page: "home" as Page,
  detail: [] as StripLine[],
  idle: [{ text: "Bader", size: 20, bold: true }] as StripLine[],
  unread: 0,
  lists: { mails: [], events: [] } as SnapshotLists,
  answer: [] as string[],
  answerAt: 0,
  rtl: false,
  buttons: [] as Button[],
  sent: new Map<string, string>(),
  revert: 0,
  answerTimer: 0,
};
let actions: FaceActions | null = null;

/** Sends a region only when its picture changed. */
function put(key: string, x: number, y: number, w: number, h: number, data: string) {
  if (ui.sent.get(key) === data) return;
  ui.sent.set(key, data);
  void Bridge.faceImg(x, y, w, h, data);
}

function drawBar() {
  const now = new Date();
  const hhmm = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  const day = now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  put("bar", 0, 0, W, BAR_H, paint(W, BAR_H, (ctx) => {
    ctx.fillStyle = PANEL;
    ctx.fillRect(0, 0, W, BAR_H - 2);
    ctx.font = font(15, true);
    ctx.fillStyle = INK;
    ctx.textAlign = "left";
    ctx.fillText(hhmm, 8, 13);
    ctx.font = font(12);
    ctx.fillStyle = GREY;
    ctx.textAlign = "center";
    ctx.fillText(day, W / 2, 13);
    ctx.textAlign = "right";
    ctx.font = font(13, true);
    ctx.fillStyle = ui.unread > 0 ? AMBER : GREEN;
    ctx.fillText(ui.unread > 0 ? `✉ ${ui.unread}` : "✉ 0", W - 8, 13);
  }));
}

function drawInfo() {
  if (ui.page !== "home") return;
  const label = LABELS[ui.mode];
  const lines: StripLine[] = ui.mode === "idle" || !label
    ? ui.idle
    : [{ text: `${label[0]} · ${label[1]}`, color: label[2], size: 19, bold: true }, ...ui.detail.slice(0, 1)];
  put("info", 0, INFO_Y, W, INFO_H, paint(W, INFO_H, (ctx) => {
    ctx.textAlign = "center";
    const rows = lines.slice(0, 2);
    rows.forEach((l, i) => {
      ctx.font = font(l.size ?? (rows.length === 1 ? 19 : 16), l.bold);
      ctx.fillStyle = l.color ?? INK;
      const y = rows.length === 1 ? INFO_H / 2 : 17 + i * 27;
      ctx.fillText(fit(ctx, l.text, W - 12), W / 2, y);
    });
  }));
}

function currentButtons(): Button[] {
  const a = actions;
  if (!a) return [];
  const talk: Button = { label: "Talk", sub: "تكلّم", color: CYAN, run: a.talk };
  const home: Button = { label: "Home", sub: "الرئيسية", color: GREY, run: () => setPage("home") };
  if (ui.mode === "approval") {
    return [
      { label: "Approve", sub: "موافق", color: GREEN, run: () => a.approve("once") },
      { label: "Deny", sub: "رفض", color: RED, run: () => a.approve("deny") },
    ];
  }
  if (ui.mode === "listening") {
    return [
      { label: "Send", sub: "إرسال", color: GREEN, run: a.talk },
      { label: "Cancel", sub: "إلغاء", color: RED, run: a.cancelTalk },
    ];
  }
  if (ui.page === "answer") {
    const more = ui.answerAt + answerRows() < ui.answer.length;
    const first: Button = ui.mode === "speaking"
      ? { label: "Stop", sub: "إيقاف", color: RED, run: a.stopSpeaking }
      : talk;
    return more
      ? [first, { label: "More", sub: "المزيد", color: AMBER, run: () => scrollAnswer(1) }, home]
      : [first, home];
  }
  if (ui.mode === "speaking") return [{ label: "Stop", sub: "إيقاف", color: RED, run: a.stopSpeaking }];
  if (ui.mode === "thinking" || ui.mode === "working") return [];
  if (ui.page !== "home") return [home, talk];
  return [
    talk,
    { label: "Brief", sub: "موجز", color: AMBER, run: () => a.ask(BRIEF) },
    { label: "Mail", sub: "البريد", color: INK, run: () => setPage("inbox") },
    { label: "Day", sub: "اليوم", color: INK, run: () => setPage("agenda") },
  ];
}

const BRIEF =
  "Give me my brief now: today's meetings and the important unread emails, in at most 6 short lines.";

function drawButtons() {
  ui.buttons = currentButtons();
  const n = ui.buttons.length;
  put("buttons", 0, BTN_Y, W, BTN_H, paint(W, BTN_H, (ctx) => {
    ctx.textAlign = "center";
    if (!n) {
      ctx.font = font(14);
      ctx.fillStyle = GREY;
      ctx.fillText("Please wait · لحظة من فضلك", W / 2, BTN_H / 2);
      return;
    }
    const bw = W / n;
    ui.buttons.forEach((b, i) => {
      ctx.fillStyle = PANEL;
      roundRect(ctx, i * bw + 3, 4, bw - 6, BTN_H - 8, 10);
      ctx.fillStyle = b.color;
      ctx.fillRect(i * bw + 14, 6, bw - 28, 2);
      ctx.font = font(n > 3 ? 15 : 17, true);
      ctx.fillText(fit(ctx, b.label, bw - 10), i * bw + bw / 2, b.sub ? 22 : BTN_H / 2);
      if (b.sub) {
        ctx.font = font(n > 3 ? 12 : 13);
        ctx.fillStyle = GREY;
        ctx.fillText(fit(ctx, b.sub, bw - 10), i * bw + bw / 2, 41);
      }
    });
  }));
}

function eventTime(start: string): string {
  const d = new Date(start);
  if (isNaN(d.getTime())) return "";
  const hhmm = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  return d.toDateString() === new Date().toDateString()
    ? hhmm
    : `${d.toLocaleDateString([], { weekday: "short" })} ${hhmm}`;
}

function listRows(): { title: string; sub: string; accent: string }[] {
  if (ui.page === "inbox") {
    return ui.lists.mails.slice(0, ROWS).map((m) => ({
      title: m.subject || "(no subject)",
      sub: m.from,
      accent: m.unread ? AMBER : GREY,
    }));
  }
  return ui.lists.events.slice(0, ROWS).map((e) => ({ title: e.title, sub: eventTime(e.start), accent: CYAN }));
}

function drawList() {
  const rows = listRows();
  const title = ui.page === "inbox" ? "Inbox · البريد" : "Meetings · الاجتماعات";
  const empty = ui.page === "inbox" ? "No mail · لا رسائل" : "No meetings ahead · لا اجتماعات";
  put("page", 0, FACE_Y, W, PAGE_H, paint(W, PAGE_H, (ctx) => {
    ctx.textAlign = "center";
    ctx.font = font(15, true);
    ctx.fillStyle = INK;
    ctx.fillText(title, W / 2, HEAD_H / 2);
    if (!rows.length) {
      ctx.font = font(15);
      ctx.fillStyle = GREY;
      ctx.fillText(empty, W / 2, PAGE_H / 2);
      return;
    }
    rows.forEach((r, i) => {
      const y = HEAD_H + i * (ROW_H + 1);
      ctx.fillStyle = PANEL;
      roundRect(ctx, 4, y, W - 8, ROW_H - 2, 8);
      ctx.fillStyle = r.accent;
      ctx.fillRect(4, y + 8, 3, ROW_H - 18);
      const rtl = isArabic(r.title);
      ctx.direction = rtl ? "rtl" : "ltr";
      ctx.textAlign = rtl ? "right" : "left";
      const x = rtl ? W - 12 : 14;
      ctx.font = font(14, true);
      ctx.fillStyle = INK;
      ctx.fillText(fit(ctx, r.title, W - 30), x, y + 13);
      ctx.font = font(12);
      ctx.fillStyle = r.accent;
      ctx.fillText(fit(ctx, r.sub, W - 30), x, y + 29);
      ctx.direction = "ltr";
    });
  }));
}

const ANSWER_LINE = 21;
const answerRows = () => Math.floor((PAGE_H - 8) / ANSWER_LINE);

function wrap(text: string): string[] {
  const c = document.createElement("canvas").getContext("2d")!;
  c.font = font(15);
  const out: string[] = [];
  for (const para of text.split(/\n+/)) {
    let line = "";
    for (const word of para.trim().split(/\s+/)) {
      if (!word) continue;
      const next = line ? `${line} ${word}` : word;
      if (c.measureText(next).width <= W - 16) line = next;
      else {
        if (line) out.push(line);
        line = word;
      }
    }
    if (line) out.push(line);
  }
  return out;
}

function drawAnswer() {
  const lines = ui.answer.slice(ui.answerAt, ui.answerAt + answerRows());
  put("page", 0, FACE_Y, W, PAGE_H, paint(W, PAGE_H, (ctx) => {
    ctx.font = font(15);
    ctx.fillStyle = INK;
    ctx.direction = ui.rtl ? "rtl" : "ltr";
    ctx.textAlign = ui.rtl ? "right" : "left";
    lines.forEach((l, i) => ctx.fillText(l, ui.rtl ? W - 8 : 8, 14 + i * ANSWER_LINE));
    ctx.direction = "ltr";
    if (ui.answer.length > answerRows()) {
      const total = Math.ceil(ui.answer.length / answerRows());
      const at = Math.floor(ui.answerAt / answerRows()) + 1;
      ctx.font = font(11);
      ctx.fillStyle = GREY;
      ctx.textAlign = "center";
      ctx.fillText(`${at} / ${total}`, W / 2, PAGE_H - 6);
    }
  }));
}

function drawPage() {
  if (ui.page === "home") return;
  if (ui.page === "answer") drawAnswer();
  else drawList();
}

function sendFace() {
  if (ui.page !== "home") return;
  void Bridge.face(ui.mode);
}

function setPage(page: Page) {
  if (page === ui.page) return;
  const wasHome = ui.page === "home";
  ui.page = page;
  window.clearTimeout(ui.answerTimer);
  ui.sent.delete("page");
  ui.sent.delete("info");
  if (page === "home") {
    void Bridge.faceCmd("POSES on");
    sendFace();
    drawInfo();
  } else {
    if (wasHome) void Bridge.faceCmd("POSES off");
    if (page !== "answer") void Bridge.snapshotLists().then((l) => {
      if (l) ui.lists = l;
      drawPage();
    });
    drawPage();
  }
  drawButtons();
}

function scrollAnswer(dir: number) {
  const next = ui.answerAt + dir * answerRows();
  if (next < 0 || next >= ui.answer.length) return;
  ui.answerAt = next;
  drawAnswer();
  drawButtons();
}

function redrawAll() {
  ui.sent.clear();
  if (!isTouch()) return;
  drawBar();
  if (ui.page === "home") {
    void Bridge.faceCmd("POSES on");
    void Bridge.face(ui.mode);
    drawInfo();
  } else {
    void Bridge.faceCmd("POSES off");
    drawPage();
  }
  drawButtons();
}

function onTouch(x: number, y: number) {
  if (!isTouch() || !actions) return;
  if (y >= BTN_Y) {
    const n = ui.buttons.length;
    if (n) ui.buttons[Math.min(n - 1, Math.floor(x / (W / n)))].run();
    return;
  }
  if (ui.page === "answer") {
    if (ui.answerAt + answerRows() < ui.answer.length) scrollAnswer(1);
    else setPage("home");
    return;
  }
  if (ui.page === "inbox" || ui.page === "agenda") {
    const i = Math.floor((y - FACE_Y - HEAD_H) / (ROW_H + 1));
    if (i < 0 || actions.busy()) return;
    if (ui.page === "inbox") {
      const m = ui.lists.mails[i];
      if (m) actions.ask(`Summarise this email and tell me what I should do about it: from ${m.from}, subject "${m.subject}".`);
    } else {
      const e = ui.lists.events[i];
      if (e) actions.ask(`Brief me on my meeting "${e.title}" (${eventTime(e.start)}): what it is about and anything related in my recent email.`);
    }
    return;
  }
  // Home: tapping Bader starts / stops talking.
  if (y >= FACE_Y && y < INFO_Y && (ui.mode === "idle" || ui.mode === "listening")) actions.talk();
}

function onSwipe(dir: string) {
  if (!isTouch()) return;
  if (ui.page === "answer") {
    if (dir === "up") scrollAnswer(1);
    else if (dir === "down") scrollAnswer(-1);
    else setPage("home");
    return;
  }
  if (ui.mode !== "idle") return;
  const order: Page[] = ["home", "agenda", "inbox"];
  const i = order.indexOf(ui.page);
  if (dir === "left") setPage(order[(i + 1) % order.length]);
  else if (dir === "right") setPage(order[(i + order.length - 1) % order.length]);
}

// ── What the chat calls ──────────────────────────────────────────────────────

/** Wires the screen to the chat; safe to call with no screen plugged in. */
export function initFace(a: FaceActions) {
  actions = a;
  void Bridge.faceInfo().then((i) => {
    dev = i ?? null;
    redrawAll();
  });
  void onEvent<FaceInfo | null>("face-ready", (i) => {
    dev = i ?? null;
    redrawAll();
  });
  void onEvent<[number, number]>("face-touch", ([x, y]) => onTouch(x, y));
  void onEvent<string>("face-swipe", (d) => onSwipe(d));
  // Holding the BOOT button on the touch screen (no approval waiting) re-runs
  // the touch calibration: tap the three crosses.
  void onEvent<string>("face-button", (kind) => {
    if (kind === "long" && isTouch()) void Bridge.faceCmd("CAL");
  });
  void onEvent<null>("face-idle", () => becameIdle());
  window.setInterval(() => {
    if (isTouch()) drawBar();
  }, 20_000);
}

/** Shows a face; with seconds, the screen goes back to idle by itself. */
export function setFace(name: FaceName, seconds?: number) {
  if (!isTouch()) {
    void Bridge.face(name, seconds);
    return;
  }
  window.clearTimeout(ui.revert);
  const changed = ui.mode !== name;
  ui.mode = name;
  if (changed) ui.detail = [];
  // Bader at work is shown on the home page; answers stay up while he speaks.
  if (["listening", "thinking", "working", "approval"].includes(name) && ui.page !== "home") setPage("home");
  if (ui.page === "home") {
    void Bridge.face(name, seconds);
    drawInfo();
  }
  drawButtons();
  if (seconds) ui.revert = window.setTimeout(() => becameIdle(), seconds * 1000 + 400);
}

/** The board went back to its idle pose by itself: only the text and buttons follow. */
function becameIdle() {
  window.clearTimeout(ui.revert);
  if (ui.mode === "idle") return;
  ui.mode = "idle";
  ui.detail = [];
  drawInfo();
  drawButtons();
}

/** Text under the current working face (cleared on the next face change). */
export function stripNow(lines: StripLine[]) {
  if (!isTouch()) {
    void Bridge.faceStrip(V2_Y, V2_H, false, v2Strip(lines));
    return;
  }
  ui.detail = lines.map((l) => ({ ...l, size: 15 }));
  drawInfo();
}

/** Text shown under every idle pose until replaced. */
export function stripIdle(lines: StripLine[], unread = 0) {
  ui.idle = lines.map((l) => ({ ...l, size: l.size && l.size > 17 ? l.size : 16 }));
  ui.unread = unread;
  if (!isTouch()) {
    void Bridge.faceStrip(V2_Y, V2_H, true, v2Strip(lines));
    return;
  }
  drawBar();
  drawInfo();
  if (ui.page === "inbox" || ui.page === "agenda") {
    void Bridge.snapshotLists().then((l) => {
      if (l) ui.lists = l;
      drawPage();
    });
  }
}

/** Touch screen: puts the full answer on the screen (tap = next page / home). */
export function showAnswer(text: string) {
  if (!isTouch() || !text.trim()) return;
  ui.answer = wrap(text);
  ui.answerAt = 0;
  ui.rtl = isArabic(text.slice(0, 80));
  if (ui.page === "answer") {
    drawAnswer();
    drawButtons();
  } else {
    setPage("answer");
  }
  window.clearTimeout(ui.answerTimer);
  ui.answerTimer = window.setTimeout(() => {
    if (ui.page === "answer" && ui.mode === "idle") setPage("home");
  }, 90_000);
}

/** Friendly label for the tool Bader is using right now. */
export function toolLabel(tool: string | null | undefined, preview?: string | null): { en: string; ar: string } {
  const t = (tool ?? "").toLowerCase();
  const p = (preview ?? "").toLowerCase();
  if (p.includes("google_api") && p.includes("calendar")) return { en: "Checking calendar…", ar: "أراجع التقويم…" };
  if (p.includes("google_api") && (p.includes(" send") || p.includes(" reply"))) return { en: "Sending email…", ar: "أرسل البريد…" };
  if (p.includes("google_api") || p.includes("bader_inbox")) return { en: "Reading mail…", ar: "أقرأ البريد…" };
  if (p.includes("recall.py")) return { en: "Remembering…", ar: "أتذكر…" };
  if (p.includes("news.py")) return { en: "Reading the news…", ar: "أقرأ الأخبار…" };
  if (p.includes("agent-reach")) return { en: "Researching online…", ar: "أبحث في الإنترنت…" };
  if (t.includes("web_search") || t === "web") return { en: "Searching the web…", ar: "أبحث في الويب…" };
  if (t.includes("web_extract") || t.includes("fetch")) return { en: "Reading a web page…", ar: "أقرأ صفحة…" };
  if (t.startsWith("browser")) return { en: "Using the browser…", ar: "أستخدم المتصفح…" };
  if (t.includes("computer")) return { en: "Using your computer…", ar: "أستخدم جهازك…" };
  if (t.includes("transcri") || t.includes("stt")) return { en: "Transcribing…", ar: "أفرغ التسجيل…" };
  if (t.includes("skill")) return { en: "Opening a skill…", ar: "أفتح مهارة…" };
  if (t.includes("read_file") || t.includes("file")) return { en: "Reading a file…", ar: "أقرأ ملفاً…" };
  if (t.includes("write") || t.includes("patch")) return { en: "Writing a file…", ar: "أكتب ملفاً…" };
  if (t.includes("terminal")) return { en: "Working…", ar: "أعمل…" };
  if (t.includes("memory")) return { en: "Remembering…", ar: "أتذكر…" };
  if (t.includes("delegat")) return { en: "Asking a helper…", ar: "أستعين بمساعد…" };
  return { en: "Working…", ar: "أعمل…" };
}

/** "18:00 · Weekly DNA Field…" and "3 unread" for the idle screen. */
export function idleLines(info: {
  unread: number;
  nextTitle?: string | null;
  nextStart?: string | null;
} | null): StripLine[] {
  if (!info) return [{ text: "Bader", size: 20, bold: true }];
  const lines: StripLine[] = [];
  if (info.nextTitle && info.nextStart) {
    lines.push({ text: `${eventTime(info.nextStart)} · ${info.nextTitle}`, color: CYAN });
  } else {
    lines.push({ text: "No meetings soon", color: GREY });
  }
  lines.push({
    text: info.unread > 0 ? `✉ ${info.unread} unread` : "✉ Inbox clear",
    color: info.unread > 0 ? AMBER : GREEN,
  });
  return lines;
}
