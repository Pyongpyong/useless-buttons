# useless-buttons

[npm](https://www.npmjs.com/package/useless-buttons) · [Live demo](https://useless-buttons-zeta.vercel.app) · [GitHub](https://github.com/Pyongpyong/useless-buttons)

> A collection of needlessly elaborate buttons — buttons that do far more
> computation than any button should.

Each `<useless-button>` is a real, focusable, form-participating
`<button>` whose background is a live Rust/wasm simulation — a boid
swarm, falling sand, Conway's Game of Life, a cursor-chasing Mandelbrot
zoom, a bouncing-pixel collision chamber, or a first-person maze crawl —
rendered straight into the button's own canvas. Ships as a
dependency-free Web Component, so it works in React, Vue, Svelte, or a
plain `.html` file identically.

- **No Canvas2D calls from Rust.** The Rust side only ever computes an
  RGBA pixel buffer in wasm linear memory; JS reads it once per frame
  through a raw pointer and hands it to `putImageData`. This is what lets
  a swarm variant push several hundred independent objects at 60fps —
  once you have that many objects, per-object calls across the JS/wasm
  boundary (as `web-sys` Canvas2D bindings require) become the bottleneck,
  not the drawing itself.
- **Zero bundler config.** The compiled `.wasm` is base64-inlined into the
  published JS at build time and loaded with `initSync`. You don't need a
  `.wasm` loader, `asset/wasm`, `vite-plugin-wasm`, or anything else.
- **One shared render loop.** However many `<useless-button>`s are on the
  page, there is exactly one `requestAnimationFrame` loop, paused when the
  tab is hidden and per-instance-excluded when off-screen.

## Install

```sh
npm install useless-buttons
```

## Usage

```html
<script type="module">
  import "useless-buttons";
</script>

<useless-button variant="swarm">Flee the swarm</useless-button>
<useless-button variant="sand">Pile up sand</useless-button>
<useless-button variant="life">Game of life</useless-button>
<useless-button variant="fractal">Fractal zoom</useless-button>
<useless-button variant="bounce">Pixel chaos</useless-button>
<useless-button variant="dungeon">Dungeon crawl</useless-button>
```

Importing the package auto-registers `<useless-button>`. It behaves like
a normal `<button>`: it's keyboard-focusable, activates on <kbd>Enter</kbd>
and <kbd>Space</kbd>, participates in forms, and fires a real `click`
event — because under the shadow DOM, it *is* a `<button type="button">`.

```js
document.querySelector("useless-button").addEventListener("click", () => {
  console.log("clicked, as buttons do");
});
```

### If you want a different tag name

```js
import { defineUselessButton } from "useless-buttons";
defineUselessButton("my-button"); // registers an additional alias tag
```

## Variants

| `variant`  | What it does | Interactions |
|---|---|---|
| `swarm` (default, and the fallback for unknown/misspelled values) | A dense flock of boids (separation/alignment/cohesion) computed on a spatial grid | Hover: cursor acts as a predator that pushes boids away. Press: fear increases. Click: a short panic burst raises speed and repulsion. |
| `sand` | A falling-sand cellular automaton, chunky 5-device-px grains | Rains in continuously on its own. Press-and-hold: extra trickle at the cursor. Click: drops a pile. Once every cell is filled, the complete pile stays visible for half a second, then resets and starts filling again in the next color. |
| `life` | Conway's Game of Life, 2 device px per cell, toroidal, advancing at a fixed ~36 generations/sec (3x faster while hovering) regardless of render rate | Whenever cells die off, new clustered seeds sprout elsewhere on the board — population stays lively instead of dwindling to almost nothing. Hover: gentle continuous seeding under the cursor too, not just on click. Click: a bigger seed splash around the cursor. |
| `fractal` | A Mandelbrot explorer, computed at half resolution and upscaled 2×2, `f64` coordinates, adaptive iteration count | Zoom only ever increases — even fully idle it keeps deepening on its own; hovering steers the center towards the cursor and roughly doubles the rate. Continuous boundary-detail probing nudges the view away from flat/boring (solid interior or empty space) patches, and relocates to a new hand-picked coordinate if one is crossed anyway — the fractal never dead-ends on a blank screen. Click: jump straight to the next coordinate. |
| `bounce` | Dozens of pixels bouncing around a fully elastic collision chamber, changing both direction and a genuinely random full-spectrum color on every wall or pixel-pixel hit | Purely ambient — no pointer interaction. The background is never cleared or faded: only the particles' current positions get painted, so every pass any pixel has ever taken through the canvas stays visible as a permanent trail. |
| `dungeon` | A first-person raycaster (DDA, Wolfenstein/DOOM-style) through a 47×35 procedurally-carved maze — a gray stone ceiling, a dark floor, and walls colored per straight run, so a corridor keeps one color along its length and switches at every corner (with alternating faces darkened a touch for extra corner definition) | Purely ambient — no pointer interaction. Walks center-to-center on autopilot forever, turning in place at each cell before moving on, picking a new open direction at every junction and reversing only at dead ends. The maze is deliberately far larger than what's visible so it roams instead of pacing the same few corridors. Unlike `bounce`, this one repaints the whole frame every render — it's a camera view, not a trail. |

An unrecognized or misspelled `variant` (`"SWRAM "`, `"boidz"`, ...) never
throws or renders a broken button — it silently falls back to `swarm`.

### Colors

Every simulation paints its moving/living/procedural elements with
genuinely random full-spectrum colors (HSV hue drawn from `Rng`, not a
gradient between two fixed theme colors) — each boid, sand-pile color
band, live cell, fractal iteration band, bouncing pixel, and dungeon wall
gets its own hue. `--ub-paper` is still the background and `--ub-ink` is
still used for a couple of neutral/structural details (e.g. the
Mandelbrot's interior, the dungeon's floor), but `--ub-accent` is no
longer read by any simulation — it's applied purely as the button
label's own text color instead (see below).

## Attributes / properties

| Attribute | Property | Default | Notes |
|---|---|---|---|
| `variant` | `.variant` | `"swarm"` | Case/whitespace-tolerant. Changing it at runtime tears down and recreates the simulation. |
| `seed` | `.seed` | `1` | Unsigned 32-bit PRNG seed. Same seed + same size ⇒ identical animation. Changing it recreates the simulation. |
| `fps` | `.fps` | the variant's `preferred_fps` (currently 60 for all six) | Caps how often the simulation is stepped, independent of the shared render loop's own rate. |
| `disabled` | `.disabled` | absent | Reflects onto the real, inner `<button disabled>` — native disabled semantics apply (no clicks, no focus, no form submission). |
| `text-fx` | `.textFx` | `"spin"` | Decorative label animation — see below. Always on by default; pass `text-fx="none"` to opt out. |

All five are observed attributes; changing them at runtime through
`setAttribute`/the JS property takes effect immediately.

### `text-fx`

A decorative animation applied to the button's own label text (not the
canvas) — **on by default** (this is `useless-buttons`; a static label
was never really the point). Runs on the same shared render loop as
everything else, and — like the canvas — is disabled entirely under
`prefers-reduced-motion: reduce`, freezing on a neutral, fully-readable
pose rather than partway through a transform. All three are tuned to be
genuinely excessive — the button no longer clips its own label
(`overflow: visible`; the canvas still clips itself to the rounded
corners), so `skew` and `explode` are free to actually leave the
button's box rather than stay politely contained inside it.

