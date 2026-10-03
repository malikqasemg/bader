// Bader on the desktop: a small character in the corner of the screen that
// pops up, shows what Bader is doing, and fools around a little when idle.
// Drag = move him. Double-click = open Bader's window. Right-click = quick actions (talk, brief, mail,
// meetings, show/hide the window, hide Bader, quit).
//
// Setting "Show Bader on the desktop" (Settings, tray menu): on | off.

import { Bridge, onEvent } from "../core/bridge";

import idle from "../assets/buddy/idle.png";
import waving from "../assets/buddy/pose_waving.png";
import thumbs from "../assets/buddy/pose_thumbs_up.png";
import welcoming from "../assets/buddy/pose_welcoming.png";
import pointing from "../assets/buddy/pose_pointing.png";
import dancing from "../assets/buddy/pose_dancing.png";
import celebrating from "../assets/buddy/pose_celebrating.png";
import listening from "../assets/buddy/listening.png";
import thinking from "../assets/buddy/thinking.png";
import speaking from "../assets/buddy/speaking.png";
import happy from "../assets/buddy/happy.png";
import concerned from "../assets/buddy/concerned.png";
import surprised from "../assets/buddy/surprised.png";

type Mode = "always" | "events" | "off";
interface Look {
  src: string;
  head?: boolean;
  anim: string;
  text?: string;
  /** Seconds before going back to idle (0 = stays until the next state). */
  secs?: number;
}

const who = document.getElementById("who") as HTMLDivElement;
const pic = document.getElementById("pic") as HTMLImageElement;
const bubble = document.getElementById("bubble") as HTMLDivElement;
for (const src of [idle, waving, thumbs, welcoming, pointing, dancing, celebrating, listening, thinking, speaking, happy, concerned, surprised]) {
  new Image().src = src; // preload: pose changes must not flicker
}

const LOOKS: Record<string, Look> = {
  listening: { src: listening, head: true, anim: "nod", text: "I'm listening… · أسمعك" },
  thinking: { src: thinking, head: true, anim: "nod", text: "Thinking… · أفكر" },
  working: { src: thinking, head: true, anim: "nod", text: "Working… · أعمل" },
  speaking: { src: speaking, head: true, anim: "dance" },
  happy: { src: thumbs, anim: "jump", text: "Done ✓ · تم", secs: 6 },
  celebrating: { src: celebrating, anim: "dance", text: "Hello! · أهلاً", secs: 5 },
  concerned: { src: concerned, head: true, anim: "shake", text: "Something went wrong · في مشكلة", secs: 6 },
  surprised: { src: surprised, head: true, anim: "wiggle", secs: 5 },
  approval: { src: surprised, head: true, anim: "wiggle", text: "I need your OK · أحتاج موافقتك" },
};

/** Little shows while nothing is happening. */
const ANTICS: Look[] = [
  { src: waving, anim: "wiggle", text: "👋" },
  { src: dancing, anim: "dance" },
  { src: thumbs, anim: "jump" },
  { src: welcoming, anim: "spin" },
  { src: pointing, anim: "wiggle", text: "Need anything? · تحتاج شيئاً؟" },
  { src: celebrating, anim: "dance", text: "🎉" },
  { src: idle, anim: "peek" },
];

let mode: Mode = "always";
let shown = false;
let face = "idle";
let snoozeUntil = 0;
let revert = 0;
let antic = 0;
let hideTimer = 0;
let bubbleTimer = 0;

function say(text?: string, secs = 5) {
  window.clearTimeout(bubbleTimer);
  if (!text) {
    bubble.classList.remove("on");
    return;
  }
  bubble.textContent = text;
  bubble.classList.add("on");
  if (secs > 0) bubbleTimer = window.setTimeout(() => bubble.classList.remove("on"), secs * 1000);
}

function animate(name: string) {
  who.className = who.classList.contains("head") ? "head" : "";
  void who.offsetWidth; // restart the animation
  who.classList.add(name);
}

function dress(look: Look) {
  pic.src = look.src;
  who.classList.toggle("head", !!look.head);
}

async function appear() {
  window.clearTimeout(hideTimer);
  if (shown) return;
  shown = true;
  await Bridge.buddyShow();
  animate("pop");
  await new Promise((r) => window.setTimeout(r, 560));
}

