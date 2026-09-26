/**
 * `<useless-button>` — a real `<button>` with a wasm simulation painted
 * behind its label.
 *
 * Everything that makes it an actual button (keyboard activation, form
 * participation, focus, `:disabled`) comes from using a real
 * `<button type="button">` inside shadow DOM; the canvas is purely
 * decorative (`aria-hidden`) and the visible label is plain slotted text.
 */
import { SPECTACLE_TEXT_MODES, paintSpectacleChar, type SpectacleTextMode } from "./spectacle-text.js";
import { registerFrameCallback } from "./scheduler.js";
import { createUselessButton, readFrame, type UselessButton } from "./wasm.js";

const DEFAULT_VARIANT = "swarm";
const DEFAULT_SEED = 1;
const MAX_DPR = 2;
const FALLBACK_CSS_WIDTH = 160;
const FALLBACK_CSS_HEIGHT = 48;

type TextFx = "none" | "spin" | "skew" | "explode" | "snake" | "slot" | "tunnel" | SpectacleTextMode;
const TEXT_FX_VALUES: readonly TextFx[] = ["spin", "skew", "explode", "snake", "slot", "tunnel", "none", ...SPECTACLE_TEXT_MODES];

/**
 * Modes that animate each character on its own, rather than transforming
 * the label as a single block. They all share the same machinery: the
 * real slotted text is hidden but keeps its layout box, and a parallel
 * layer of per-character `<span>`s is animated in its place (see
 * `syncLabelMode`).
 */
const PER_CHAR_FX: ReadonlySet<TextFx> = new Set<TextFx>(["explode", "snake", "slot", ...SPECTACLE_TEXT_MODES]);

// --- `text-fx="spin"`: continuous rotation whose angular *speed* traces
// a sine wave (always net-forward — amplitude is kept under the base
// speed — so it reads as "spinning, now faster, now slower" rather than
// wobbling back and forth). The peak speed is deliberately extreme: at
// ~5 rotations/second a CSS transform has no motion blur to help it, so
// the label just reads as a spinning smear — that's the point, it should
// swing all the way from "readable" to "too fast to read".
const SPIN_BASE_DEG_PER_SEC = 900;
const SPIN_AMPLITUDE_DEG_PER_SEC = 850;
// Scale pulses in sync with the very same wave driving the speed, so it
// visibly swells while spinning fast and shrinks while spinning slow
// rather than pulsing on an unrelated rhythm. Both how *long* each cycle
// takes to reach peak speed and how far it scales up are re-rolled at
// the start of every cycle, so consecutive spins don't all feel
// identical.
const SPIN_CYCLE_MIN_SEC = 0.7;
const SPIN_CYCLE_MAX_SEC = 4.0;
const SPIN_SCALE_AMP_MIN = 0.12;
const SPIN_SCALE_AMP_MAX = 0.65;

// --- `text-fx="skew"`: a "random"-looking wobble built from a couple of
// sine waves per axis at deliberately non-integer-ratio frequencies, so
// the combined motion doesn't visibly repeat on any short cycle. Amounts
// are large enough that the label's corners genuinely swing outside the
// button's own box at the extremes (the button no longer clips its
// label — see `overflow: visible` below) and the frequencies are fast
// enough that it reads as agitated rather than a gentle sway.
// Near 90° tan() blows up, so these are pushed close to that edge on
// purpose — a button's own padding otherwise leaves enough slack that
// even a fairly large skew angle can still land short of the button's
// actual edge for a short/small label.
const SKEW_AMP_X_DEG = 80;
const SKEW_AMP_Y_DEG = 55;
const SKEW_FREQ_X1_HZ = 0.9;
const SKEW_FREQ_X2_HZ = 1.5;
const SKEW_FREQ_Y1_HZ = 1.2;
const SKEW_FREQ_Y2_HZ = 0.65;
// Rides along with the skew angle (see below) so the label actually
// travels off-center instead of just shearing in place — needed for it
// to reliably clear a wide button's padding rather than staying inside.
const SKEW_TRANSLATE_PX_PER_DEG = 0.9;

// --- `text-fx="explode"`: each character bursts out and snaps back, on
// a repeating cycle with a slight per-character stagger. Unlike a smooth
// sine breathing motion, the phases below are deliberately asymmetric —
// quick burst out, a beat held scattered, a quick snap back, a longer
// rest assembled — so it reads as an actual *explosion* rather than a
// gentle pulse. Direction, distance and rotation are re-rolled per
// character *for every cycle* (seeded off the character index and the
// cycle number, not the clock) rather than fixed once forever, so the
// same word doesn't burst the exact same way twice in a row.
const EXPLODE_PERIOD_SEC = 2.1;
const EXPLODE_STAGGER_SEC = 0.02;
const EXPLODE_BURST_OUT_FRAC = 0.12; // 0 -> fraction: fast ease-out burst
const EXPLODE_HOLD_FRAC = 0.4; // fraction -> fraction: held at full scatter
const EXPLODE_SNAP_BACK_FRAC = 0.58; // fraction -> fraction: fast ease-in snap back
// past EXPLODE_SNAP_BACK_FRAC and up to 1.0: resting, assembled
// The button no longer clips its label (see `overflow: visible` below),
// so the scatter radius can be large enough to genuinely leave the
// button's box — a real burst, not a jitter contained inside it.
const EXPLODE_RADIUS_MIN_PX = 22;
const EXPLODE_RADIUS_MAX_PX = 66;
const EXPLODE_ROTATE_MIN_DEG = 30;
const EXPLODE_ROTATE_MAX_DEG = 210;
const EXPLODE_SCALE_MIN = 1.1;
const EXPLODE_SCALE_MAX = 1.9;

