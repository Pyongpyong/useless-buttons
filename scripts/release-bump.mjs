// Decide how far to bump the version for a release, from the Conventional
// Commit subjects on main since the last `v*` tag:
//
//   feat!: / fix!: / ... or a "BREAKING CHANGE" footer  -> major
//   feat:                                               -> minor
//   fix: / perf: / refactor: / revert:                  -> patch
//   anything else (docs:, chore:, ci:, test:, ...)      -> none (no release)
//
// Prints the bump ("major" | "minor" | "patch" | "none") and, when run in
// GitHub Actions, also writes it to $GITHUB_OUTPUT as `bump=...`.
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const RANK = { none: 0, patch: 1, minor: 2, major: 3 };
const PATCH_TYPES = new Set(["fix", "perf", "refactor", "revert"]);

/** The bump a single commit asks for. */
export function classify(subject, body = "") {
  // The release workflow's own version commits never trigger another release.
  if (/^chore\(release\)/i.test(subject)) return "none";
  const m = /^(\w+)(?:\([^)]*\))?(!)?:/.exec(subject.trim());
  if (!m) return "none";
  const type = m[1].toLowerCase();
  if (m[2] || /^BREAKING[ -]CHANGE:/m.test(body)) return "major";
  if (type === "feat") return "minor";
  if (PATCH_TYPES.has(type)) return "patch";
  return "none";
}

/** The largest bump any of the commits asks for. */
export function bumpFor(commits) {
  return commits.reduce((best, c) => {
    const b = classify(c.subject, c.body);
    return RANK[b] > RANK[best] ? b : best;
  }, "none");
}

function git(...args) {
  // stderr is dropped: `git describe` complains loudly when there's no tag yet.
  return execFileSync("git", args, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim();
}

/** Commits since the last `v*` tag; with no tag yet, just the latest commit. */
function commitsSinceLastRelease() {
  let range;
  try {
    range = `${git("describe", "--tags", "--abbrev=0", "--match", "v*")}..HEAD`;
  } catch {
    range = "HEAD~1..HEAD";
  }
  const log = git("log", "--format=%s%x1f%b%x1e", range);
  return log
    .split("\x1e")
    .map((entry) => entry.trim())
    .filter(Boolean)
    .map((entry) => {
      const [subject, body = ""] = entry.split("\x1f");
      return { subject, body };
    });
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) {
  const commits = commitsSinceLastRelease();
  const bump = bumpFor(commits);
  for (const c of commits) console.error(`  ${classify(c.subject, c.body).padEnd(5)}  ${c.subject}`);
  console.log(bump);
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `bump=${bump}\n`);
}