function leave() {
  if (!shown) return;
  say();
  animate("bye");
  window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => {
    shown = false;
    who.className = "away";
    void Bridge.buddyHide();
  }, 420);
}

function scheduleAntic() {
  window.clearTimeout(antic);
  if (mode !== "always") return;
  antic = window.setTimeout(async () => {
    if (face === "idle" && shown && performance.now() >= snoozeUntil) {
      const a = ANTICS[Math.floor(Math.random() * ANTICS.length)];
      dress(a);
      animate(a.anim);
      say(a.text, 3);
      await new Promise((r) => window.setTimeout(r, 3200));
      if (face === "idle") rest();
    }
    scheduleAntic();
  }, 45_000 + Math.random() * 60_000);
}

/** Nothing going on: stand there (always) or go away (events). */
function rest() {
  face = "idle";
  say();
  if (mode === "always" && performance.now() >= snoozeUntil) {
    dress({ src: idle, anim: "breathe" });
    void appear().then(() => animate("breathe"));
  } else {
    leave();
  }
}

async function show(name: string, text?: string | null) {
  window.clearTimeout(revert);
  if (mode === "off") return;
  if (name === "idle") {
    // An answer snippet may still be on show: let it finish.
    if (face === "happy") return;
    rest();
    return;
  }
  const look = LOOKS[name];
  if (!look) return;
  if (performance.now() < snoozeUntil && name !== "approval") return;
  face = name;
  dress(look);
  await appear();
  animate(look.anim);
  say(text || look.text, look.secs ? look.secs : 0);
  if (look.secs) revert = window.setTimeout(() => rest(), look.secs * 1000);
}

async function loadMode() {
  const v = (await Bridge.engineStatus())?.values["bader.buddy"];
  const next: Mode = v === "events" || v === "off" ? v : "always";
  if (next === mode) return;
  mode = next;
  if (mode === "off") {
    leave();
  } else if (face === "idle") {
    // Back on the desktop: say hello again.
    dress({ src: waving, anim: "wiggle" });
    await appear();
    animate("wiggle");
    say("👋", 2);
    window.clearTimeout(revert);
    revert = window.setTimeout(() => rest(), 2200);
  }
  scheduleAntic();
}

// Hold and move = drag him anywhere (the spot is remembered).
// Double-click = open Bader's window. A plain click just makes him hop.
let downAt: { x: number; y: number } | null = null;
let dragging = false;
document.body.addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  downAt = { x: e.screenX, y: e.screenY };
  dragging = false;
});
document.body.addEventListener("mousemove", (e) => {
  if (!downAt || dragging) return;
  if (Math.abs(e.screenX - downAt.x) + Math.abs(e.screenY - downAt.y) < 4) return;
  dragging = true;
  downAt = null;
  void Bridge.buddyDrag();
  // The system takes over the drag; the page gets no mouse-up for it.
  window.setTimeout(() => void Bridge.buddyDropped(), 1500);
});
document.body.addEventListener("mouseup", () => {
  downAt = null;
  if (dragging) void Bridge.buddyDropped();
});
document.body.addEventListener("mouseenter", () => {
  if (dragging) {
    dragging = false;
    void Bridge.buddyDropped();
  }
});
document.body.addEventListener("click", () => {
  if (!dragging) animate("jump");
});
document.body.addEventListener("dblclick", () => void Bridge.buddyClick());
document.body.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  void Bridge.buddyMenu();
});
void onEvent<null>("buddy-mode", () => void loadMode());

void onEvent<{ face: string; text?: string | null }>("buddy", (e) => void show(e.face, e.text));
window.setInterval(() => void loadMode(), 15_000);

void (async () => {
  const v = (await Bridge.engineStatus())?.values["bader.buddy"];
  mode = v === "events" || v === "off" ? v : "always";
  if (mode === "off") return;
  // Hello: pop up waving, then settle.
  dress({ src: waving, anim: "wiggle" });
  await appear();
  animate("wiggle");
  say("Hi, I'm Bader · أهلاً، أنا بدر", 4);
  face = "celebrating";
  revert = window.setTimeout(() => rest(), 4500);
  scheduleAntic();
})();
