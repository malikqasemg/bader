// Live text on Bader's face screen (ESP32, 172×320). The island draws the text
// on a canvas — so Arabic and English both render properly — converts it to
// RGB565 and sends it as a strip at the bottom of the screen (y 266–319).

import { Bridge } from "../core/bridge";

const W = 172;
const STRIP_Y = 266;
const STRIP_H = 54;

export interface StripLine {
  text: string;
  color?: string;
  size?: number;
  bold?: boolean;
}

function toBase64(bytes: Uint8Array): string {
  let s = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    s += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(s);
}

function fit(ctx: CanvasRenderingContext2D, text: string, max: number): string {
  if (ctx.measureText(text).width <= max) return text;
  let t = text;
  while (t.length > 1 && ctx.measureText(t + "…").width > max) t = t.slice(0, -1);
  return t + "…";
}

function render(lines: StripLine[]): Uint8Array {
  const c = document.createElement("canvas");
  c.width = W;
  c.height = STRIP_H;
  const ctx = c.getContext("2d")!;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, W, STRIP_H);
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  const rows = lines.slice(0, 2);
  rows.forEach((l, i) => {
    const size = l.size ?? (rows.length === 1 ? 17 : 15);
    ctx.font = `${l.bold ? 600 : 500} ${size}px -apple-system, "SF Pro Rounded", "Segoe UI", "SF Arabic", Tahoma, sans-serif`;
    ctx.fillStyle = l.color ?? "#ebeef2";
    const y = rows.length === 1 ? STRIP_H / 2 : 14 + i * 26;
    ctx.fillText(fit(ctx, l.text, W - 10), W / 2, y);
  });
  const px = ctx.getImageData(0, 0, W, STRIP_H).data;
  const out = new Uint8Array(W * STRIP_H * 2);
  for (let i = 0, j = 0; i < px.length; i += 4, j += 2) {
    const v = ((px[i] & 0xf8) << 8) | ((px[i + 1] & 0xfc) << 3) | (px[i + 2] >> 3);
    out[j] = v >> 8;
    out[j + 1] = v & 0xff;
  }
  return out;
}

/** Text under the current working face (cleared on the next face change). */
export function stripNow(lines: StripLine[]) {
  void Bridge.faceStrip(STRIP_Y, STRIP_H, false, toBase64(render(lines)));
}

/** Text shown under every idle pose until replaced. */
export function stripIdle(lines: StripLine[]) {
  void Bridge.faceStrip(STRIP_Y, STRIP_H, true, toBase64(render(lines)));
}

/** Friendly label for the tool Bader is using right now. */
export function toolLabel(tool: string | null | undefined, preview?: string | null): { en: string; ar: string } {
  const t = (tool ?? "").toLowerCase();
  const p = (preview ?? "").toLowerCase();
  if (p.includes("google_api") && p.includes("calendar")) return { en: "Checking calendar…", ar: "أراجع التقويم…" };
  if (p.includes("google_api") && (p.includes(" send") || p.includes(" reply"))) return { en: "Sending email…", ar: "أرسل البريد…" };
  if (p.includes("google_api") || p.includes("bader_inbox")) return { en: "Reading mail…", ar: "أقرأ البريد…" };
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
  return { en: tool ? `Using ${tool}…` : "Working…", ar: "أعمل…" };
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
    const d = new Date(info.nextStart);
    const now = new Date();
    const sameDay = d.toDateString() === now.toDateString();
    const hhmm = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
    const when = sameDay ? hhmm : `${d.toLocaleDateString([], { weekday: "short" })} ${hhmm}`;
    lines.push({ text: `${when} · ${info.nextTitle}`, color: "#40e0ff" });
  } else {
    lines.push({ text: "No meetings soon", color: "#96a0aa" });
  }
  lines.push({
    text: info.unread > 0 ? `✉ ${info.unread} unread` : "✉ Inbox clear",
    color: info.unread > 0 ? "#ffc440" : "#50dc78",
  });
  return lines;
}
