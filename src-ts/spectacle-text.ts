/** Per-character material effects. All timing comes from the shared scheduler;
 * no independent timers, duplicate accessible labels, or CSS animation loops. */
export const SPECTACLE_TEXT_MODES = [
  "blackhole", "chrome", "plasma", "stained-glass", "aurora", "ripple", "hologram", "supernova", "matrix", "zoom", "ricochet", "corridor", "streak",
  "fire", "rain", "fireworks", "snow", "lava",
  "reaction", "spirograph", "kaleidoscope", "pendulum", "lissajous",
  "synthwave", "crt", "pipes", "radar", "metaball",
  "constellation", "ascii",
] as const;
export type SpectacleTextMode = typeof SPECTACLE_TEXT_MODES[number];

/** Characters the `ascii` effect scrambles through. */
const ASCII_NOISE = "#$%&@*+=-:;!?/\\|<>[]{}~^01";

/** Stable per-character pseudo-random value in [0, 1). */
function hash01(n: number): number {
  const x = Math.sin(n * 12.9898 + 78.233) * 43758.5453;
  return x - Math.floor(x);
}

export function paintSpectacleChar(
  mode: SpectacleTextMode, el: HTMLElement, index: number, count: number,
  time: number,
): void {
  const clock = time;
  time *= 2.6;
  const p = index * 0.65;
  const wave = Math.sin(time * 2 + p);
  const offset = index - (count - 1) / 2;
  let transform = "";
  let shadow = "";
  let color = "";
  let opacity = 1;
  switch (mode) {
    case "zoom": {
      const lens = (Math.sin(clock * 4 - index * 0.45) + 1) / 2;
      transform = `scale(${0.75 + lens * 0.8}) rotate(${wave * 8}deg)`;
      shadow = `0 0 ${lens * 9}px currentColor`;
      break;
    }
    case "ricochet": {
      const phase = ((clock * 1.6 + index * 0.14) % 1);
      const bounce = 1 - Math.abs(phase * 2 - 1);
      transform = `translate(${Math.sin(clock * 4 + p) * 5}px, ${-bounce * 20}px) rotate(${wave * 18}deg) scale(${1.15 - bounce * 0.15}, ${0.85 + bounce * 0.15})`;
      shadow = `0 ${5 + bounce * 12}px 5px #0003`;
      break;
    }
    case "corridor": {
      const depth = (Math.sin(clock * 3 - p * 0.35) + 1) / 2;
      transform = `perspective(180px) translateZ(${depth * 38 - 20}px) rotateY(${Math.sin(clock * 2.2 + p * 0.3) * 35}deg)`;
      shadow = `${-offset * 0.7}px 2px 1px #0006, ${-offset * 1.4}px 4px 2px #0003`;
      opacity = 0.55 + depth * 0.45;
      break;
    }
    case "streak": {
      const speed = (Math.sin(clock * 4) + 1) / 2;
      transform = `translateX(${Math.sin(clock * 4 + p) * 5}px) scaleX(${1 + speed * 0.6})`;
      shadow = Array.from({length: 6}, (_, i) => `${-(i + 1) * (1 + speed * 3)}px 0 ${i * 0.4}px rgba(110,220,255,${(6 - i) * 0.09})`).join(",");
      color = "#e9faff";
      break;
    }
    case "matrix": {
      const fall = ((clock * 2.8 + index * 0.17) % 1);
      transform = `translateY(${(fall - 0.5) * 15}px)`;
      color = fall < 0.18 ? "#d3ffe2" : "#43ff7b";
      shadow = `0 -5px 2px #1bce6577, 0 -11px 4px #13854255, 0 0 7px #22ef70`;
      opacity = 0.55 + (1 - fall) * 0.45;
      break;
    }
    case "blackhole": {
      transform = `translateY(${wave * 2}px) rotate(${Math.sin(clock * 1.8) * 12}deg) scale(${1.15 + Math.cos(clock * 1.8) * 0.25})`;
      color = "hsl(35, 100%, 85%)";
      shadow = "0 0 7px #ff8c38, 0 0 15px #d83cff";
      break;
    }
    case "chrome":
      transform = `translateY(${wave * 2}px) scale(${1 + wave * 0.1}, ${1 - wave * 0.16})`;
      color = `hsl(200, ${15 + 20 * wave}%, ${65 + 30 * Math.sin(time * 2.5 + p)}%)`;
      shadow = "0 -1px 0 #fff, 0 2px 1px #122536, 2px 0 4px #43eaff, -2px 0 4px #f06eff";
      break;
    case "plasma": {
      const jitter = Math.sin(time * 19 + p * 9) * Math.sin(time * 7 + p);
      transform = `translate(${jitter}px, ${wave}px)`;
      color = "#ecfaff";
      shadow = `0 0 2px white, ${wave * 4}px 0 6px #5cecff, ${-wave * 4}px 0 10px #b63cff`;
      break;
    }
    case "stained-glass":
      transform = `rotate(${wave * 8}deg) scale(${1 + wave * 0.12})`;
      color = `hsl(${index * 43 + time * 12}, 90%, 75%)`;
      shadow = "1px 1px 0 #182238, -1px -1px 0 #fff8, 0 0 6px currentColor";
      break;
    case "aurora":
      transform = `translateY(${wave * 3}px) skewX(${wave * 7}deg)`;
      color = `hsl(${155 + wave * 60}, 95%, 82%)`;
      shadow = `0 -4px 5px #37ffab, ${wave * 5}px -10px 10px #44cfff, ${-wave * 8}px -18px 16px #9d48ff`;
      break;
    case "ripple":
      transform = `translateY(${Math.sin(time * 3 - p) * 3}px) skewY(${wave * 9}deg)`;
      color = "#d0f8ff";
      shadow = `${wave * 3}px ${5 + wave * 2}px 2px #36c9f688, 0 0 8px #1b9fff`;
      break;
    case "hologram": {
      const slice = Math.max(0, Math.sin(time * 2.5 + p * 0.5)) ** 18;
      transform = `perspective(250px) translateX(${slice * 8}px) rotateY(${wave * 18}deg)`;
      color = "#adffff";
      shadow = "2px 0 0 #ff3ba888, -2px 0 0 #3c8cff99, 0 0 7px #35ffff";
      opacity = 0.8 + wave * 0.2;
      break;
    }
    case "supernova": {
      const kick = Math.exp(-((clock * 2.4) % 1) * 9);
      transform = `rotate(${Math.sin(clock * 3) * 7}deg) scale(${1 + kick * 0.45})`;
      color = "#fff4c9";
      shadow = `0 0 3px white, 0 0 7px #ffbd35, ${wave * 3}px -5px 12px #ff4825`;
      break;
    }
    case "fire": {
      // Letters flicker and stretch upward, with flame-colored glow rising off them.
      const flick = Math.sin(clock * 17 + index * 3.1) * Math.sin(clock * 11 + index * 1.7);
      const rise = (Math.sin(clock * 6 + p) + 1) / 2;
      transform = `translateY(${-rise * 2 - Math.abs(flick) * 2}px) scale(1, ${1 + flick * 0.12})`;
      color = `hsl(${48 - rise * 18}, 100%, ${84 + flick * 8}%)`;
      // A dark outline first, so the letters stay readable over the flames.
      shadow = "0 1px 2px #2a0600, 0 0 2px #2a0600, 0 0 5px #ffcf4a, 0 -4px 8px #ff8a1e, 0 -10px 12px #ff3d00aa";
      break;
    }
    case "rain": {
      // Each letter slowly grows a drip beneath it, which falls and starts over.
      const cycle = (clock * 0.5 + hash01(index)) % 1;
      const drip = cycle < 0.75 ? cycle / 0.75 : 0;
      transform = `translateY(${Math.sin(clock * 1.5 + p) * 1.2}px)`;
      color = "#dff1ff";
      shadow = `0 1px 0 #ffffffaa, 0 ${2 + drip * 10}px ${1 + drip * 2}px rgba(120, 180, 255, ${(0.75 - drip * 0.55).toFixed(2)}), 0 0 6px #6aa8ff88`;
      break;
    }
    case "fireworks": {
      // Letters pop one after another in a fresh color each time.
      const period = 1.8;
      const t = clock + hash01(index + 1) * period;
      const local = (t % period) / period;
      const pop = local < 0.12 ? local / 0.12 : Math.max(0, 1 - (local - 0.12) / 0.5);
      const hue = Math.floor(hash01(index * 31 + Math.floor(t / period)) * 360);
      transform = `scale(${1 + pop * 0.35}) rotate(${(hash01(index + 7) - 0.5) * pop * 24}deg)`;
      color = `hsl(${hue}, 100%, ${72 + pop * 20}%)`;
      shadow = `0 0 ${4 + pop * 10}px hsl(${hue}, 100%, 60%), 0 0 ${pop * 22}px hsl(${hue}, 100%, 55%)`;
      break;
    }
    case "snow": {
      // Icy letters with a cap of snow on top, swaying and now and then shivering.
      const cold = Math.sin(clock * 0.9 + p) > 0.6 ? 1 : 0.2;
      const shiver = Math.sin(clock * 23 + index * 5) * 0.5 * cold;
      transform = `translate(${shiver}px, ${Math.sin(clock * 1.2 + p) * 1.5}px) rotate(${Math.sin(clock * 0.8 + p) * 3}deg)`;
      color = "#eaf6ff";
      shadow = "0 -2px 0 #ffffff, 0 -3px 2px #ffffffcc, 0 0 6px #9cc9ff, 0 1px 1px #3a5a8a";
      break;
    }
    case "lava": {
      // Gooey letters squash and stretch as they slowly rise and sink.
      const squish = Math.sin(clock * 1.6 + p * 0.8);
      transform = `translateY(${Math.sin(clock * 0.7 + p * 0.5) * 4}px) scale(${1 - squish * 0.12}, ${1 + squish * 0.18})`;
      color = `hsl(${40 + squish * 10}, 100%, ${86 + squish * 6}%)`;
      shadow = "0 1px 2px #3a0024, 0 0 2px #3a0024, 0 0 8px #ff3d8e, 0 0 16px #ff9a2e88";
      break;
    }
    case "reaction": {
      // Letters swell and pinch like dividing cells, each on its own rhythm.
      const b = Math.sin(clock * 1.3 + hash01(index) * Math.PI * 2);
      transform = `scale(${1 + b * 0.12}, ${1 - b * 0.1}) rotate(${b * 4}deg)`;
      color = `hsl(${165 + b * 40}, 70%, 84%)`;
      shadow = `0 1px 2px #06221c, 0 0 6px hsla(${165 + b * 40}, 80%, 55%, 0.8)`;
      break;
    }
    case "spirograph": {
      // Each letter rides an epicycle: a small circle carried on a bigger one.
      const a = clock * 2.2 + p;
      transform = `translate(${Math.cos(a) * 3 + Math.cos(-2.5 * a) * 2}px, ${Math.sin(a) * 3 + Math.sin(-2.5 * a) * 2}px)`;
      const hue = (clock * 40 + index * 25) % 360;
      color = `hsl(${hue}, 70%, 38%)`;
      shadow = `0 0 1px #fffdf5, 0 0 5px hsla(${hue}, 80%, 60%, 0.6)`;
      break;
    }
    case "kaleidoscope": {
      // Mirror-symmetric: letters either side of the middle turn opposite ways.
      const side = Math.sign(offset) || 0;
      const turn = Math.sin(clock * 1.8 + Math.abs(offset) * 0.6) * 22 * side;
      const hue = (Math.abs(offset) * 50 + clock * 70) % 360;
      transform = `rotate(${turn}deg) scale(${1 + Math.abs(Math.sin(clock * 1.8 + Math.abs(offset))) * 0.15})`;
      color = `hsl(${hue}, 95%, 82%)`;
      shadow = `0 1px 2px #150a20, 0 0 6px hsl(${(hue + 180) % 360}, 90%, 60%)`;
      break;
    }
    case "pendulum": {
      // Letters swing from their tops like weights, on two mixed frequencies.
      const swing = Math.sin(clock * 2.2 + p * 0.3) * 22 + Math.sin(clock * 3.7 + p * 0.9) * 10;
      transform = `translateY(-0.55em) rotate(${swing}deg) translateY(0.55em)`;
      const hue = (index * 60) % 360;
      color = `hsl(${hue}, 80%, 82%)`;
      shadow = `${-swing * 0.12}px 0 3px hsla(${hue}, 90%, 60%, 0.6), 0 0 1px #000`;
      break;
    }
    case "lissajous": {
      // Green phosphor letters each tracing a tiny 3:2 figure, with a flicker.
      transform = `translate(${Math.sin(clock * 3 + p) * 3}px, ${Math.sin(clock * 2 + p * 0.7) * 2.5}px)`;
      color = "#8dffa8";
      shadow = "0 0 3px #3dff7a, 0 0 9px #1a9e4a";
      opacity = 0.82 + 0.18 * Math.abs(Math.sin(clock * 37 + index));
      break;
    }
    case "synthwave": {
      // Slanted neon lettering, pink to cyan across the word, bobbing to the beat.
      const hue = 315 - (index / Math.max(1, count - 1)) * 125;
      const beat = Math.exp(-((clock * 2) % 1) * 6);
      transform = `skewX(-12deg) translateY(${-beat * 2}px)`;
      color = `hsl(${hue}, 100%, 78%)`;
      shadow = `0 0 2px #ffffff, 0 0 ${6 + beat * 6}px #ff2bd6, 2px 2px 0 #2de2ff`;
      break;
    }
    case "crt": {
      // Colors split red/cyan; now and then a letter glitches sideways.
      const glitch = hash01(index * 13 + Math.floor(clock * 8)) > 0.82;
      const jump = glitch ? (hash01(index + Math.floor(clock * 30)) - 0.5) * 8 : 0;
      transform = `translateX(${jump}px) skewX(${glitch ? jump * 2 : 0}deg)`;
      color = "#eef6ff";
      shadow = `${-1.5 - Math.abs(jump) * 0.3}px 0 0 #ff2a5c, ${1.5 + Math.abs(jump) * 0.3}px 0 0 #2ae0ff`;
      opacity = 0.8 + 0.2 * Math.abs(Math.sin(clock * 50 + index));
      break;
    }
    case "pipes": {
      // Chunky 3D letters extruded down-right, popping in one after another.
      const hue = (index * 55 + 150) % 360;
      const k = ((clock * 0.8 - index * 0.12) % 1.6 + 1.6) % 1.6;
      const pop = k < 0.2 ? 0.6 + 0.4 * (k / 0.2) : 1;
      transform = `scale(${pop})`;
      color = `hsl(${hue}, 70%, 68%)`;
      shadow = `1px 1px 0 hsl(${hue}, 70%, 42%), 2px 2px 0 hsl(${hue}, 70%, 34%), 3px 3px 0 hsl(${hue}, 70%, 26%), 3px 4px 5px #0008`;
      break;
    }
    case "radar": {
      // Letters light up green as the sweep passes over them, then fade.
      const sweep = (clock * 0.7 % 1.5) * Math.max(1, count);
      const since = sweep - index;
      // Never fully dark: the label has to stay readable between sweeps.
      const lit = Math.max(since >= 0 ? Math.exp(-since * 0.7) : 0, 0.35);
      color = `rgb(${Math.round(60 + 160 * lit)}, 255, ${Math.round(110 + 120 * lit)})`;
      shadow = `0 0 ${2 + lit * 10}px rgba(70, 255, 120, ${(0.3 + 0.7 * lit).toFixed(2)})`;
      opacity = 0.55 + 0.45 * lit;
      break;
    }
    case "metaball": {
      // Glossy candy letters that swell and settle like blobs.
      const squish = Math.sin(clock * 2.1 + p);
      const hue = (clock * 30 + index * 40) % 360;
      transform = `scale(${1 + squish * 0.1}, ${1 - squish * 0.08})`;
      color = `hsl(${hue}, 85%, 70%)`;
      shadow = `-1px -1px 0 #ffffffbb, 1px 2px 0 hsl(${hue}, 70%, 38%), 0 4px 6px #0007`;
      break;
    }
    case "constellation": {
      // Letters twinkle like stars and drift slowly on their own paths.
      const tw = 0.65 + 0.35 * Math.sin(clock * (1.5 + hash01(index) * 2) + index * 2.3);
      transform = `translate(${Math.sin(clock * 0.5 + index) * 2}px, ${Math.cos(clock * 0.4 + index * 1.7) * 2}px)`;
      color = "#eef4ff";
      shadow = `0 0 ${3 + tw * 5}px rgba(160, 200, 255, ${(0.4 + 0.5 * tw).toFixed(2)}), 0 0 1px #ffffff`;
      opacity = 0.55 + 0.45 * tw;
      break;
    }
    case "ascii": {
      // Scrambles through random ASCII, then decodes left to right, holds,
      // and scrambles again. The real character is kept on the span.
      const real = (el.dataset.ch ??= el.textContent ?? "");
      const cycle = clock % 4;
      const settled = cycle > 0.3 + index * 0.09 && cycle < 3.4;
      if (real.trim() && !settled) {
        const k = Math.floor(hash01(index * 7 + Math.floor(clock * 16)) * ASCII_NOISE.length);
        el.textContent = ASCII_NOISE.charAt(k);
      } else if (el.textContent !== real) {
        el.textContent = real;
      }
      el.style.fontFamily = "ui-monospace, SFMono-Regular, Menlo, monospace";
      color = settled ? "#b8ffcc" : "#4dff88";
      shadow = "0 0 4px #1fdc62, 0 0 1px #000";
      break;
    }
  }
  el.style.transform = transform;
  el.style.textShadow = shadow;
  el.style.color = color;
  el.style.opacity = String(opacity);
}
