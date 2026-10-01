// Bader sprite — the character art (bader/assets) drawn into the island canvases.
// Replaces the original procedural character: only the art changes, the motion
// (squash, tilt, bob, badges, particles) still comes from the engine.

import headUrl from "../assets/bader-head.png";

const head = new Image();
head.decoding = "async";
head.src = headUrl;

export const spriteReady = () => head.complete && head.naturalWidth > 0;

/** Draws Bader's head centred on (0,0) in the current transform, `size` px wide. */
export function drawBaderHead(x: CanvasRenderingContext2D, size: number, alpha = 1) {
  if (!spriteReady() || size <= 0.5) return;
  x.save();
  if (alpha < 1) x.globalAlpha *= alpha;
  x.imageSmoothingEnabled = true;
  x.imageSmoothingQuality = "high";
  x.drawImage(head, -size / 2, -size / 2, size, size);
  x.restore();
}