// --- `text-fx="snake"`: the characters slither along a travelling wave.
// Each one is a sample of the same curve at its own phase offset, so the
// word ripples end to end rather than every letter bobbing in unison,
// and each character is also *rotated to the curve's local tangent* —
// that tangent is what makes it read as one body following a path
// instead of a row of letters bouncing independently.
// Amplitudes are deliberately larger than the button: a typical button is
// ~48px tall, so a ±34px wave carries the letters clean out of its top and
// bottom, and the whole-body sweep below takes them out past the sides
// too. The button doesn't clip its label (`overflow: visible` below), so
// the snake is free to leave the box entirely rather than wriggling in
// place in the middle of it.
const SNAKE_AMP_Y_PX = 34;
/** Per-character sideways sway, at half the vertical frequency, so the path is a slither rather than a pure up/down wave. */
const SNAKE_AMP_X_PX = 18;
/**
 * A slow horizontal sweep applied to every character equally, so the body
 * as a whole travels across and beyond the button rather than rippling
 * around a fixed center.
 */
const SNAKE_SWEEP_X_PX = 52;
const SNAKE_SWEEP_HZ = 0.21;
const SNAKE_SPEED_HZ = 0.62;
/** Radians of phase between neighboring characters — the wavelength of the body, in letters. */
const SNAKE_PHASE_PER_CHAR = 0.95;
/** Nominal character advance in px, used to convert the curve's slope into a tangent angle. */
const SNAKE_CHAR_ADVANCE_PX = 11;
/** How much of the tangent angle to actually apply. Full tangent is too steep to stay readable. */
const SNAKE_TANGENT_FRAC = 0.75;

// --- `text-fx="slot"`: each character is a slot-machine reel spinning
// about the X axis. Reels decelerate into a whole number of turns (so
// they always land face-on, never stopped edge-on and invisible), stop
// left to right, hold the result for a beat, then spin up again. Turn
// count and stagger are re-rolled per character per cycle, so the reels
// don't fall into lockstep.
const SLOT_PERIOD_SEC = 3.4;
/** When the first reel comes to rest, and how much later each subsequent one does. */
const SLOT_FIRST_STOP_SEC = 1.1;
const SLOT_STAGGER_SEC = 0.22;
/** How long the reels stay stopped before the next cycle spins them up. */
const SLOT_HOLD_SEC = 0.7;
const SLOT_MIN_TURNS = 3;
const SLOT_MAX_TURNS = 9;
/** Perspective distance for the reels' `rotateX`, in px. Shorter = more dramatic foreshortening. */
const SLOT_PERSPECTIVE_PX = 260;

