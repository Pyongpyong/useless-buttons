import assert from "node:assert/strict";
import { test } from "node:test";
import { bumpFor, classify } from "./release-bump.mjs";

test("commit types map to semver bumps", () => {
  assert.equal(classify("feat: add a pong game (#7)"), "minor");
  assert.equal(classify("Feat: Add 12 more games (#7)"), "minor");
  assert.equal(classify("fix: give timing a single chance (#8)"), "patch");
  assert.equal(classify("perf(sand): fewer allocations"), "patch");
  assert.equal(classify("refactor: split games module"), "patch");
  assert.equal(classify("docs: describe new backgrounds"), "none");
  assert.equal(classify("chore: bump deps"), "none");
  assert.equal(classify("ci: add release workflow"), "none");
  assert.equal(classify("Merge branch 'main'"), "none");
});

test("breaking changes are a major bump", () => {
  assert.equal(classify("feat!: drop the pointer() API"), "major");
  assert.equal(classify("fix(core)!: rename press()"), "major");
  assert.equal(classify("feat: new input", "BREAKING CHANGE: Input.at replaces left_clicks"), "major");
});

test("the release workflow's own commits never trigger a release", () => {
  assert.equal(classify("chore(release): v0.2.0"), "none");
});

test("the biggest bump across commits wins", () => {
  const c = (subject) => ({ subject, body: "" });
  assert.equal(bumpFor([]), "none");
  assert.equal(bumpFor([c("docs: x"), c("chore: y")]), "none");
  assert.equal(bumpFor([c("fix: a"), c("docs: b")]), "patch");
  assert.equal(bumpFor([c("fix: a"), c("feat: b"), c("fix: c")]), "minor");
  assert.equal(bumpFor([c("feat: a"), c("fix!: b")]), "major");
});
