// createBackground runs the wasm simulation with no element, scheduler or DOM.
import assert from "node:assert/strict";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { build } from "esbuild";

globalThis.ImageData ??= class ImageData {
  constructor(a, b, c) {
    if (typeof a === "number") {
      this.width = a;
      this.height = b;
      this.data = new Uint8ClampedArray(a * b * 4);
    } else {
      this.data = a;
      this.width = b;
      this.height = c;
    }
  }
};
const bundled = await build({
  entryPoints: [fileURLToPath(new URL("../src-ts/background.ts", import.meta.url))],
  bundle: true,
  format: "esm",
  write: false,
  platform: "neutral",
  mainFields: ["module", "main"],
});
const { createBackground } = await import(
  "data:text/javascript;base64," + Buffer.from(bundled.outputFiles[0].text).toString("base64")
);

const opts = { variant: "swarm", width: 64, height: 40, seed: 7 };
const pixels = (bg) => Buffer.from(bg.imageData().data);

test("is deterministic for a seed and advances with tick", () => {
  const a = createBackground(opts);
  const b = createBackground(opts);
  assert.deepEqual(pixels(a), pixels(b));
  for (let i = 0; i < 20; i++) {
    a.tick(1 / 30);
    b.tick(1 / 30);
  }
  assert.deepEqual(pixels(a), pixels(b));
  assert.notDeepEqual(pixels(a), pixels(createBackground(opts)));
  a.free();
  b.free();
});

test("imageData is a copy that survives later ticks", () => {
  const bg = createBackground(opts);
  const before = bg.imageData();
  const snapshot = Buffer.from(before.data);
  for (let i = 0; i < 10; i++) bg.tick(1 / 30);
  assert.deepEqual(Buffer.from(before.data), snapshot);
  assert.equal(before.width, 64);
  assert.equal(before.height, 40);
  bg.free();
});

test("theme colors change the picture", () => {
  const light = createBackground({ ...opts, paper: 0xffffff, ink: 0x000000 });
  const dark = createBackground({ ...opts, paper: 0x000000, ink: 0xffffff });
  assert.notDeepEqual(pixels(light), pixels(dark));
});

test("rejects games and unknown variants", () => {
  assert.throws(() => createBackground({ ...opts, variant: "flappy" }), /game/);
  assert.throws(() => createBackground({ ...opts, variant: "no-such-variant" }), /Unknown/);
});
