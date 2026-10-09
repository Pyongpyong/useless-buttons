/**
 * `<useless-button>` — a real `<button>` with a wasm simulation painted
 * behind its label.
 *
 * Everything that makes it an actual button (keyboard activation, form
 * participation, focus, `:disabled`) comes from using a real
 * `<button type="button">` inside shadow DOM; the canvas is purely
 * decorative (`aria-hidden`) and the visible label is plain slotted text.
 */
import { SPECTACLE_TEXT_MODES, type SpectacleTextMode } from "./spectacle-text.js";
import { EXPLODE_STAGGER_SEC, TEXT_FX_VALUES, TextFxEngine, type FxChar, type TextFx } from "./text-fx.js";
import { registerFrameCallback } from "./scheduler.js";
import { createUselessButton, readFrame, type UselessButton } from "./wasm.js";

const DEFAULT_VARIANT = "swarm";
const DEFAULT_SEED = 1;
const MAX_DPR = 2;
const FALLBACK_CSS_WIDTH = 160;
const FALLBACK_CSS_HEIGHT = 48;


/**
 * Modes that animate each character on its own, rather than transforming
 * the label as a single block. They all share the same machinery: the
 * real slotted text is hidden but keeps its layout box, and a parallel
 * layer of per-character `<span>`s is animated in its place (see
 * `syncLabelMode`).
 */
const PER_CHAR_FX: ReadonlySet<TextFx> = new Set<TextFx>(["explode", "snake", "slot", ...SPECTACLE_TEXT_MODES]);

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
:host([text-fx="streak"]) .label-fx,
:host([text-fx="fire"]) .label-fx,
:host([text-fx="rain"]) .label-fx,
:host([text-fx="fireworks"]) .label-fx,
:host([text-fx="snow"]) .label-fx,
:host([text-fx="lava"]) .label-fx,
:host([text-fx="reaction"]) .label-fx,
:host([text-fx="spirograph"]) .label-fx,
:host([text-fx="kaleidoscope"]) .label-fx,
:host([text-fx="pendulum"]) .label-fx,
:host([text-fx="lissajous"]) .label-fx,
:host([text-fx="synthwave"]) .label-fx,
:host([text-fx="crt"]) .label-fx,
:host([text-fx="pipes"]) .label-fx,
:host([text-fx="radar"]) .label-fx,
:host([text-fx="metaball"]) .label-fx,
:host([text-fx="constellation"]) .label-fx,
:host([text-fx="ascii"]) .label-fx {
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

  private readonly fx = new TextFxEngine();
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
    this.buttonEl.addEventListener("pointerup", this.onPointerRelease);
    this.buttonEl.addEventListener("pointercancel", this.onPointerRelease);
    this.buttonEl.addEventListener("pointerleave", this.onPointerRelease);
    // Registered on the host itself, before any page code can add its
    // own listeners, so it runs first — see `onHostClickCapture`.
    this.addEventListener("click", this.onHostClickCapture, true);
  }

  connectedCallback(): void {
    this.reducedMotionQuery.addEventListener("change", this.onReducedMotionChange);
    this.syncDisabled();
    this.resizeObserver.observe(this);
    this.intersectionObserver.observe(this);
    this.ensureCore();
    // `slotchange` reliably fires for content assigned after connection,
    // but covering the initial-markup case explicitly (rather than
    // relying on timing) is simpler than it is to get wrong.
    this.syncLabelMode();
    // A release can land outside the button (or never reach it, e.g. the
    // window losing focus mid-press), so any release anywhere ends a hold.
    window.addEventListener("pointerup", this.onPointerRelease);
    window.addEventListener("pointercancel", this.onPointerRelease);
    window.addEventListener("blur", this.onPointerRelease);
  }

  disconnectedCallback(): void {
    this.reducedMotionQuery.removeEventListener("change", this.onReducedMotionChange);
    window.removeEventListener("pointerup", this.onPointerRelease);
    window.removeEventListener("pointercancel", this.onPointerRelease);
    window.removeEventListener("blur", this.onPointerRelease);
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

  /** Clears any active label transform/phase state back to neutral. Does not remove the per-character `<span>`s — just re-centers them. */
  private resetTextFx(): void {
    this.fx.reset(this.labelEl, this.fxChars);
  }

  private updateTextFx(dt: number): void {
    this.fx.update(this.textFx, dt, this.labelEl, this.fxChars);
  }

  /**
   * The only game input: a pointer press on the button. It's taken on
   * pointerdown rather than `click`, which only fires on release — too
   * late for a flap or a jump to feel responsive — and it carries where
   * the press landed, as fractions of the button, for games that aim
   * (`django`, `balloon`, `memory`) or steer (`crossy`, `dodge`). The
   * keyboard never plays a game: Enter/Space on a locked button do
   * nothing, since its clicks are swallowed. Visual variants ignore
   * presses; a cleared game celebrates them.
   */
  private onPointerDown = (ev: PointerEvent): void => {
    this.pressWhileLocked = this.gameLocked;
    if (!this.core || this.disabled || !this.core.is_game() || ev.button !== 0) return;
    const rect = this.canvas.getBoundingClientRect();
    this.core.press(
      (ev.clientX - rect.left) / Math.max(1, rect.width),
      (ev.clientY - rect.top) / Math.max(1, rect.height),
    );
    // Some games (`heli`) respond to holding the press, not just to it.
    this.core.hold(true);
    if (!this.gameLocked && this.prefersReducedMotion()) {
      // No rAF loop is running in reduced-motion mode: advance exactly
      // one step so a press on a (necessarily unlocked) game still
      // visibly does something.
      this.applyTheme();
      this.core.tick(1 / 60);
      this.paint();
    }
  };

  /** Letting go, or sliding off the button, ends a hold. */
  private onPointerRelease = (): void => {
    this.core?.hold(false);
  };
}