| Value | Effect |
|---|---|
| `"spin"` (default) | Rotates continuously around its own center, scaling up and down in sync. Both the angular *speed* and the scale trace the same sine wave — ranging from a slow, small, readable crawl up to several rotations per second at a visibly larger size — fast enough to blur into an unreadable smear at its peak, then ease back down. How long each cycle takes to reach peak speed, and how large the peak scale is, are both re-rolled every cycle, so consecutive spins vary instead of repeating an identical ramp forever. |
| `"skew"` | A large, fast `skew()` wobble (up to ~80°) built from layered sine waves at non-integer-ratio frequencies, riding along with a proportional offset so the label actually travels off-center — the ends visibly swing outside the button's own box at the extremes, not just shear in place inside it. |
| `"explode"` | Splits the label into individual characters that burst outward (fast ease-out), hold scattered for a beat — flung well past the button's edges, tumbling and briefly scaled up — then snap back together (fast ease-in) and rest assembled before the next burst. Each character's direction, distance, rotation and peak scale are re-rolled every cycle, so the same word doesn't burst the same way twice in a row. The original text is fully hidden for as long as `text-fx="explode"` is set (see Accessibility below) — what's on screen is only ever the animated characters, never both at once. |
| `"none"` | Opt out: static label, no animation. |

