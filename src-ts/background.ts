/**
 * The wasm simulations without the custom element: step one forward by hand and
 * read its pixels. Nothing here touches the DOM scheduler, observers or media
 * queries, so a caller owns the clock (offline rendering, video export, tests).
 */
import { createUselessButton, readFrame, type UselessButton } from "./wasm.js";

export interface BackgroundOptions {
  /** A visual variant name, e.g. `"swarm"`. Game variants are rejected: they need input. */
  variant: string;
  /** Size in device pixels. */
  width: number;
  height: number;
  seed?: number;
  /** Theme colors as `0xRRGGBB`; the element's light-theme defaults when omitted. */
  paper?: number;
  ink?: number;
  accent?: number;
}

export interface Background {
  readonly width: number;
  readonly height: number;
  /** Advance the simulation by `seconds` (0 repaints the current state). */
  tick(seconds: number): void;
  /** A copy of the current pixels; it stays valid after later ticks or wasm memory growth. */
  imageData(): ImageData;
  /** The current pixels encoded as an image data URL (PNG by default). Needs a DOM canvas. */
  toDataURL(type?: string): string;
  /** Release the native simulation. The object must not be used afterwards. */
  free(): void;
}

const DEFAULT_SEED = 1;
const DEFAULT_PAPER = 0xf5f3ec;
const DEFAULT_INK = 0x1a1a1a;
const DEFAULT_ACCENT = 0xe0503d;

export function createBackground(options: BackgroundOptions): Background {
  const { variant } = options;
  const width = Math.max(1, Math.round(options.width));
  const height = Math.max(1, Math.round(options.height));
  const core: UselessButton = createUselessButton(variant, width, height, (options.seed ?? DEFAULT_SEED) >>> 0);
  let canvas: HTMLCanvasElement | null = null;
  const free = () => core.free();
  try {
    if (core.variant() !== variant) throw new Error(`Unknown variant "${variant}".`);
    if (core.is_game()) throw new Error(`"${variant}" is a game and cannot be rendered as a background.`);
    core.set_theme(options.paper ?? DEFAULT_PAPER, options.ink ?? DEFAULT_INK, options.accent ?? DEFAULT_ACCENT);
    core.tick(0);
  } catch (error) {
    free();
    throw error;
  }
  const imageData = () => {
    const view = readFrame(core, width, height);
    const copy = new ImageData(width, height);
    copy.data.set(view.data);
    return copy;
  };
  return {
    width,
    height,
    tick: (seconds) => core.tick(seconds),
    imageData,
    toDataURL(type = "image/png") {
      canvas ??= Object.assign(document.createElement("canvas"), { width, height });
      canvas.getContext("2d")!.putImageData(readFrame(core, width, height), 0, 0);
      return canvas.toDataURL(type);
    },
    free,
  };
}