/** Cheap, deterministic, seed -> [0, 1) pseudo-random hash (no RNG dependency needed for decorative jitter). */
function pseudoRandom(seed: number): number {
  const x = Math.sin(seed * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

/**
 * Asymmetric burst-out / hold / snap-back-in curve for `text-fx="explode"`,
 * `t` in `[0, 1)`. A plain `sin(t*π)` reads as a gentle breathing pulse;
 * this instead front-loads a fast ease-out burst, holds at full scatter
 * for a beat, then snaps back in fast — closer to an actual explosion.
 */
function explodeScatterAmount(t: number): number {
  if (t < EXPLODE_BURST_OUT_FRAC) {
    const u = t / EXPLODE_BURST_OUT_FRAC;
    return 1 - (1 - u) ** 3; // ease-out cubic, 0 -> 1
  }
  if (t < EXPLODE_HOLD_FRAC) {
    return 1;
  }
  if (t < EXPLODE_SNAP_BACK_FRAC) {
    const u = (t - EXPLODE_HOLD_FRAC) / (EXPLODE_SNAP_BACK_FRAC - EXPLODE_HOLD_FRAC);
    return 1 - u * u; // ease-in quadratic, 1 -> 0 (accelerating snap)
  }
  return 0; // resting, fully assembled
}

interface FxChar {
  el: HTMLSpanElement;
  /** Position within the label, used (with the current cycle number) to
   * seed that character's direction/distance/rotation for each burst —
   * see `updateTextFx`'s explode branch. */
  index: number;
  stagger: number;
}

const SHADOW_CSS = `
:host {
  display: inline-block;
  --ub-paper: #f5f3ec;
  --ub-ink: #1a1a1a;
  --ub-accent: #e0503d;
}
@media (prefers-color-scheme: dark) {
  :host {
    --ub-paper: #1c1c1c;
    --ub-ink: #f2f0e9;
    --ub-accent: #ff7a5c;
  }
}
button {
  appearance: none;
  -webkit-appearance: none;
  border: 0;
  margin: 0;
  padding: 0 20px;
  background: transparent;
  font: inherit;
  /* Simulations now paint with full-spectrum random RGB colors instead of
     a shared accent-derived gradient (see the Rust sims), which freed up
     --ub-accent to become the label's own color instead. */
  color: var(--ub-accent);
  cursor: pointer;
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  box-sizing: border-box;
  /* Deliberately NOT clipped: text-fx (spin/skew/explode) is allowed to
     swing or burst outside the button's own box. The canvas clips itself
     (see below) so the simulation still stays within the rounded shape. */
  overflow: visible;
  border-radius: 10px;
  min-width: 160px;
  min-height: 48px;
  isolation: isolate;
  -webkit-tap-highlight-color: transparent;
}
button:focus-visible {
  outline: 2px solid var(--ub-accent);
  outline-offset: 2px;
}
:host([disabled]) button {
  cursor: not-allowed;
  opacity: 0.6;
}
canvas {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  display: block;
  pointer-events: none;
  /* Clip the simulation to the button's own rounded corners itself,
     since the button no longer does this for its children. */
  border-radius: inherit;
  overflow: hidden;
  /* The backing store is sized in device pixels and then scaled to the
     CSS box (see element.ts's devicePixelRatio handling) — if that scale
     isn't a clean 1:1, the browser's default smooth/bilinear resampling
     blurs hard pixel-art edges (falling-sand grains, life cells) into a
     soft gradient, which can look like a gap or a lighter/emptier edge
     even though the underlying simulation pixels are fully opaque.
     Nearest-neighbor scaling keeps every simulation pixel crisp. */
  image-rendering: pixelated;
  image-rendering: crisp-edges; /* Firefox fallback */
}
span[part="label"] {
  position: relative;
  z-index: 1;
  display: inline-block;
  pointer-events: none;
  white-space: nowrap;
  transform-origin: 50% 50%;
  transition: opacity 0.45s ease-out;
}
/* Game variants hide the label until the game is cleared; dropping the
   class fades it in. */
span[part="label"].locked {
  visibility: hidden;
  opacity: 0;
  transition: none;
}
.label-fx {
  display: none;
  pointer-events: none;
}
.label-fx .ch {
  display: inline-block;
  white-space: pre;
}
/* The slot itself is hidden (or shown) via inline "visibility" set from
   JS (see syncLabelMode()) — so the real text and the animated
   character layer are never both visible/hidden at once. This rule just
   positions the character layer once it's shown, for every mode that
   animates characters individually (see PER_CHAR_FX). */
:host([text-fx="explode"]) .label-fx,
:host([text-fx="snake"]) .label-fx,
:host([text-fx="slot"]) .label-fx,
:host([text-fx="blackhole"]) .label-fx,
:host([text-fx="chrome"]) .label-fx,
:host([text-fx="plasma"]) .label-fx,
:host([text-fx="stained-glass"]) .label-fx,
:host([text-fx="aurora"]) .label-fx,
:host([text-fx="ripple"]) .label-fx,
:host([text-fx="hologram"]) .label-fx,
:host([text-fx="supernova"]) .label-fx,
:host([text-fx="matrix"]) .label-fx,
:host([text-fx="zoom"]) .label-fx,
:host([text-fx="ricochet"]) .label-fx,
:host([text-fx="corridor"]) .label-fx,
:host([text-fx="streak"]) .label-fx {
  display: inline-block;
  position: absolute;
  inset: 0;
}
/* Slot-machine reels rotate about X; without a 3D transform style the
   foreshortening reads as a flat vertical squash instead of a reel. */
:host([text-fx="slot"]) .label-fx .ch {
  transform-style: preserve-3d;
  backface-visibility: hidden;
}
`;

let probeEl: HTMLSpanElement | null = null;

/**
 * Resolve an arbitrary CSS color string (keyword, hex, hsl(), a chain of
 * `var()`, anything) to `0xRRGGBB` by letting the browser do the actual
 * parsing: assign it to an offscreen element's `color` and read back
 * `getComputedStyle`'s normalized `rgb(r, g, b)`.
 */
function resolveCssColorToPacked(rawValue: string, fallback: number): number {
  const value = rawValue.trim();
  if (!value) return fallback;
  if (!document.body) return fallback;
  if (!probeEl) {
    probeEl = document.createElement("span");
    probeEl.setAttribute("aria-hidden", "true");
    probeEl.style.position = "fixed";
    probeEl.style.left = "-9999px";
    probeEl.style.top = "-9999px";
    probeEl.style.opacity = "0";
    probeEl.style.pointerEvents = "none";
    document.body.appendChild(probeEl);
  }
  probeEl.style.color = "";
  probeEl.style.color = value;
  const resolved = getComputedStyle(probeEl).color;
  const match = /rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/.exec(resolved);
  if (!match) return fallback;
  const r = Number(match[1]) & 0xff;
  const g = Number(match[2]) & 0xff;
  const b = Number(match[3]) & 0xff;
  return (r << 16) | (g << 8) | b;
}

export class UselessButtonElement extends HTMLElement {
  static get observedAttributes(): string[] {
    return ["variant", "fps", "seed", "disabled", "text-fx"];
  }

  private readonly buttonEl: HTMLButtonElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly labelEl: HTMLElement;
  private readonly labelFxEl: HTMLElement;
  private readonly slotEl: HTMLSlotElement;
  private ctx: CanvasRenderingContext2D | null = null;

  private core: UselessButton | null = null;
  private devW = 0;
  private devH = 0;

  private readonly resizeObserver: ResizeObserver;
  private readonly intersectionObserver: IntersectionObserver;
  private unregisterScheduler: (() => void) | null = null;

  private isIntersecting = false;
  private readonly reducedMotionQuery: MediaQueryList;


  private fpsIntervalSeconds = 1 / 60;
  private tickAccumulator = 0;

  private textFxTime = 0;
  private spinAngleDeg = 0;
  private spinCycleT = 0; // 0..1 progress within the current spin cycle
  private spinCycleIndex = 0;
  private spinCyclePeriodSec = (SPIN_CYCLE_MIN_SEC + SPIN_CYCLE_MAX_SEC) / 2;
  private spinCycleScaleAmp = (SPIN_SCALE_AMP_MIN + SPIN_SCALE_AMP_MAX) / 2;
  private fxChars: FxChar[] = [];

  /** A game variant that hasn't been cleared yet: label hidden, clicks swallowed. */
  private gameLocked = false;
  /** Whether the press (pointer or key) that will produce the next click began while locked. */
  private pressWhileLocked = false;

  constructor() {
    super();
    // `delegatesFocus` means `hostEl.focus()` (e.g. a React ref calling
    // `.focus()`, or a form library doing the same) actually moves focus
    // into the real inner `<button>`, and `document.activeElement` at the
    // top level reports this host directly instead of requiring callers
    // to drill into `shadowRoot.activeElement` themselves.
    const root = this.attachShadow({ mode: "open", delegatesFocus: true });
    root.innerHTML = `<style>${SHADOW_CSS}</style>
      <button type="button" part="button">
        <canvas aria-hidden="true" part="canvas"></canvas>
        <span part="label">
          <slot></slot><span class="label-fx" aria-hidden="true"></span>
        </span>
      </button>`;
    this.buttonEl = root.querySelector("button")!;
    this.canvas = root.querySelector("canvas")!;
    this.labelEl = root.querySelector('span[part="label"]')!;
    this.labelFxEl = root.querySelector(".label-fx")!;
    this.slotEl = root.querySelector("slot")!;
    this.slotEl.addEventListener("slotchange", this.onSlotChange);

    this.reducedMotionQuery = matchMedia("(prefers-reduced-motion: reduce)");

    this.resizeObserver = new ResizeObserver((entries) => this.handleResize(entries));
    this.intersectionObserver = new IntersectionObserver(
      (entries) => this.handleIntersection(entries),
      { threshold: 0 },
    );

    this.buttonEl.addEventListener("pointerdown", this.onPointerDown);
    this.buttonEl.addEventListener("click", this.onClick);
    this.buttonEl.addEventListener("keydown", this.onKeyDown);
    // Registered on the host itself, before any page code can add its
    // own listeners, so it runs first — see `onHostClickCapture`.
    this.addEventListener("click", this.onHostClickCapture, true);
    this.reducedMotionQuery.addEventListener("change", this.onReducedMotionChange);
  }

  connectedCallback(): void {
    this.syncDisabled();
    this.resizeObserver.observe(this);
    this.intersectionObserver.observe(this);
    this.ensureCore();
    // `slotchange` reliably fires for content assigned after connection,
    // but covering the initial-markup case explicitly (rather than
    // relying on timing) is simpler than it is to get wrong.
    this.syncLabelMode();
  }

  disconnectedCallback(): void {
    this.resizeObserver.disconnect();
    this.intersectionObserver.disconnect();
    this.stopAnimating();
    this.core?.free();
    this.core = null;
    this.ctx = null;
  }

  attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    if (oldValue === newValue) return;
    switch (name) {
      case "variant":
      case "seed":
        this.recreateCore();
        break;
      case "fps":
        this.fpsIntervalSeconds = this.computeFpsInterval();
        break;
      case "disabled":
        this.syncDisabled();
        break;
      case "text-fx":
        this.resetTextFx();
        this.syncLabelMode();
        break;
      default:
        break;
    }
  }

  // ---- reflected attributes / JS properties ----

  get variant(): string {
    return this.getAttribute("variant") ?? DEFAULT_VARIANT;
  }
  set variant(value: string) {
    this.setAttribute("variant", value);
  }

  get seed(): number {
    const raw = this.getAttribute("seed");
    const n = raw === null ? NaN : Number(raw);
    return Number.isFinite(n) ? n >>> 0 : DEFAULT_SEED;
  }
  set seed(value: number) {
    this.setAttribute("seed", String(value >>> 0));
  }

  get fps(): number | null {
    const raw = this.getAttribute("fps");
    if (raw === null) return null;
    const n = Number(raw);
    return Number.isFinite(n) && n > 0 ? n : null;
  }
  set fps(value: number | null) {
    if (value === null) this.removeAttribute("fps");
    else this.setAttribute("fps", String(value));
  }

  get disabled(): boolean {
    return this.hasAttribute("disabled");
  }
  set disabled(value: boolean) {
    this.toggleAttribute("disabled", value);
  }

  /**
   * Decorative label animation: `"spin"` (default — rotates around its
   * own center, angular speed tracing a sine wave, scale pulsing along
   * with it), `"skew"` (a large, fast, non-repeating-looking skew
   * wobble), `"explode"` (characters burst apart into "pixels" and
   * re-assemble, on a loop), `"tunnel"` (colored depth echoes), or `"none"` to opt out. Some label motion is
   * on by default — pass `text-fx="none"` to get a static label. Disabled
   * automatically under `prefers-reduced-motion: reduce`, same as the
   * canvas animation.
   */
  get textFx(): TextFx {
    const raw = this.getAttribute("text-fx");
    if (raw !== null && (TEXT_FX_VALUES as readonly string[]).includes(raw)) {
      return raw as TextFx;
    }
    return "spin"; // mandatory by default: absent or unrecognized -> spin
  }
  set textFx(value: TextFx) {
    this.setAttribute("text-fx", value);
  }

  /**
   * `true` while a game variant (`flappy`, `runner`, `timing`) is still
   * waiting to be cleared. The label stays hidden and `click` events
   * never reach the page until it flips to `false`, at which point a
   * `game-clear` event is dispatched. Always `false` for visual variants.
   */
  get locked(): boolean {
    return this.gameLocked;
  }

  /** Restart a game variant from scratch, locking it again. */
  resetGame(): void {
    this.recreateCore();
  }

  // ---- internals ----

  private syncDisabled(): void {
    this.buttonEl.disabled = this.disabled;
  }

  private measureDevicePixelSize(): { w: number; h: number } {
    const rect = this.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
    const cssW = rect.width || FALLBACK_CSS_WIDTH;
    const cssH = rect.height || FALLBACK_CSS_HEIGHT;
    return { w: Math.max(1, Math.round(cssW * dpr)), h: Math.max(1, Math.round(cssH * dpr)) };
  }

  private ensureCore(): void {
    if (this.core) return;
    this.ctx = this.canvas.getContext("2d", { alpha: false });
    const { w, h } = this.measureDevicePixelSize();
    this.devW = w;
    this.devH = h;
    this.canvas.width = w;
    this.canvas.height = h;
    this.core = createUselessButton(this.variant, w, h, this.seed);
    this.fpsIntervalSeconds = this.computeFpsInterval();
    this.lockIfGame();
    this.renderStaticFrame();
    this.syncScheduling();
  }

  private recreateCore(): void {
    if (!this.core) return; // not connected yet — ensureCore() will read fresh attrs
    this.core.free();
    this.core = createUselessButton(this.variant, this.devW, this.devH, this.seed);
    this.fpsIntervalSeconds = this.computeFpsInterval();
    this.tickAccumulator = 0;
    this.lockIfGame();
    this.renderStaticFrame();
  }

  private computeFpsInterval(): number {
    const explicit = this.fps;
    if (explicit !== null) return 1 / explicit;
    const preferred = this.core?.preferred_fps() ?? 60;
    return 1 / Math.max(1, preferred);
  }

  private prefersReducedMotion(): boolean {
    return this.reducedMotionQuery.matches;
  }

  private syncScheduling(): void {
    const shouldAnimate =
      this.isConnected && this.isIntersecting && this.core !== null && !this.prefersReducedMotion();
    if (shouldAnimate && !this.unregisterScheduler) {
      this.unregisterScheduler = registerFrameCallback((dt) => this.onFrame(dt));
    } else if (!shouldAnimate && this.unregisterScheduler) {
      this.stopAnimating();
    }
  }

  private stopAnimating(): void {
    this.unregisterScheduler?.();
    this.unregisterScheduler = null;
  }

  private handleResize(entries: ResizeObserverEntry[]): void {
    const entry = entries[entries.length - 1];
    if (!entry || !this.core) return;
    const box = entry.contentBoxSize?.[0];
    const cssW = box ? box.inlineSize : entry.contentRect.width;
    const cssH = box ? box.blockSize : entry.contentRect.height;
    const dpr = Math.min(window.devicePixelRatio || 1, MAX_DPR);
    const nextW = Math.max(1, Math.round(cssW * dpr));
    const nextH = Math.max(1, Math.round(cssH * dpr));
    if (nextW === this.devW && nextH === this.devH) return;
    this.devW = nextW;
    this.devH = nextH;
    this.canvas.width = nextW;
    this.canvas.height = nextH;
    this.core.resize(nextW, nextH);
    this.renderStaticFrame();
  }

  private handleIntersection(entries: IntersectionObserverEntry[]): void {
    const entry = entries[entries.length - 1];
    this.isIntersecting = entry ? entry.isIntersecting : false;
    this.syncScheduling();
  }

  private onReducedMotionChange = (): void => {
    this.syncScheduling();
    if (this.prefersReducedMotion()) {
      this.renderStaticFrame();
      // Freeze on a neutral pose rather than whatever mid-animation
      // transform happened to be active the instant reduced-motion
      // turned on.
      this.resetTextFx();
      // A game can't be played without animation, so it simply unlocks.
      if (this.gameLocked) this.setGameLocked(false);
    }
  };

  // ---- game variants ----

  /**
   * Fresh core: games start locked. Under reduced motion there's no
   * animation loop to play them with, so they start unlocked instead.
   */
  private lockIfGame(): void {
    this.setGameLocked(this.core !== null && this.core.is_game() && !this.prefersReducedMotion());
  }

  private setGameLocked(locked: boolean): void {
    this.gameLocked = locked;
    this.labelEl.classList.toggle("locked", locked);
    this.syncAriaLabel();
  }

  private unlockClearedGame(): void {
    this.setGameLocked(false);
    // Reveal the label from a neutral pose rather than mid-animation.
    this.resetTextFx();
    this.dispatchEvent(new CustomEvent("game-clear", { bubbles: true, composed: true }));
  }

  /**
   * Game input is taken on press (pointerdown / keydown) rather than on
   * `click`, which only fires on release — too late for a flap or a
   * jump to feel responsive. `left` marks a press on the left half of
   * the button, which games that move both ways (`crossy`) read as
   * "go left"; every other game treats both halves the same.
   */
  private sendGameInput(left = false): void {
    if (!this.core || this.disabled) return;
    if (left) this.core.click_left();
    else this.core.click();
  }

  /**
   * Swallows clicks while a game is locked — and also the click that
   * ends the very press which cleared it, since that press began before
   * the game was won. A capture listener on the host runs ahead of every
   * non-capture listener (and `onclick`) the page puts on this element,
   * and ahead of the inner button's own handlers, whether the click came
   * from the inner button or from a programmatic `host.click()`.
   */
  private onHostClickCapture = (ev: MouseEvent): void => {
    const swallow = this.gameLocked || this.pressWhileLocked;
    this.pressWhileLocked = false;
    if (!swallow) return;
    ev.stopImmediatePropagation();
    ev.preventDefault();
  };

  private onKeyDown = (ev: KeyboardEvent): void => {
    // Arrow keys are the keyboard's two halves of the button. They never
    // activate a <button>, so there's no click to swallow afterwards.
    if (ev.key === "ArrowLeft" || ev.key === "ArrowRight") {
      if (!this.gameLocked) return;
      ev.preventDefault();
      if (!ev.repeat) this.sendGameInput(ev.key === "ArrowLeft");
      return;
    }
    if (ev.key !== "Enter" && ev.key !== " ") return;
    this.pressWhileLocked = this.gameLocked;
    if (this.gameLocked && !ev.repeat) this.sendGameInput();
  };

  private applyTheme(): void {
    if (!this.core) return;
    const style = getComputedStyle(this);
    const paper = resolveCssColorToPacked(style.getPropertyValue("--ub-paper"), 0xf5f3ec);
    const ink = resolveCssColorToPacked(style.getPropertyValue("--ub-ink"), 0x1a1a1a);
    const accent = resolveCssColorToPacked(style.getPropertyValue("--ub-accent"), 0xe0503d);
    this.core.set_theme(paper, ink, accent);
  }

  private renderStaticFrame(): void {
    if (!this.core || !this.ctx) return;
    this.applyTheme();
    this.core.tick(0);
    this.paint();
  }

  private paint(): void {
    if (!this.core || !this.ctx) return;
    const image = readFrame(this.core, this.devW, this.devH);
    this.ctx.putImageData(image, 0, 0);
  }

  private onFrame(dt: number): void {
    if (!this.core) return;
    // Label motion runs every frame regardless of the simulation's own
    // fps throttle — `fps` caps how often the wasm sim steps, not how
    // smoothly the text moves.
    this.updateTextFx(dt);
    this.tickAccumulator += dt;
    if (this.tickAccumulator < this.fpsIntervalSeconds) return;
    const usedDt = this.tickAccumulator;
    this.tickAccumulator = 0;
    this.applyTheme();
    this.core.tick(usedDt);
    this.paint();
    if (this.gameLocked && this.core.cleared()) this.unlockClearedGame();
  }

  // ---- label text effects ----

  private onSlotChange = (): void => {
    if (PER_CHAR_FX.has(this.textFx)) this.rebuildFxChars();
    this.syncAriaLabel();
  };

  /**
   * Keeps the real slotted text and the animated `.label-fx` character
   * layer mutually exclusive for every per-character mode (see
   * `PER_CHAR_FX`).
   *
   * Uses `visibility: hidden`, not `display: none`. Both fully remove
   * the slot from rendering (and from accessible-name computation —
   * see the `aria-label` pinning below) so the real text and the
   * animated glyphs are never simultaneously visible. But `display:
   * none` also removes the slot from *layout*: since `.label-fx` is
   * absolutely positioned (`inset: 0`) and contributes nothing to
   * `span[part="label"]`'s own size, collapsing the slot's box left
   * that label span with nothing left to size itself by, so it shrank
   * to ~0 and the character layer rendered pinned to that point instead
   * of centered in the button. `visibility: hidden` hides the slot the
   * same way but keeps its layout box — the label span still sizes
   * itself to the (invisible) real text as normal, and the character
   * layer inherits that same, correctly-centered box.
   */
  private syncLabelMode(): void {
    const mode = this.textFx;
    if (PER_CHAR_FX.has(mode)) {
      this.slotEl.style.visibility = "hidden";
      this.rebuildFxChars();
    } else {
      this.slotEl.style.visibility = "";
      this.labelFxEl.textContent = "";
      this.fxChars = [];
    }
    this.syncAriaLabel();
  }

  /**
   * Hidden text doesn't contribute to the button's accessible name. In a
   * per-character mode the real text is hidden and the animated layer is
   * `aria-hidden` (a visual-only stand-in, not a second copy), and a
   * locked game hides the whole label — so in both cases the name is
   * pinned explicitly.
   */
  private syncAriaLabel(): void {
    const text = this.textContent ?? "";
    if (this.gameLocked) {
      const name = text.trim() ? `${text.trim()} (locked: clear the game to unlock)` : "Locked: clear the game to unlock";
      this.buttonEl.setAttribute("aria-label", name);
    } else if (PER_CHAR_FX.has(this.textFx) && text.trim()) {
      this.buttonEl.setAttribute("aria-label", text);
    } else {
      this.buttonEl.removeAttribute("aria-label");
    }
  }

  /** Rebuilds the per-character spans used by the `PER_CHAR_FX` modes from the host's current (light-DOM) text content. */
  private rebuildFxChars(): void {
    this.labelFxEl.textContent = "";
    this.fxChars = [];
    const text = this.textContent ?? "";
    let i = 0;
    for (const ch of Array.from(text)) {
      const span = document.createElement("span");
      span.className = "ch";
      span.textContent = ch === " " ? " " : ch;
      this.labelFxEl.appendChild(span);
      this.fxChars.push({ el: span, index: i, stagger: i * EXPLODE_STAGGER_SEC });
      i++;
    }
  }

  /** Clears any active label transform/phase state back to neutral. Does not remove the explode `<span>`s — just re-centers them. */
  private resetTextFx(): void {
    this.textFxTime = 0;
    this.spinAngleDeg = 0;
    this.spinCycleT = 0;
    this.spinCycleIndex = 0;
    this.spinCyclePeriodSec = (SPIN_CYCLE_MIN_SEC + SPIN_CYCLE_MAX_SEC) / 2;
    this.spinCycleScaleAmp = (SPIN_SCALE_AMP_MIN + SPIN_SCALE_AMP_MAX) / 2;
    this.labelEl.style.transform = "";
    this.labelEl.style.textShadow = "";
    for (const ch of this.fxChars) {
      ch.el.style.transform = "";
      ch.el.style.opacity = "";
      ch.el.style.textShadow = "";
      ch.el.style.color = "";
    }
  }

  private updateTextFx(dt: number): void {
    const mode = this.textFx;
    if (mode === "none") return;
    this.textFxTime += dt;
    if ((SPECTACLE_TEXT_MODES as readonly string[]).includes(mode)) {
      for (const ch of this.fxChars) {
        paintSpectacleChar(mode as SpectacleTextMode, ch.el, ch.index,
          this.fxChars.length, this.textFxTime);
      }
      return;
    }

    if (mode === "tunnel") {
      const t = this.textFxTime * 2.4;
      const dx = Math.cos(t * 0.7) * 0.9;
      const dy = Math.sin(t * 0.9) * 0.7;
      const shadows = [];
      for (let i = 1; i <= 18; i++) {
        const depth = i + (t * 8) % 1;
        shadows.push(`${dx * depth}px ${dy * depth}px ${i * 0.15}px hsla(${190 + i * 9 + t * 30}, 100%, 65%, ${(1 - i / 20) * 0.65})`);
      }
      this.labelEl.style.textShadow = shadows.join(",");
      this.labelEl.style.transform = `perspective(300px) rotateY(${Math.sin(t * 0.7) * 18}deg) scale(${1 + Math.sin(t * 1.4) * 0.06})`;
      return;
    }

    if (mode === "spin") {
      // Each cycle is a full sine sweep of the angular *speed* (always
      // net-forward — amplitude stays under the base speed — so it
      // reads as "now faster, now slower", never reversing) with scale
      // riding the same wave. Both how long a cycle takes (i.e. how
      // quickly it reaches peak speed, at the quarter-cycle point) and
      // how far it scales up are re-rolled every time a cycle completes,
      // seeded off the cycle count — so consecutive spins vary instead
      // of repeating the exact same ramp forever.
      this.spinCycleT += dt / this.spinCyclePeriodSec;
      if (this.spinCycleT >= 1) {
        this.spinCycleT -= Math.floor(this.spinCycleT);
        this.spinCycleIndex++;
        this.spinCyclePeriodSec =
          SPIN_CYCLE_MIN_SEC + pseudoRandom(this.spinCycleIndex * 13.7 + 1) * (SPIN_CYCLE_MAX_SEC - SPIN_CYCLE_MIN_SEC);
        this.spinCycleScaleAmp =
          SPIN_SCALE_AMP_MIN + pseudoRandom(this.spinCycleIndex * 17.3 + 2) * (SPIN_SCALE_AMP_MAX - SPIN_SCALE_AMP_MIN);
      }
      const wave = Math.sin(this.spinCycleT * Math.PI * 2);
      const speed = SPIN_BASE_DEG_PER_SEC + SPIN_AMPLITUDE_DEG_PER_SEC * wave;
      this.spinAngleDeg += speed * dt;
      const scale = 1 + this.spinCycleScaleAmp * wave;
      this.labelEl.style.transform = `rotate(${this.spinAngleDeg}deg) scale(${scale})`;
      return;
    }

    if (mode === "skew") {
      const t = this.textFxTime;
      const skewX =
        SKEW_AMP_X_DEG * 0.6 * Math.sin(t * SKEW_FREQ_X1_HZ * Math.PI * 2) +
        SKEW_AMP_X_DEG * 0.4 * Math.sin(t * SKEW_FREQ_X2_HZ * Math.PI * 2 + 1.7);
      const skewY =
        SKEW_AMP_Y_DEG * 0.6 * Math.sin(t * SKEW_FREQ_Y1_HZ * Math.PI * 2 + 0.9) +
        SKEW_AMP_Y_DEG * 0.4 * Math.sin(t * SKEW_FREQ_Y2_HZ * Math.PI * 2 + 3.1);
      // `skew()` alone grows a short label's bounding box by less than a
      // wide button's own padding can absorb, so it can stay fully
      // inside the button even at a large angle. Ride an offset along
      // with the skew, scaled to it, so the label actually travels
      // off-axis rather than just shearing in place — this is what
      // makes it reliably poke outside the button's own box instead of
      // staying politely centered inside it.
      const translateX = skewX * SKEW_TRANSLATE_PX_PER_DEG;
      const translateY = skewY * SKEW_TRANSLATE_PX_PER_DEG * 0.6;
      this.labelEl.style.transform = `translate(${translateX}px, ${translateY}px) skew(${skewX}deg, ${skewY}deg)`;
      return;
    }

    if (mode === "snake") {
      // One travelling wave, sampled once per character at its own phase.
      // Because the phase offset is per *index*, the crest moves along
      // the word instead of every letter bobbing together — and each
      // character is turned to the curve's local tangent, which is what
      // makes the row of letters read as a single body following a path.
      const phaseBase = this.textFxTime * SNAKE_SPEED_HZ * Math.PI * 2;
      // Shared sweep: carries the whole body left and right, well past
      // the button's own edges, so the snake travels instead of
      // wriggling on the spot.
      const sweepX = Math.sin(this.textFxTime * SNAKE_SWEEP_HZ * Math.PI * 2) * SNAKE_SWEEP_X_PX;
      for (const ch of this.fxChars) {
        const p = phaseBase - ch.index * SNAKE_PHASE_PER_CHAR;
        const ty = Math.sin(p) * SNAKE_AMP_Y_PX;
        const tx = sweepX + Math.sin(p * 0.5) * SNAKE_AMP_X_PX;

        // d(ty)/d(index): the curve's slope in px per character, turned
        // into an angle using a nominal character advance.
        const slope = (-SNAKE_AMP_Y_PX * SNAKE_PHASE_PER_CHAR * Math.cos(p)) / SNAKE_CHAR_ADVANCE_PX;
        const tangentDeg = Math.atan(slope) * (180 / Math.PI) * SNAKE_TANGENT_FRAC;

        ch.el.style.transform = `translate(${tx}px, ${ty}px) rotate(${tangentDeg}deg)`;
        ch.el.style.opacity = "";
      ch.el.style.textShadow = "";
      ch.el.style.color = "";
      }
      return;
    }

    if (mode === "slot") {
      // Each character is a reel. Within a cycle it decelerates (ease-out
      // cubic) through a whole number of turns and therefore always comes
      // to rest face-on, never stopped edge-on and invisible. Reels stop
      // left to right, hold, then the next cycle spins them all up again.
      for (const ch of this.fxChars) {
        const cyclePos = this.textFxTime / SLOT_PERIOD_SEC;
        const cycleIndex = Math.floor(cyclePos);
        const localSec = (cyclePos - cycleIndex) * SLOT_PERIOD_SEC;

        // Re-rolled per character per cycle so the reels don't settle
        // into a fixed pattern.
        const seed = ch.index * 61.3 + cycleIndex * 157.1;
        const turns =
          SLOT_MIN_TURNS + Math.floor(pseudoRandom(seed + 1) * (SLOT_MAX_TURNS - SLOT_MIN_TURNS + 1));
        const stopAt = SLOT_FIRST_STOP_SEC + ch.index * SLOT_STAGGER_SEC;

        let angleDeg: number;
        if (localSec >= stopAt + SLOT_HOLD_SEC) {
          // Past the hold: spin straight back up for the next cycle, so
          // the reels are already moving when the cycle rolls over
          // instead of twitching from a standstill.
          const spinUp = localSec - (stopAt + SLOT_HOLD_SEC);
          angleDeg = spinUp * 360 * (1.5 + pseudoRandom(seed + 2));
        } else if (localSec >= stopAt) {
          angleDeg = 0; // stopped, face-on
        } else {
          const progress = localSec / stopAt;
          const eased = 1 - (1 - progress) ** 3;
          angleDeg = (1 - eased) * turns * 360;
        }

        // Reel faces darken as they turn away from the viewer, the way a
        // physical drum would.
        const facing = Math.abs(Math.cos((angleDeg * Math.PI) / 180));
        ch.el.style.transform = `perspective(${SLOT_PERSPECTIVE_PX}px) rotateX(${angleDeg}deg)`;
        ch.el.style.opacity = String(0.45 + 0.55 * facing);
      }
      return;
    }

    // mode === "explode": each character bursts out and snaps back,
    // staggered slightly per character. Direction, distance, rotation
    // and peak scale are re-derived every cycle from a hash of (this
    // character's position, this cycle's number) — not stored once and
    // reused — so the same word bursts a different way each time rather
    // than repeating an identical pattern on every loop.
    for (const ch of this.fxChars) {
      const cyclePos = (this.textFxTime + ch.stagger) / EXPLODE_PERIOD_SEC;
      const cycleIndex = Math.floor(cyclePos);
      const localT = cyclePos - cycleIndex;
      const scatter = explodeScatterAmount(localT);

      const seed = ch.index * 97.7 + cycleIndex * 131.3;
      const angle = pseudoRandom(seed + 1) * Math.PI * 2;
      const dx = Math.cos(angle);
      const dy = Math.sin(angle);
      const rot = (pseudoRandom(seed + 2) - 0.5) * 2;
      const radius = EXPLODE_RADIUS_MIN_PX + pseudoRandom(seed + 3) * (EXPLODE_RADIUS_MAX_PX - EXPLODE_RADIUS_MIN_PX);
      const maxRotate =
        EXPLODE_ROTATE_MIN_DEG + pseudoRandom(seed + 4) * (EXPLODE_ROTATE_MAX_DEG - EXPLODE_ROTATE_MIN_DEG);
      const maxScale = EXPLODE_SCALE_MIN + pseudoRandom(seed + 5) * (EXPLODE_SCALE_MAX - EXPLODE_SCALE_MIN);

      const tx = dx * radius * scatter;
      const ty = dy * radius * scatter;
      const rotDeg = rot * maxRotate * scatter;
      const scale = 1 + (maxScale - 1) * scatter;
      ch.el.style.transform = `translate(${tx}px, ${ty}px) rotate(${rotDeg}deg) scale(${scale})`;
      ch.el.style.opacity = String(1 - scatter * 0.7);
    }
  }

  private onPointerDown = (ev: PointerEvent): void => {
    this.pressWhileLocked = this.gameLocked;
    if (!this.gameLocked || ev.button !== 0) return;
    const rect = this.buttonEl.getBoundingClientRect();
    this.sendGameInput(ev.clientX < rect.left + rect.width / 2);
  };

  /** Visual variants don't react to clicks; a cleared game celebrates. */
  private onClick = (): void => {
    if (!this.core || !this.core.is_game()) return;
    this.core.click();
    if (this.prefersReducedMotion()) {
      // No rAF loop is running in reduced-motion mode: advance exactly
      // one step so a click still visibly does something.
      this.applyTheme();
      this.core.tick(1 / 60);
      this.paint();
    }
  };
}
