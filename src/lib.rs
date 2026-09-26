//! `useless-buttons-core` — the Rust half of `useless-buttons`.
//!
//! This crate computes RGBA pixel buffers for a handful of pointless but
//! elaborate simulations. It never touches Canvas2D or any other browser
//! API: JS reads the finished buffer straight out of wasm linear memory
//! and hands it to `putImageData`. That split is the whole point — once a
//! simulation has hundreds of objects, per-object calls across the
//! JS/wasm boundary (as `web-sys` Canvas2D bindings would require) become
//! the dominant cost, not the drawing itself.
//!
//! Deliberately **not** gated behind `#[cfg(target_arch = "wasm32")]`: the
//! `wasm-bindgen` attribute macro compiles fine natively too, which means
//! `cargo test` alone exercises the exact same code path that ends up in
//! the browser. If it passes natively, the pixels it produces in wasm are
//! the same pixels.

pub mod paint;
pub mod rng;
pub mod sims;

use paint::{Rgb, Theme};
use rng::Rng;
use sims::{Input, Sim, Variant};
use wasm_bindgen::prelude::*;

/// The whole runtime state of one `<useless-button>` instance.
#[wasm_bindgen]
pub struct UselessButton {
    sim: Box<dyn Sim>,
    frame: paint::Frame,
    theme: Theme,
    rng: Rng,
    input: Input,
    variant: Variant,
}

#[wasm_bindgen]
impl UselessButton {
    /// Create a new instance. `variant` is parsed leniently — an unknown
    /// or misspelled name falls back to `"swarm"` rather than throwing,
    /// because a typo in a `variant` HTML attribute must never produce a
    /// broken button.
    #[wasm_bindgen(constructor)]
    pub fn new(variant: &str, w: u32, h: u32, seed: u32) -> UselessButton {
        let variant = Variant::parse(variant);
        let mut rng = Rng::new(seed);
        let w = (w as usize).max(1);
        let h = (h as usize).max(1);
        let sim = variant.make(w, h, &mut rng);
        UselessButton {
            sim,
            frame: paint::Frame::new(w, h),
            theme: Theme::default(),
            rng,
            input: Input::default(),
            variant,
        }
    }

    /// Resize the backing frame and let the simulation re-layout for the
    /// new dimensions. Safe to call with any size, including 0 or very
    /// small values (a canvas mid-`ResizeObserver` thrash).
    pub fn resize(&mut self, w: u32, h: u32) {
        let w = (w as usize).max(1);
        let h = (h as usize).max(1);
        self.frame.resize(w, h);
        self.sim.resize(w, h, &mut self.rng);
    }

    /// Set the three theme colors, each packed as `0xRRGGBB`.
    pub fn set_theme(&mut self, paper: u32, ink: u32, accent: u32) {
        self.theme = Theme {
            paper: Rgb::from_u32(paper),
            ink: Rgb::from_u32(ink),
            accent: Rgb::from_u32(accent),
        };
    }

    /// Register a press (game input; visual variants ignore it). Consumed
    /// (reset to 0) on the next `tick`.
    pub fn click(&mut self) {
        self.input.clicks = self.input.clicks.saturating_add(1);
    }

    /// Register a press on the left half of the button. Counts as a click
    /// too, for games that don't care which side was pressed.
    pub fn click_left(&mut self) {
        self.click();
        self.input.left_clicks = self.input.left_clicks.saturating_add(1);
    }

    /// Advance the simulation by `dt` seconds and render the result into
    /// the internal frame buffer. `dt` is not assumed to be small or even
    /// finite-and-sane — each simulation clamps it internally.
    pub fn tick(&mut self, dt: f32) {
        self.sim.step(dt, &self.input, &mut self.rng);
        self.sim.render(&mut self.frame, &self.theme);
        self.input = Input::default();
    }

    /// Pointer to the start of the RGBA pixel buffer in wasm linear
    /// memory. Only valid until the next allocation that could grow
    /// memory (including the next `resize`) — callers must re-derive a
    /// typed array view from this every frame rather than caching it.
    pub fn frame_ptr(&self) -> *const u8 {
        self.frame.pixels.as_ptr()
    }

    /// Length, in bytes, of the buffer at `frame_ptr` (`w * h * 4`).
    pub fn frame_len(&self) -> usize {
        self.frame.pixels.len()
    }

    pub fn preferred_fps(&self) -> f32 {
        self.sim.preferred_fps()
    }

    /// Whether this variant is a game, i.e. its label and clicks should
    /// stay locked until `cleared()` turns true.
    pub fn is_game(&self) -> bool {
        self.variant.is_game()
    }

    /// Games only: `true` once the game has been beaten. Never reverts.
    pub fn cleared(&self) -> bool {
        self.sim.cleared()
    }

    /// The variant actually running (after typo/fallback resolution), as
    /// a stable lowercase string.
    pub fn variant(&self) -> String {
        self.variant.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructs_and_ticks_for_every_variant() {
        for name in ["swarm", "sand", "life", "fractal", "bounce", "dungeon", "starry", "voronoi", "hyperdrive", "tunnel", "matrix", "blackhole", "chrome", "plasma", "stained-glass", "aurora", "ripple", "hologram", "supernova", "flappy", "runner", "timing", "crossy"] {
            let mut ub = UselessButton::new(name, 320, 96, 1);
            assert_eq!(ub.variant(), name);
            for _ in 0..10 {
                ub.tick(1.0 / 60.0);
            }
            assert_eq!(ub.frame_len(), 320 * 96 * 4);
            let ptr = ub.frame_ptr();
            assert!(!ptr.is_null());
        }
    }

    #[test]
    fn only_games_start_locked() {
        for name in ["flappy", "runner", "timing", "crossy"] {
            let ub = UselessButton::new(name, 320, 96, 1);
            assert!(ub.is_game() && !ub.cleared(), "{name}");
        }
        let ub = UselessButton::new("swarm", 320, 96, 1);
        assert!(!ub.is_game() && !ub.cleared());
    }

    #[test]
    fn unknown_variant_falls_back_to_swarm() {
        let ub = UselessButton::new("totally-bogus", 320, 96, 1);
        assert_eq!(ub.variant(), "swarm");
    }

    #[test]
    fn resize_updates_frame_len() {
        let mut ub = UselessButton::new("sand", 320, 96, 1);
        ub.tick(1.0 / 60.0);
        assert_eq!(ub.frame_len(), 320 * 96 * 4);
        ub.resize(4, 4);
        ub.tick(1.0 / 60.0);
        assert_eq!(ub.frame_len(), 4 * 4 * 4);
    }

    #[test]
    fn frame_pixels_are_always_opaque() {
        let mut ub = UselessButton::new("life", 64, 64, 1);
        ub.set_theme(0xF5F3EC, 0x1A1A1A, 0xE04F3D);
        for _ in 0..30 {
            ub.tick(1.0 / 60.0);
        }
        let ptr = ub.frame_ptr();
        let len = ub.frame_len();
        // SAFETY: `ub` is alive and hasn't been resized since `tick`.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        for px in bytes.chunks_exact(4) {
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn click_is_consumed_after_one_tick() {
        let mut ub = UselessButton::new("flappy", 320, 96, 1);
        ub.click();
        assert_eq!(ub.input.clicks, 1);
        ub.tick(1.0 / 60.0);
        assert_eq!(ub.input.clicks, 0);
    }

    #[test]
    fn zero_size_construction_does_not_panic() {
        let mut ub = UselessButton::new("fractal", 0, 0, 1);
        ub.tick(1.0 / 60.0);
        assert!(ub.frame_len() > 0);
    }
}
