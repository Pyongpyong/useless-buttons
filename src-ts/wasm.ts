/**
 * Owns the single wasm module instance shared by every `<useless-button>`
 * on the page, and the one bit of unsafe-feeling plumbing in this
 * package: turning a raw pointer + length from wasm linear memory into
 * something `putImageData` can draw.
 */
import { UselessButton, initSync } from "../pkg/useless_buttons_core.js";
import { WASM_BASE64 } from "./wasm-inline.generated.js";

export { UselessButton };

let memory: WebAssembly.Memory | null = null;

function base64ToBytes(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * Initialize the wasm module exactly once, however many
 * `<useless-button>` elements end up on the page. Safe to call
 * repeatedly — after the first call it's a no-op.
 */
export function ensureWasmReady(): void {
  if (memory) return;
  const bytes = base64ToBytes(WASM_BASE64);
  const output = initSync({ module: bytes });
  memory = output.memory;
}

export function createUselessButton(variant: string, w: number, h: number, seed: number): UselessButton {
  ensureWasmReady();
  return new UselessButton(variant, w, h, seed);
}

/**
 * Build a fresh `ImageData` view directly over the instance's pixel
 * buffer in wasm linear memory — no copy.
 *
 * This is deliberately re-created every single frame rather than cached:
 * wasm memory can grow (most likely right after a `resize()` call that
 * needs a bigger `Vec<u8>` on the Rust side), and growing linear memory
 * detaches any typed array views built over the old `ArrayBuffer`. Views
 * are cheap to construct, so we just never let one live longer than one
 * frame.
 */
export function readFrame(instance: UselessButton, w: number, h: number): ImageData {
  ensureWasmReady();
  const ptr = instance.frame_ptr();
  const len = instance.frame_len();
  const bytes = new Uint8ClampedArray(memory!.buffer, ptr, len);
  return new ImageData(bytes, w, h);
}