```html
<useless-button variant="swarm">Spinning</useless-button>          <!-- text-fx defaults to "spin" -->
<useless-button variant="sand" text-fx="skew">Wobbling</useless-button>
<useless-button variant="life" text-fx="explode">Boom!</useless-button>
<useless-button variant="fractal" text-fx="none">Perfectly still</useless-button>
```

`text-fx="explode"` hides the real slotted text (`visibility: hidden` on
its wrapper — not `display: none`, which would collapse the label's
layout box and break centering) for as long as that mode is active — the
animated characters
are `aria-hidden`, since they're a visual stand-in, not a second copy of
the label — so the button's accessible name is pinned explicitly via
`aria-label` on the inner `<button>` instead, taken from the same text.
Switching away from `"explode"` removes that override and restores the
slot, so the real text drives the accessible name again as normal.

## CSS custom properties

Three colors, read from the host's computed style:

| Property | Meaning | Default (light) | Default (dark, via `prefers-color-scheme`) |
|---|---|---|---|
| `--ub-paper` | Simulation background | `#f5f3ec` | `#1c1c1c` |
| `--ub-ink` | Simulation's neutral/structural color (interior fractal points, the dungeon floor, ...) | `#1a1a1a` | `#f2f0e9` |
| `--ub-accent` | The button label's own text color (`color` on the inner `<button>`) — not used by any simulation, which instead paint with random full-spectrum colors (see [Colors](#colors) above) | `#e0503d` | `#ff7a5c` |

Any valid CSS color is accepted — keywords, hex, `hsl()`, `oklch()`, a
chain of `var()` — it's resolved by assigning it to an offscreen
element's `color` and reading back the browser's own normalized
`rgb(r, g, b)`, not parsed by hand.

```css
useless-button {
  --ub-paper: #0d1117;
  --ub-ink: #c9d1d9;
  --ub-accent: #58a6ff;
}
```

Colors are re-read every render tick, so they animate/transition live —
flip a theme toggle, drag a color picker, whatever.

## `part` selectors

Shadow parts exposed for external styling:

| Part | Element |
|---|---|
| `button` | The inner `<button>` |
| `canvas` | The simulation canvas |
| `label` | The `<span>` wrapping the slotted label text |

```css
useless-button::part(button) {
  border-radius: 999px;
}
useless-button::part(label) {
  text-transform: uppercase;
  letter-spacing: 0.04em;
}
```

## React

```jsx
import "useless-buttons";

function App() {
  return (
    <useless-button variant="swarm" onClick={() => console.log("clicked")}>
      Flee the swarm
    </useless-button>
  );
}
```

React ≥19 passes unrecognized lowercase tags through as custom elements
with their attributes set directly, which is all this component needs. If
you're on an older React or use TypeScript with the DOM types, you may
want a small JSX intrinsic-element declaration:

```ts
declare global {
  namespace JSX {
    interface IntrinsicElements {
      "useless-button": React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & {
          variant?: string;
          seed?: number;
          fps?: number;
          "text-fx"?: "none" | "spin" | "skew" | "explode";
        },
        HTMLElement
      >;
    }
  }
}
```

## Vue

```vue
<script setup>
import "useless-buttons";
</script>

<template>
  <useless-button variant="fractal" @click="onClick">Fractal zoom</useless-button>
</template>
```

Tell Vue's compiler to treat it as a custom element rather than a
component (e.g. in `vite.config.js`):

```js
export default {
  vue: {
    template: {
      compilerOptions: {
        isCustomElement: (tag) => tag === "useless-button",
      },
    },
  },
};
```

## Accessibility

- The canvas is `aria-hidden="true"` — it's decorative, and the button's
  accessible name normally comes entirely from its real, visible slotted
  text (`text-fx="spin"`/`"skew"`/`"none"` never touch it).
- The interactive surface is a genuine `<button type="button">`, so
  focus, keyboard activation, and screen reader button semantics all work
  without any ARIA patching.
- **`prefers-reduced-motion: reduce` disables the animation loop
  entirely.** A single static frame is rendered and the simulation only
  advances by exactly one step in response to a click — nothing animates
  on its own. This also disables `text-fx` — the label freezes on a
  neutral, fully-readable pose rather than mid-rotation/skew/scatter (and
  `text-fx="spin"`'s default is exactly that: motion, so reduced-motion
  users always get a plain static label regardless of the default).
