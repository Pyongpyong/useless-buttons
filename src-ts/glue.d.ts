/**
 * Ambient type declaration for wasm-pack's generated glue module.
 *
 * `wasm.ts` imports from `"../pkg/useless_buttons_core.js"`, which only
 * exists after `npm run build:wasm` has actually run `wasm-pack`. Without
 * this file, `tsc` would fail to resolve that import in a fresh checkout
 * (or in a lint-only CI step that never builds the wasm crate). Declaring
 * the module ambiently here means `tsc` type-checks fine either way: if
 * `pkg/` exists, TypeScript prefers the real generated `.d.ts`; if it
 * doesn't, this declaration is used instead. Either way the shape must
 * match what `wasm-pack build --target web` actually emits for
 * `src/lib.rs`'s `#[wasm_bindgen]` surface.
 */
declare module "../pkg/useless_buttons_core.js" {
  /** Mirrors the `#[wasm_bindgen] impl UselessButton` surface in Rust. */
  export class UselessButton {
    constructor(variant: string, w: number, h: number, seed: number);
    resize(w: number, h: number): void;
    set_theme(paper: number, ink: number, accent: number): void;
    click(): void;
    click_left(): void;
    press(x: number, y: number): void;
    tick(dt: number): void;
    frame_ptr(): number;
    frame_len(): number;
    preferred_fps(): number;
    is_game(): boolean;
    cleared(): boolean;
    variant(): string;
    /** Frees the wasm-side allocation. Must be called exactly once. */
    free(): void;
  }

  /** The subset of the wasm module's raw exports we actually use. */
  export interface InitOutput {
    readonly memory: WebAssembly.Memory;
  }

  /**
   * Synchronously instantiate from already-in-hand bytes (or a
   * precompiled `WebAssembly.Module`) — this is what lets us skip
   * `fetch()`/a bundler `.wasm` loader entirely by inlining the bytes as
   * base64 at build time.
   */
  export function initSync(
    module: { module: BufferSource | WebAssembly.Module } | BufferSource | WebAssembly.Module,
  ): InitOutput;

  /** The default async loader wasm-pack generates. Unused by this
   * package (we always use `initSync`), declared here for completeness.
   */
  export default function init(
    module_or_path?: { module_or_path?: BufferSource | WebAssembly.Module | string | URL | Request } | BufferSource | WebAssembly.Module | string | URL | Request,
  ): Promise<InitOutput>;
}
