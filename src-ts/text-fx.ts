/**
 * The label animations of `<useless-button>` as a DOM-free engine: it writes
 * inline styles onto whatever style-bearing objects it is handed, so the element
 * and anything else that can host styles (an SVG exporter, a test) share one
 * implementation.
 */
import { SPECTACLE_TEXT_MODES, paintSpectacleChar, type SpectacleTextMode, type FxCharElement, type FxStyle } from "./spectacle-text.js";

export type { FxCharElement, FxStyle };
export type TextFx = "none" | "spin" | "skew" | "explode" | "snake" | "slot" | "tunnel" | SpectacleTextMode;
export const TEXT_FX_VALUES: readonly TextFx[] = ["spin", "skew", "explode", "snake", "slot", "tunnel", "none", ...SPECTACLE_TEXT_MODES];

/** Whole-label modes write to the block's `transform` and `textShadow`. */
export interface FxBlock {
  style: Pick<FxStyle, "transform" | "textShadow">;
}
export interface FxChar {
  el: FxCharElement;
  /** Position within the label, used (with the current cycle number) to
   * seed that character's direction/distance/rotation for each burst. */
  index: number;
  stagger: number;
}
/** Every character of the label; `length` is the full word length even if only some are painted. */
export type FxChars = Iterable<FxChar> & { readonly length: number };

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
export const EXPLODE_STAGGER_SEC = 0.02;
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

export class TextFxEngine {
  time = 0;
  spinAngleDeg = 0;
  spinCycleT = 0; // 0..1 progress within the current spin cycle
  spinCycleIndex = 0;
  spinCyclePeriodSec = (SPIN_CYCLE_MIN_SEC + SPIN_CYCLE_MAX_SEC) / 2;
  spinCycleScaleAmp = (SPIN_SCALE_AMP_MIN + SPIN_SCALE_AMP_MAX) / 2;

  /** Clears the motion state and puts the block and every character back to a neutral pose. */
  reset(block: FxBlock, chars: FxChars): void {
    this.time = 0;
    this.spinAngleDeg = 0;
    this.spinCycleT = 0;
    this.spinCycleIndex = 0;
    this.spinCyclePeriodSec = (SPIN_CYCLE_MIN_SEC + SPIN_CYCLE_MAX_SEC) / 2;
    this.spinCycleScaleAmp = (SPIN_SCALE_AMP_MIN + SPIN_SCALE_AMP_MAX) / 2;
    block.style.transform = "";
    block.style.textShadow = "";
    for (const ch of chars) {
      ch.el.style.transform = "";
      ch.el.style.opacity = "";
      ch.el.style.textShadow = "";
      ch.el.style.color = "";
    }
  }

  /** An independent copy of the motion state, e.g. to sample a fractional step without advancing this one. */
  clone(): TextFxEngine {
    return Object.assign(new TextFxEngine(), this);
  }

  /** Advances `dt` seconds and paints `mode` onto `block` (whole-label modes) or `chars` (per-character modes). */
  update(mode: TextFx, dt: number, block: FxBlock, chars: FxChars): void {
    if (mode === "none") return;
    this.time += dt;
    if ((SPECTACLE_TEXT_MODES as readonly string[]).includes(mode)) {
      for (const ch of chars) {
        paintSpectacleChar(mode as SpectacleTextMode, ch.el, ch.index,
          chars.length, this.time);
      }
      return;
    }

    if (mode === "tunnel") {
      const t = this.time * 2.4;
      const dx = Math.cos(t * 0.7) * 0.9;
      const dy = Math.sin(t * 0.9) * 0.7;
      const shadows = [];
      for (let i = 1; i <= 18; i++) {
        const depth = i + (t * 8) % 1;
        shadows.push(`${dx * depth}px ${dy * depth}px ${i * 0.15}px hsla(${190 + i * 9 + t * 30}, 100%, 65%, ${(1 - i / 20) * 0.65})`);
      }
      block.style.textShadow = shadows.join(",");
      block.style.transform = `perspective(300px) rotateY(${Math.sin(t * 0.7) * 18}deg) scale(${1 + Math.sin(t * 1.4) * 0.06})`;
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
      block.style.transform = `rotate(${this.spinAngleDeg}deg) scale(${scale})`;
      return;
    }

    if (mode === "skew") {
      const t = this.time;
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
      block.style.transform = `translate(${translateX}px, ${translateY}px) skew(${skewX}deg, ${skewY}deg)`;
      return;
    }

    if (mode === "snake") {
      // One travelling wave, sampled once per character at its own phase.
      // Because the phase offset is per *index*, the crest moves along
      // the word instead of every letter bobbing together — and each
      // character is turned to the curve's local tangent, which is what
      // makes the row of letters read as a single body following a path.
      const phaseBase = this.time * SNAKE_SPEED_HZ * Math.PI * 2;
      // Shared sweep: carries the whole body left and right, well past
      // the button's own edges, so the snake travels instead of
      // wriggling on the spot.
      const sweepX = Math.sin(this.time * SNAKE_SWEEP_HZ * Math.PI * 2) * SNAKE_SWEEP_X_PX;
      for (const ch of chars) {
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
      for (const ch of chars) {
        const cyclePos = this.time / SLOT_PERIOD_SEC;
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
    for (const ch of chars) {
      const cyclePos = (this.time + ch.stagger) / EXPLODE_PERIOD_SEC;
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
}

/**
 * Evaluates the animation at an arbitrary time. Stateless modes are a pure
 * function of `seconds`; `spin` integrates a randomized speed curve, so it is
 * replayed from zero in fixed steps, and forward playback resumes from the last
 * step. The result never depends on which times were requested before.
 */
export class TextFxSampler {
  private spin: { engine: TextFxEngine; step: number } | null = null;
  constructor(private readonly stepsPerSecond = 30) {}

  sample(mode: TextFx, seconds: number, block: FxBlock, chars: FxChars): void {
    if (mode === "none") return;
    const time = Math.max(0, seconds);
    if (mode !== "spin") {
      const engine = new TextFxEngine();
      engine.time = time;
      engine.update(mode, 0, block, chars);
      return;
    }
    const step = Math.floor(time * this.stepsPerSecond);
    if (!this.spin || step < this.spin.step) {
      const engine = new TextFxEngine();
      engine.reset(block, chars);
      this.spin = { engine, step: 0 };
    }
    // Whole steps advance the cached state; paint only the remainder on a copy.
    const idle: FxBlock = { style: { transform: "", textShadow: "" } };
    while (this.spin.step < step) {
      this.spin.engine.update("spin", 1 / this.stepsPerSecond, idle, chars);
      this.spin.step++;
    }
    this.spin.engine.clone().update("spin", time - step / this.stepsPerSecond, block, chars);
  }
}