- `text-fx="explode"` is the one exception to "the label is always the
  real text": while it's active, the real text is hidden (`display: none`
  on the `<slot>`) and only the animated, `aria-hidden` character spans
  are shown, so the button's accessible name is pinned explicitly via
  `aria-label` (taken from that same text) instead. Switching away from
  `"explode"` removes the override and restores the slot.
- Off-screen buttons (`IntersectionObserver`) and buttons in a hidden tab
  (`document.visibilitychange`) are excluded from the shared render loop
  entirely, not just throttled.

## Building from source

Requires Rust with the `wasm32-unknown-unknown` target, `wasm-pack`, and
Node 18+:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack   # or the official install script
```

```sh
git clone <this repo>
cd useless-buttons
npm install
npm run build   # wasm-pack -> base64-inline -> esbuild -> tsc --declaration
```

Other useful scripts:

```sh
npm test       # cargo test — the simulation logic, verified natively
npm run preview  # cargo run --release --example preview
               # renders preview/{swarm,sand,life,fractal,bounce,dungeon}.gif
               # so you can eyeball a variant without a browser
```

Then open `demo/index.html` behind any static file server (it imports
`../dist/index.js`, so it needs `npm run build` to have run first):

```sh
npx serve .
# or: python3 -m http.server
```

### Deploy the demo

Build a standalone static site locally (requires the same Rust/wasm-pack
toolchain as the package build):

```sh
npm run build:demo
python3 -m http.server 8080 --directory site
# Open http://localhost:8080
```

The generated `site/` directory contains the demo at `/` and the bundled
library at `/dist/index.js`, including its WASM. It does not depend on an
npm release. Re-run the build before each deployment to include changes.

**Vercel:** deploy the generated directory, using the included static-site
configuration to skip cloud builds:

```sh
npx vercel login
npx vercel site --prod
```

**Cloudflare Pages:** create a Direct Upload project in Workers & Pages
and upload the `site/` folder, or use the CLI:

```sh
npx wrangler login
npx wrangler pages project create useless-buttons-demo --production-branch main
npx wrangler pages deploy site --project-name useless-buttons-demo --branch main
```

Project creation is only needed once; choose another project name if needed.
These commands deploy local build output.

### Automatic Vercel previews for pull requests

Use Vercel's native GitHub integration. GitHub Actions and deployment secrets
are not required: Vercel checks out the source, installs Rust/wasm-pack, runs
the tests, builds the demo, and publishes `site/`.

One-time setup:

1. Commit and push `vercel.json`, `scripts/`, and the rest of the source to GitHub.
2. In Vercel, choose **Add New → Project**, connect GitHub, and import the
   repository. For an existing project, connect it under **Settings → Git**.
3. Keep **Root Directory** at the repository root (`.`), choose **Other** as
   the framework preset, and use Node.js 22.x. Remove any old dashboard build
   overrides; the checked-in `vercel.json` supplies these settings:

   | Setting | Value |
   | --- | --- |
   | Install Command | `npm ci` |
   | Build Command | `bash scripts/vercel-build.sh` |
   | Output Directory | `site` |

4. Deploy the project, then push a feature branch and open a PR. Vercel adds
   the Preview deployment status and URL to the PR and rebuilds after new pushes.

The first build includes installation of Rust and wasm-pack, which can take
several minutes. The local `site/` directory stays gitignored; Vercel generates
it from source, including the WASM bundle, on every build.

By default, Vercel also builds branch pushes before a PR is opened. Pushes to
the configured Production Branch (usually `main`), including PR merges, deploy
to production. External fork contributions may require approval in Vercel.

See [Vercel's GitHub integration documentation](https://vercel.com/docs/git/vercel-for-github).

### Why `cargo test` and not wasm-bindgen-test

The `#[wasm_bindgen]` layer in `src/lib.rs` is deliberately **not** gated
behind `#[cfg(target_arch = "wasm32")]`. `wasm-bindgen`'s macros compile
fine on the host target too, so `cargo test` alone exercises the exact
code that ends up in the browser — same pixel math, same PRNG, same
clamping. If it's correct natively, it's correct in wasm.

## Project layout

```
Cargo.toml, src/            Rust: sims + software rasterizer (no Canvas2D)
src-ts/                     TypeScript: web component, scheduler, wasm glue
scripts/inline-wasm.mjs     base64-inlines pkg/*.wasm into a TS constant
examples/preview.rs         renders preview/*.gif from the native lib
demo/index.html             manual browser smoke test + theme controls
```

## License

MIT
