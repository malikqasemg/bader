// Thin wrapper over the Tauri commands/events. Every call is a no-op when the
// page is opened in a plain browser, so the island can be iterated on with
// `npm run dev` alone.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Settings } from "./state";

export const IS_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!IS_TAURI) return null;
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    console.error(`[bader] ${cmd} failed`, err);
    return null;
  }
}

export type FaceName =
  | "idle" | "neutral" | "listening" | "working" | "approval" | "thinking" | "speaking"
  | "happy" | "concerned" | "surprised" | "celebrating";

export interface SnapshotInfo {
  unread: number;
  mails: number;
  nextTitle?: string | null;
  nextStart?: string | null;
  updated?: string | null;
}

export interface RunEvent {
  kind: "tool" | "approval" | "approval-resolved" | "interim";
  tool?: string | null;
  text?: string | null;
}

export interface AccountsStatus {
  gmail: boolean;
  gmailClient: boolean;
  outlook: boolean;
  outlookClientId: string;
  outlookTenant: string;
}

export interface DeviceCode {
  userCode: string;
  verificationUri: string;
  deviceCode: string;
  interval: number;
  expiresIn: number;
}

export interface EngineStatus {
  found: boolean;
  running: boolean;
  home: string;
  /** Current YAML values, e.g. "model.default", "tts.edge.voice". */
  values: Record<string, string>;
  /** Which API keys are present in the engine (never their values). */
  keys: Record<string, boolean>;
}

export interface BootInfo {
  settings: Settings;
  /** Logical screen rect of the monitor the island lives on. */
  screen: { x: number; y: number; width: number; height: number; scale: number };
  version: string;
  hookPath: string;
}

export const Bridge = {
  boot: () => call<BootInfo>("boot"),

  saveSettings: (settings: Settings) => call<void>("save_settings", { settings }),

  /** Shrink the window down to the invisible wake strip (hidden) or back to full. */
  setCollapsed: (collapsed: boolean) => call<void>("set_collapsed", { collapsed }),

  /**
   * Pushes the island shape in window coordinates. Rust flips click-through from
   * its own cursor poll, so the flag is never a frame behind a click.
   */
  setIslandRect: (x: number, y: number, width: number, height: number) =>
    call<void>("set_island_rect", { x, y, width, height }),

  /** Give the window keyboard focus (chat field) and take it away again. */
  focusWindow: (focused: boolean) => call<void>("focus_window", { focused }),

  reposition: () => call<void>("reposition"),

  openUrl: (url: string) => call<void>("open_url", { url }),

  /** "Open terminal" → opens the folder in VS Code when `code` is on PATH. */
  openInVSCode: (path: string | null) => call<boolean>("open_in_vscode", { path }),

  quit: () => call<void>("quit_app"),

  openSettingsWindow: () => call<void>("open_settings_window"),

  /** Writes to %LOCALAPPDATA%\Bader\bader.log, next to the Rust lines. */
  log: (message: string) => call<void>("log_line", { message }),

  // ── Claude Code hooks ─────────────────────────────────────────────────────
  hooksStatus: () => call<HookStatus>("hooks_status"),
  /** Diff to show before anything is written. `install: false` previews removal. */
  hooksPreview: (install: boolean) => callOrThrow<HookPreview>("hooks_preview", { install }),
  /**
   * Writes ~/.claude/settings.json — only ever after an explicit click, and only
   * when the file still matches the preview the user looked at.
   */
  hooksApply: (install: boolean, fingerprint: string) =>
    callOrThrow<string>("hooks_apply", { install, fingerprint }),

  approvalDecision: (requestId: string, decision: "allow" | "deny") =>
    call<void>("approval_decision", { requestId, decision }),
  /** "The card is up" — until this lands the relay only waits a moment. */
  approvalAck: (requestId: string) => call<void>("approval_ack", { requestId }),
  /** "Nobody can act on this" — Claude Code asks in the terminal right away. */
  approvalDecline: (requestId: string) => call<void>("approval_decline", { requestId }),

  // ── Chat, files, secrets ──────────────────────────────────────────────────
  /** One chat turn. The API key and any file bytes never leave Rust. */
  chatSend: (query: string, context: ChatContext | null) =>
    callOrThrow<{ text: string }>("chat_send", { query, context }),
  chatReset: () => call<void>("chat_reset"),
  /** Copies a dropped file into the inbox. */
  ingestFile: (path: string) => callOrThrow<DroppedFile>("ingest_file", { path }),
  /** Only ever tells you whether a key exists — never its value. */
  secretPresent: (key: string) => call<boolean>("secret_present", { key }),
  secretSet: (key: string, value: string) => callOrThrow<void>("secret_set", { key, value }),
  secretClear: (key: string) => callOrThrow<void>("secret_clear", { key }),

  // ── Face screen (ESP32 on USB) ────────────────────────────────────────────
  /** Shows a face on Bader's USB screen; with seconds, it returns to "idle". */
  face: (name: FaceName, seconds?: number) => call<void>("face_set", { name, seconds: seconds ?? null }),
  /** Full-width RGB565 strip (base64) at y on the face screen; idle = keep under idle poses. */
  faceStrip: (y: number, h: number, idle: boolean, data: string) => call<void>("face_strip", { y, h, idle, data }),
  faceLed: (r: number, g: number, b: number, pulse = false) => call<void>("face_led", { r, g, b, pulse }),

  // ── Runs: approvals, snapshot ─────────────────────────────────────────────
  /** "once" approves what Bader is waiting on, "deny" refuses it. */
  runApprove: (choice: "once" | "deny") => callOrThrow<boolean>("run_approve", { choice }),
  snapshotInfo: () => call<SnapshotInfo>("snapshot_info"),
  syncNow: () => call<SnapshotInfo>("sync_now"),

  // ── Voice (mic in the island) ─────────────────────────────────────────────
  voiceStart: () => callOrThrow<void>("voice_start"),
  /** Stops the mic and returns what was said (Arabic or English). */
  voiceStop: () => callOrThrow<{ text: string; language: string }>("voice_stop"),
  voiceCancel: () => call<void>("voice_cancel"),
  /** Bader's spoken reply as a data URL. */
  voiceSpeak: (text: string) => callOrThrow<string>("voice_speak", { text }),
  /** Screenshot of the main display for one question; returns its path. */
  captureScreen: () => callOrThrow<string>("capture_screen"),

  // ── Accounts (Gmail, Outlook) ─────────────────────────────────────────────
  accountsStatus: () => call<AccountsStatus>("accounts_status"),
  gmailFindClientFile: () => call<string | null>("gmail_find_client_file"),
  gmailSetClient: (path: string) => callOrThrow<void>("gmail_set_client", { path }),
  gmailAuthUrl: () => callOrThrow<string>("gmail_auth_url"),
  gmailAuthCode: (code: string) => callOrThrow<void>("gmail_auth_code", { code }),
  gmailDisconnect: () => callOrThrow<void>("gmail_disconnect"),
  outlookStart: (clientId: string, tenant: string) =>
    callOrThrow<DeviceCode>("outlook_start", { clientId, tenant }),
  outlookWait: (deviceCode: string, interval: number, expiresIn: number) =>
    callOrThrow<string>("outlook_wait", { deviceCode, interval, expiresIn }),
  outlookDisconnect: () => callOrThrow<void>("outlook_disconnect"),

  // ── Engine (AI + voice settings) ──────────────────────────────────────────
  engineStatus: () => call<EngineStatus>("engine_status"),
  /** Writes engine settings; keys go to the engine's .env, never back to the page. */
  engineApply: (values: Record<string, string>, secrets: Record<string, string>, restart: boolean) =>
    callOrThrow<void>("engine_apply", { values, secrets, restart }),

  // ── Integrations ──────────────────────────────────────────────────────────
  refreshIntegration: (id: string) => call<void>("refresh_integration", { id }),
  /** Opens the configured n8n instance in the browser. */
  openN8n: () => call<void>("open_n8n"),

  /** Tray → Pause. Stops the integration pollers, not just the island. */
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
};

