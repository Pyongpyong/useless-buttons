// The text-fx engine must paint exactly what the element's original in-class
// implementation did. `fixtures/text-fx-golden.json` was captured from that
// implementation (a fake `this` driving `UselessButtonElement.prototype`).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const root = new URL("../", import.meta.url);
const bundled = await build({
  entryPoints: [fileURLToPath(new URL("src-ts/text-fx.ts", root))],
  bundle: true,
  format: "esm",
  write: false,
  platform: "neutral",
});
const { TextFxEngine, TextFxSampler, TEXT_FX_VALUES } = await import(
  "data:text/javascript;base64," + Buffer.from(bundled.outputFiles[0].text).toString("base64")
);
const { times, count, golden } = JSON.parse(
  readFileSync(new URL("scripts/fixtures/text-fx-golden.json", root), "utf8"),
);

const style = () => ({ transform: "", textShadow: "", color: "", opacity: "", fontFamily: "" });
const target = () => ({
  block: { style: style() },
  chars: Object.assign(
    Array.from({ length: count }, (_, i) => ({
      el: { style: style(), dataset: {}, textContent: "AB가나다"[i] },
      index: i,
      stagger: i * 0.02,
    })),
    {},
  ),
});
const snapshot = ({ block, chars }) => ({
  block: { ...block.style },
  chars: chars.map((c) => ({ ...c.el.style, text: c.el.textContent })),
});

test("every shipped mode is covered by the golden file", () => {
  for (const mode of TEXT_FX_VALUES) if (mode !== "none") assert.ok(golden[mode], mode);
});

for (const mode of Object.keys(golden).filter((m) => !m.startsWith("__"))) {
  test(`sampler reproduces ${mode}`, () => {
    const sampler = new TextFxSampler();
    for (const time of times) {
      const t = target();
      sampler.sample(mode, time, t.block, t.chars);
      assert.deepEqual(snapshot(t), golden[mode][time], `${mode} @ ${time}s`);
    }
  });
}

test("incremental playback matches the element's frame loop", () => {
  for (const [mode, expected] of Object.entries(golden.__incremental)) {
    const t = target();
    const engine = new TextFxEngine();
    engine.reset(t.block, t.chars);
    for (let i = 0; i < 90; i++) engine.update(mode, 1 / 60, t.block, t.chars);
    assert.deepEqual(snapshot(t), expected, mode);
  }
});

test("a spin sample does not depend on the order of requests", () => {
  const ask = (sampler, time) => {
    const t = target();
    sampler.sample("spin", time, t.block, t.chars);
    return t.block.style.transform;
  };
  const forward = new TextFxSampler();
  const jumpy = new TextFxSampler();
  ask(jumpy, 8);
  ask(jumpy, 1);
  for (const time of [0.5, 2, 4.1, 7]) assert.equal(ask(jumpy, time), ask(new TextFxSampler(), time));
  assert.equal(ask(forward, 4.1), ask(jumpy, 4.1));
});
