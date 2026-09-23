/**
 * One shared `requestAnimationFrame` loop for every `<useless-button>` on
 * the page. Running a separate rAF per instance is wasteful and, worse,
 * scales badly — a page with a dozen decorative buttons should not mean
 * a dozen independent animation loops fighting the browser's frame
 * budget. Instances opt in and out (connected + on-screen + motion
 * allowed) and the loop itself is cancelled entirely whenever nothing
 * needs it.
 */

export type FrameCallback = (dt: number) => void;

const callbacks = new Set<FrameCallback>();
let rafHandle: number | null = null;
let lastTime: number | null = null;
let pausedForVisibility = false;

function frame(now: number): void {
  rafHandle = null;
  if (pausedForVisibility) return;

  const dt = lastTime === null ? 0 : Math.max(0, (now - lastTime) / 1000);
  lastTime = now;

  for (const cb of callbacks) {
    cb(dt);
  }

  scheduleNext();
}

function scheduleNext(): void {
  if (rafHandle !== null || pausedForVisibility || callbacks.size === 0) return;
  rafHandle = requestAnimationFrame(frame);
}

/**
 * Join the shared loop. Returns an unsubscribe function — call it from
 * `disconnectedCallback` (or whenever the instance should stop
 * animating) or the loop will keep a reference to it forever.
 */
export function registerFrameCallback(cb: FrameCallback): () => void {
  callbacks.add(cb);
  // A callback joining mid-flight shouldn't retroactively receive
  // whatever time has passed since the loop's last frame.
  lastTime = null;
  scheduleNext();
  return () => {
    callbacks.delete(cb);
    if (callbacks.size === 0 && rafHandle !== null) {
      cancelAnimationFrame(rafHandle);
      rafHandle = null;
    }
  };
}

function handleVisibilityChange(): void {
  if (typeof document === "undefined") return;
  if (document.hidden) {
    pausedForVisibility = true;
    if (rafHandle !== null) {
      cancelAnimationFrame(rafHandle);
      rafHandle = null;
    }
  } else {
    pausedForVisibility = false;
    // Don't hand the first post-hidden frame a dt covering the entire
    // time the tab was backgrounded.
    lastTime = null;
    scheduleNext();
  }
}

if (typeof document !== "undefined") {
  document.addEventListener("visibilitychange", handleVisibilityChange);
}
