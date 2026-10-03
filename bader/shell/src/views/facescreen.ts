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
  /** speak: true = text + voice, false = text only. */
  ask: (query: string, speak: boolean) => void;
  /** The setting: ask each time, text only, or text + voice. */
  replyMode: () => Promise<"ask" | "text" | "voice">;
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
  // Older WebKit / WebView2 have no roundRect.
  if (typeof ctx.roundRect === "function") ctx.roundRect(x, y, w, h, r);
  else ctx.rect(x, y, w, h);
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

const BAR_H = 28;
const FACE_Y = 28;
const FACE_BOTTOM = 204; // the face picture is 240×176 at (0, 28) either way up
const ROW_H = 40;
const HEAD_H = 30;

interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Where things go. Upright: buttons along the bottom. On its side: buttons down the right. */
function geo(): { w: number; land: boolean; info: Rect; btn: Rect; page: Rect } {
  const w = dev?.w ?? 240;
  const land = w > (dev?.h ?? 320);
  return land
    ? { w, land, info: { x: 0, y: 204, w: 240, h: 36 }, btn: { x: 240, y: 28, w: 80, h: 212 }, page: { x: 0, y: 28, w: 240, h: 212 } }
    : { w, land, info: { x: 0, y: 204, w: 240, h: 60 }, btn: { x: 0, y: 264, w: 240, h: 56 }, page: { x: 0, y: 28, w: 240, h: 236 } };
}

const listRowCount = () => Math.floor((geo().page.h - HEAD_H) / (ROW_H + 1));

type Page = "home" | "agenda" | "inbox" | "answer";
interface Button {
  label: string;
  sub?: string;
  color: string;
  /** Relative size (default 1). */
  flex?: number;
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
  /** A request waiting for "Text" or "Text + voice". */
  choice: null as string | null,
  choiceTimer: 0,
};
let actions: FaceActions | null = null;

/** Sends a region only when its picture changed. */
function put(key: string, r: Rect, data: string) {
  if (ui.sent.get(key) === data) return;
  ui.sent.set(key, data);
  void Bridge.faceImg(r.x, r.y, r.w, r.h, data);
}

/** The turn-the-screen button lives at the left end of the top bar. */
const TURN_W = 34;

function drawBar() {
  const g = geo();
  const now = new Date();
  const hhmm = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  const day = now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" });
  put("bar", { x: 0, y: 0, w: g.w, h: BAR_H }, paint(g.w, BAR_H, (ctx) => {
    ctx.fillStyle = PANEL;
    ctx.fillRect(0, 0, g.w, BAR_H - 2);
    // Turn button: a small screen outline with an arrow.
    ctx.strokeStyle = GREY;
    ctx.lineWidth = 1.5;
    ctx.strokeRect(8, 7, 10, 13);
    ctx.beginPath();
    ctx.arc(20, 13, 6, -Math.PI / 2, Math.PI / 3);
    ctx.stroke();
    ctx.fillStyle = GREY;
    ctx.beginPath();
    ctx.moveTo(20, 4);
    ctx.lineTo(24, 7.5);
    ctx.lineTo(19, 10);
    ctx.fill();
    ctx.font = font(15, true);
    ctx.fillStyle = INK;
    ctx.textAlign = "left";
    ctx.fillText(hhmm, TURN_W + 4, 13);
    ctx.font = font(12);
    ctx.fillStyle = GREY;
    ctx.textAlign = "center";
    ctx.fillText(day, g.w / 2 + 14, 13);
    ctx.textAlign = "right";
    ctx.font = font(13, true);
    ctx.fillStyle = ui.unread > 0 ? AMBER : GREEN;
    ctx.fillText(ui.unread > 0 ? `✉ ${ui.unread}` : "✉ 0", g.w - 8, 13);
  }));
}