export interface IntegrationUpdate {
  id: string;
  data: Record<string, unknown>;
  error: string | null;
  event: { success: boolean; label: string; detail: string | null } | null;
}

export type ChatContext =
  | { kind: "file"; name: string; path: string }
  | { kind: "window"; appName: string; title: string; url?: string }
  | { kind: "screen"; path: string };

export interface DroppedFile {
  name: string;
  path: string;
  size: number;
}

export interface HookStatus {
  installed: boolean;
  settingsPath: string;
  hookPath: string;
  hookReady: boolean;
}

export interface HookPreview {
  diff: string;
  backup: string;
  settingsPath: string;
  /** Hand back to hooksApply so only the reviewed diff is ever written. */
  fingerprint: string;
}

/** Same as `call`, but surfaces the error so the UI can show what went wrong. */
async function callOrThrow<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!IS_TAURI) throw new Error("not running inside Bader");
  return invoke<T>(cmd, args);
}

export type BridgeEvent =
  | { name: "cursor"; payload: { x: number; y: number } }
  | { name: "tray"; payload: string }
  | { name: "hook"; payload: Record<string, unknown> }
  | { name: "screen-changed"; payload: null };

export interface DragDropPayload {
  type: "enter" | "over" | "drop" | "leave";
  paths?: string[];
}

/** Files dragged onto the island. Only reaches us when the window takes the mouse. */
export async function onDragDrop(handler: (e: DragDropPayload) => void) {
  if (!IS_TAURI) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    handler(event.payload as DragDropPayload);
  });
}

export async function onEvent<T>(name: string, handler: (payload: T) => void) {
  if (!IS_TAURI) return () => {};
  return listen<T>(name, (e) => handler(e.payload));
}