function drawInfo() {
  if (ui.page !== "home") return;
  const r = geo().info;
  const small = r.h < 50;
  const label = LABELS[ui.mode];
  const lines: StripLine[] = ui.choice && ui.mode === "idle"
    ? [{ text: "How do you want the answer?", color: INK, size: 16, bold: true }, { text: "كيف تريد الإجابة؟", color: AMBER, size: 16 }]
    : ui.mode === "idle" || !label
    ? ui.idle
    : [{ text: `${label[0]} · ${label[1]}`, color: label[2], size: 19, bold: true }, ...ui.detail.slice(0, 1)];
  put("info", r, paint(r.w, r.h, (ctx) => {
    ctx.textAlign = "center";
    const rows = lines.slice(0, 2);
    rows.forEach((l, i) => {
      const size = l.size ?? (rows.length === 1 ? 19 : 16);
      ctx.font = font(small ? Math.min(size, rows.length === 1 ? 16 : 13) : size, l.bold);
      ctx.fillStyle = l.color ?? INK;
      const y = rows.length === 1 ? r.h / 2 : small ? 9 + i * 17 : 17 + i * 27;
      ctx.fillText(fit(ctx, l.text, r.w - 12), r.w / 2, y);
    });
  }));
}

function currentButtons(): Button[] {
  const a = actions;
  if (!a) return [];
  const talk: Button = { label: "Talk", sub: "تكلّم", color: CYAN, run: a.talk };
  const home: Button = { label: "Home", sub: "الرئيسية", color: GREY, run: () => setPage("home") };
  if (ui.choice && ui.mode === "idle") {
    const q = ui.choice;
    const pick = (speak: boolean) => () => {
      clearChoice();
      a.ask(q, speak);
    };
    return [
      { label: "Text only", sub: "نص فقط", color: INK, flex: 5, run: pick(false) },
      { label: "Text + Voice", sub: "نص وصوت", color: CYAN, flex: 5, run: pick(true) },
      { label: "✕", color: RED, flex: 2, run: () => { clearChoice(); drawInfo(); drawButtons(); } },
    ];
  }
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
  // One colour for all four: a coloured button reads as "selected".
  const wide = geo().land ? 4 : 6;
  return [
    { ...talk, color: INK, flex: 4 },
    { label: "Brief", sub: "موجز", color: INK, flex: 4, run: () => request(BRIEF) },
    { label: "Mail", sub: "البريد", color: INK, flex: 4, run: () => setPage("inbox") },
    { label: "Meetings", sub: "اجتماعاتي", color: INK, flex: wide, run: () => setPage("agenda") },
  ];
}

function clearChoice() {
  ui.choice = null;
  window.clearTimeout(ui.choiceTimer);
}

/** A screen button asked Bader something: answer as text, or text + voice? */
function request(query: string) {
  const a = actions;
  if (!a || a.busy()) return;
  void a.replyMode().then((mode) => {
    if (mode !== "ask") {
      a.ask(query, mode === "voice");
      return;
    }
    ui.choice = query;
    drawInfo();
    drawButtons();
    window.clearTimeout(ui.choiceTimer);
    ui.choiceTimer = window.setTimeout(() => {
      if (!ui.choice) return;
      clearChoice();
      drawInfo();
      drawButtons();
    }, 20_000);
  });
}

const BRIEF =
  "Give me my brief now: today's meetings and the important unread emails, in at most 6 short lines.";

/** Each button's rectangle inside the button area (side by side, or stacked on a sideways screen). */
function buttonRects(): Rect[] {
  const g = geo();
  const total = ui.buttons.reduce((sum, b) => sum + (b.flex ?? 1), 0) || 1;
  let at = 0;
  return ui.buttons.map((b) => {
    const share = (b.flex ?? 1) / total;
    const r = g.land
      ? { x: 0, y: at, w: g.btn.w, h: g.btn.h * share }
      : { x: at, y: 0, w: g.btn.w * share, h: g.btn.h };
    at += g.land ? r.h : r.w;
    return r;
  });
}

/** pressed: the button under the finger is drawn filled, so a tap is seen. */
function drawButtons(pressed = -1) {
  if (pressed < 0) ui.buttons = currentButtons();
  const g = geo();
  const n = ui.buttons.length;
  const rects = buttonRects();
  put("buttons", g.btn, paint(g.btn.w, g.btn.h, (ctx) => {
    ctx.textAlign = "center";
    if (!n) {
      ctx.fillStyle = GREY;
      if (g.land) {
        ctx.font = font(13);
        ctx.fillText("Please wait", g.btn.w / 2, g.btn.h / 2 - 10);
        ctx.fillText("لحظة من فضلك", g.btn.w / 2, g.btn.h / 2 + 10);
      } else {
        ctx.font = font(14);
        ctx.fillText("Please wait · لحظة من فضلك", g.btn.w / 2, g.btn.h / 2);
      }
      return;
    }
    ui.buttons.forEach((b, i) => {
      const r = rects[i];
      const down = i === pressed;
      const cx = r.x + r.w / 2;
      const cy = r.y + r.h / 2;
      ctx.fillStyle = down ? b.color : PANEL;
      roundRect(ctx, r.x + 3, r.y + 3, r.w - 6, r.h - 6, 10);
      ctx.fillStyle = down ? "#000" : b.color;
      if (!down) ctx.fillRect(r.x + 14, r.y + 5, r.w - 28, 2);
      ctx.font = font(!g.land && n > 3 ? 15 : 16, true);
      ctx.fillText(fit(ctx, b.label, r.w - 8), cx, b.sub ? cy - 7 : cy);
      if (b.sub) {
        ctx.font = font(!g.land && n > 3 ? 12 : 13);
        ctx.fillStyle = down ? "#000" : GREY;
        ctx.fillText(fit(ctx, b.sub, r.w - 8), cx, cy + 12);
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
  const n = listRowCount();
  if (ui.page === "inbox") {
    return ui.lists.mails.slice(0, n).map((m) => ({
      title: m.subject || "(no subject)",
      sub: m.from,
      accent: m.unread ? AMBER : GREY,
    }));
  }
  return ui.lists.events.slice(0, n).map((e) => ({ title: e.title, sub: eventTime(e.start), accent: CYAN }));
}

function drawList(pressed = -1) {
  const p = geo().page;
  const rows = listRows();
  const title = ui.page === "inbox" ? "Inbox · البريد" : "Meetings · الاجتماعات";
  const empty = ui.page === "inbox" ? "No mail · لا رسائل" : "No meetings ahead · لا اجتماعات";
  put("page", p, paint(p.w, p.h, (ctx) => {
    ctx.textAlign = "center";
    ctx.font = font(15, true);
    ctx.fillStyle = INK;
    ctx.fillText(title, p.w / 2, HEAD_H / 2);
    if (!rows.length) {
      ctx.font = font(15);
      ctx.fillStyle = GREY;
      ctx.fillText(empty, p.w / 2, p.h / 2);
      return;
    }
    rows.forEach((r, i) => {
      const y = HEAD_H + i * (ROW_H + 1);
      ctx.fillStyle = i === pressed ? "#2c4a66" : PANEL;
      roundRect(ctx, 4, y, p.w - 8, ROW_H - 2, 8);
      ctx.fillStyle = r.accent;
      ctx.fillRect(4, y + 8, 3, ROW_H - 18);
      const rtl = isArabic(r.title);
      ctx.direction = rtl ? "rtl" : "ltr";
      ctx.textAlign = rtl ? "right" : "left";
      const x = rtl ? p.w - 12 : 14;
      ctx.font = font(14, true);
      ctx.fillStyle = INK;
      ctx.fillText(fit(ctx, r.title, p.w - 30), x, y + 13);
      ctx.font = font(12);
      ctx.fillStyle = r.accent;
      ctx.fillText(fit(ctx, r.sub, p.w - 30), x, y + 29);
      ctx.direction = "ltr";
    });
  }));
}

const ANSWER_LINE = 21;
const answerRows = () => Math.floor((geo().page.h - 8) / ANSWER_LINE);

function wrap(text: string): string[] {
  const c = document.createElement("canvas").getContext("2d")!;
  c.font = font(15);
  const max = geo().page.w - 16;
  const out: string[] = [];
  for (const para of text.split(/\n+/)) {
    let line = "";
    for (const word of para.trim().split(/\s+/)) {
      if (!word) continue;
      const next = line ? `${line} ${word}` : word;
      if (c.measureText(next).width <= max) line = next;
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
  const p = geo().page;
  const lines = ui.answer.slice(ui.answerAt, ui.answerAt + answerRows());
  put("page", p, paint(p.w, p.h, (ctx) => {
    ctx.font = font(15);
    ctx.fillStyle = INK;
    ctx.direction = ui.rtl ? "rtl" : "ltr";
    ctx.textAlign = ui.rtl ? "right" : "left";
    lines.forEach((l, i) => ctx.fillText(l, ui.rtl ? p.w - 8 : 8, 14 + i * ANSWER_LINE));
    ctx.direction = "ltr";
    if (ui.answer.length > answerRows()) {
      const total = Math.ceil(ui.answer.length / answerRows());
      const at = Math.floor(ui.answerAt / answerRows()) + 1;
      ctx.font = font(11);
      ctx.fillStyle = GREY;
      ctx.textAlign = "center";
      ctx.fillText(`${at} / ${total}`, p.w / 2, p.h - 6);
    }
  }));
}

function drawPage() {
  if (ui.page === "home") return;
  if (ui.page === "answer") drawAnswer();
  else drawList();
}

// The app decides every redraw (idle poses too): the board must never be busy
// drawing by itself while a picture is on its way — its USB link has no flow control.
const POSES = ["idle", "pose_waving", "pose_thumbs_up", "pose_welcoming", "pose_pointing", "pose_dancing", "pose_celebrating"];
let poseAt = 0;

function sendFace() {
  if (ui.page !== "home") return;
  void Bridge.face((ui.mode === "idle" ? POSES[poseAt % POSES.length] : ui.mode) as FaceName);
}

function setPage(page: Page) {
  if (page === ui.page) return;
  ui.page = page;
  window.clearTimeout(ui.answerTimer);
  ui.sent.delete("page");
  ui.sent.delete("info");
  if (page === "home") {
    sendFace();
    drawInfo();
  } else {
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
  try {
    redrawAllNow();
  } catch (err) {
    void Bridge.log(`face screen redraw failed: ${String(err)}`);
  }
}

function redrawAllNow() {
  ui.sent.clear();
  if (!isTouch()) return;
  void Bridge.faceCmd("POSES off");
  drawBar();
  if (ui.page === "home") {
    sendFace();
    drawInfo();
  } else {
    if (ui.page === "answer") ui.answer = wrap(ui.answer.join(" "));
    drawPage();
  }
  drawButtons();
}

/** Turns the picture a quarter turn: upright → on its side → upside down → other side.
 *  The board answers with its new size, and everything is drawn again for it. */
function turnScreen() {
  ui.sent.clear();
  void Bridge.faceCmd("ROT +");
}

const inside = (r: Rect, x: number, y: number) => x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h;

function onTouch(x: number, y: number) {
  if (!isTouch() || !actions) return;
  const g = geo();
  if (y < BAR_H) {
    void Bridge.log(`screen tap ${x},${y} on the bar`);
    if (x < TURN_W + 10) turnScreen();
    return;
  }
  // A little slack around the buttons: fingers are not styluses.
  const slack = { x: g.btn.x - (g.land ? 6 : 0), y: g.btn.y - (g.land ? 0 : 6), w: g.btn.w + 6, h: g.btn.h + 6 };
  if (inside(slack, x, y)) {
    const rects = buttonRects();
    let i = rects.findIndex((r) => inside(r, x - g.btn.x, y - g.btn.y));
    if (i < 0) i = g.land ? (y < g.btn.y ? 0 : rects.length - 1) : (x < g.btn.x + g.btn.w / 2 ? 0 : rects.length - 1);
    const b = ui.buttons[i];
    void Bridge.log(`screen tap ${x},${y} → ${b ? b.label : "no button"}`);
    if (!b) return;
    // Show the press, then act.
    drawButtons(i);
    window.setTimeout(() => {
      b.run();
      drawButtons();
    }, 160);
    return;
  }
  void Bridge.log(`screen tap ${x},${y} on ${ui.page}`);
  if (ui.page === "answer") {
    if (ui.answerAt + answerRows() < ui.answer.length) scrollAnswer(1);
    else setPage("home");
    return;
  }
  if (ui.page === "inbox" || ui.page === "agenda") {
    const i = Math.floor((y - g.page.y - HEAD_H) / (ROW_H + 1));
    if (i < 0 || i >= listRowCount() || actions.busy()) return;
    const m = ui.page === "inbox" ? ui.lists.mails[i] : null;
    const e = ui.page === "agenda" ? ui.lists.events[i] : null;
    if (!m && !e) return;
    drawList(i);
    window.setTimeout(() => {
      drawList();
      if (m) request(`Summarise this email and tell me what I should do about it: from ${m.from}, subject "${m.subject}".`);
      else if (e) request(`Brief me on my meeting "${e.title}" (${eventTime(e.start)}): what it is about and anything related in my recent email.`);
    }, 160);
    return;
  }
  // Home: tapping Bader starts / stops talking.
  if (y >= FACE_Y && y < FACE_BOTTOM && x < 240 && (ui.mode === "idle" || ui.mode === "listening")) actions.talk();
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
  // The "screen arrived" event can fire before this page listens (app start):
  // ask now and then, and redraw when the answer changed.
  window.setInterval(() => {
    void Bridge.faceInfo().then((i) => {
      const now = i ?? null;
      if (JSON.stringify(now) === JSON.stringify(dev)) return;
      dev = now;
      redrawAll();
    });
  }, 4_000);
  window.setInterval(() => {
    if (!isTouch() || ui.mode !== "idle" || ui.page !== "home") return;
    poseAt++;
    sendFace();
  }, 30_000);
}

/** Shows a face; with seconds, the screen goes back to idle by itself. */
let lastFace: FaceName = "idle";

export function setFace(name: FaceName, seconds?: number) {
  lastFace = name;
  void Bridge.buddy(name); // the character on the desktop follows along
  if (!isTouch()) {
    void Bridge.face(name, seconds);
    return;
  }
  window.clearTimeout(ui.revert);
  if (name !== "idle") clearChoice();
  const changed = ui.mode !== name;
  ui.mode = name;
  if (changed) ui.detail = [];
  // Bader at work is shown on the home page; answers stay up while he speaks.
  if (["listening", "thinking", "working", "approval"].includes(name) && ui.page !== "home") setPage("home");
  if (ui.page === "home") {
    sendFace();
    drawInfo();
  }
  drawButtons();
  if (seconds) ui.revert = window.setTimeout(() => becameIdle(), seconds * 1000);
}

/** The board went back to its idle pose by itself: only the text and buttons follow. */
function becameIdle() {
  window.clearTimeout(ui.revert);
  if (ui.mode === "idle") return;
  ui.mode = "idle";
  ui.detail = [];
  sendFace();
  drawInfo();
  drawButtons();
}

/** Text under the current working face (cleared on the next face change). */
export function stripNow(lines: StripLine[]) {
  if (lines[0]?.text && lastFace !== "idle") void Bridge.buddy(lastFace, lines[0].text);
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
  if (text.trim()) void Bridge.buddy("happy", text.replace(/\s+/g, " ").trim().slice(0, 100));
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
