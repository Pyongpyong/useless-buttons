//! Software framebuffer + tiny 2D drawing primitives.
//!
//! Everything here operates on a plain `Vec<u8>` of RGBA bytes. There is no
//! Canvas2D, no `web-sys`, nothing that crosses the JS/wasm boundary per
//! draw call — that boundary is exactly what gets expensive once a
//! simulation has hundreds of objects. JS reads the finished buffer once
//! per frame through a raw pointer.

/// An 8-bit-per-channel color. No alpha here on purpose: simulations only
/// ever composite onto an opaque frame, so alpha lives on the *operation*
/// (`blend_pixel`, `rect`, `disc`, ...), not on the color itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Decode a `0xRRGGBB` packed color, as handed across the wasm boundary
    /// from a CSS custom property resolved on the JS side.
    pub fn from_u32(v: u32) -> Self {
        Rgb {
            r: ((v >> 16) & 0xFF) as u8,
            g: ((v >> 8) & 0xFF) as u8,
            b: (v & 0xFF) as u8,
        }
    }

    /// Linear interpolation between two colors. `t` is clamped to `[0, 1]`.
    pub fn lerp(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        Rgb {
            r: lerp_u8(self.r, other.r, t),
            g: lerp_u8(self.g, other.g, t),
            b: lerp_u8(self.b, other.b, t),
        }
    }

    /// Build a color from HSV. `h` is in degrees and wraps automatically
    /// (any finite value, including negative or > 360, is valid); `s` and
    /// `v` are clamped to `[0, 1]`.
    ///
    /// This is how simulations reach genuinely random, full-spectrum
    /// colors — picking `h` uniformly from `Rng` covers every hue, unlike
    /// interpolating between two fixed theme colors (`lerp`), which can
    /// only ever produce points along one line segment in color space.
    pub fn from_hsv(h: f32, s: f32, v: f32) -> Rgb {
        let h = if h.is_finite() { h.rem_euclid(360.0) } else { 0.0 };
        let s = if s.is_finite() { s.clamp(0.0, 1.0) } else { 0.0 };
        let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };

        let c = v * s;
        let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = v - c;
        let (r1, g1, b1) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Rgb {
            r: ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
            g: ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
            b: ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

/// The three colors every simulation is allowed to paint with. They come
/// from the host page's `--ub-paper` / `--ub-ink` / `--ub-accent` CSS
/// custom properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub paper: Rgb,
    pub ink: Rgb,
    pub accent: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            paper: Rgb::new(0xF5, 0xF3, 0xEC),
            ink: Rgb::new(0x1A, 0x1A, 0x1A),
            accent: Rgb::new(0xE0, 0x4F, 0x3D),
        }
    }
}

/// An RGBA pixel buffer, opaque everywhere (alpha byte is always `255`).
///
/// The buffer lives at a stable address for the lifetime of the struct
/// unless it is resized, at which point the `Vec` is reallocated and the
/// old pointer is invalid — callers on the JS side re-derive the pointer
/// (`frame_ptr`) after every `resize` and, defensively, every frame.
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub pixels: Vec<u8>,
}

impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        let w = w.max(1);
        let h = h.max(1);
        let mut pixels = vec![0u8; w * h * 4];
        // alpha = 255 everywhere, always.
        for px in pixels.chunks_exact_mut(4) {
            px[3] = 255;
        }
        Frame { w, h, pixels }
    }

    pub fn resize(&mut self, w: usize, h: usize) {
        let w = w.max(1);
        let h = h.max(1);
        if w == self.w && h == self.h {
            return;
        }
        *self = Frame::new(w, h);
    }

    #[inline]
    fn index(&self, x: usize, y: usize) -> usize {
        (y * self.w + x) * 4
    }

    #[inline]
    fn in_bounds(&self, x: i64, y: i64) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h
    }

    /// Flood the whole frame with a flat color. Alpha stays 255.
    pub fn fill(&mut self, c: Rgb) {
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = c.r;
            px[1] = c.g;
            px[2] = c.b;
            px[3] = 255;
        }
    }

    /// Blend every pixel a little way towards `target` — the classic
    /// "trailing" effect used before redrawing moving objects each frame.
    ///
    /// `amount` is the fraction of the remaining distance to close, in
    /// `[0, 1]`. Values `<= 0` are a no-op; values `>= 1` snap straight to
    /// `target`.
    ///
    /// Naively computing `old + (target - old) * amount` and truncating to
    /// `u8` rounds *down* on every step. When the remaining difference is
    /// 1 (e.g. 254 fading towards 255) and `amount` is small, `diff *
    /// amount` truncates to 0 forever and the pixel gets stuck one or two
    /// levels short of the target — a permanent, practically invisible
    /// ghost. To guarantee convergence in a bounded number of calls we
    /// always step by at least 1 towards the target when they differ.
    pub fn fade_to(&mut self, target: Rgb, amount: f32) {
        if amount <= 0.0 {
            return;
        }
        if amount >= 1.0 {
            self.fill(target);
            return;
        }
        for px in self.pixels.chunks_exact_mut(4) {
            px[0] = fade_channel(px[0], target.r, amount);
            px[1] = fade_channel(px[1], target.g, amount);
            px[2] = fade_channel(px[2], target.b, amount);
            px[3] = 255;
        }
    }

    /// Alpha-blend a single pixel, clipped silently if out of bounds.
    pub fn blend_pixel(&mut self, x: i64, y: i64, c: Rgb, alpha: f32) {
        if !self.in_bounds(x, y) {
            return;
        }
        let alpha = alpha.clamp(0.0, 1.0);
        if alpha <= 0.0 {
            return;
        }
        let idx = self.index(x as usize, y as usize);
        if alpha >= 1.0 {
            self.pixels[idx] = c.r;
            self.pixels[idx + 1] = c.g;
            self.pixels[idx + 2] = c.b;
        } else {
            self.pixels[idx] = blend_u8(self.pixels[idx], c.r, alpha);
            self.pixels[idx + 1] = blend_u8(self.pixels[idx + 1], c.g, alpha);
            self.pixels[idx + 2] = blend_u8(self.pixels[idx + 2], c.b, alpha);
        }
        self.pixels[idx + 3] = 255;
    }

    /// Axis-aligned filled rectangle in float coordinates, clipped to the
    /// frame. Never panics, including for fully off-frame or
    /// negative-size input.
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: Rgb, alpha: f32) {
        if w <= 0.0 || h <= 0.0 || !x.is_finite() || !y.is_finite() {
            return;
        }
        let x0 = x.floor().max(0.0);
        let y0 = y.floor().max(0.0);
        let x1 = (x + w).ceil().min(self.w as f32);
        let y1 = (y + h).ceil().min(self.h as f32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (x0, x1) = (x0 as usize, x1 as usize);
        let (y0, y1) = (y0 as usize, y1 as usize);
        for py in y0..y1 {
            for px in x0..x1 {
                self.blend_pixel(px as i64, py as i64, c, alpha);
            }
        }
    }

    /// Anti-aliased filled disc, clipped to the frame. Never panics for
    /// any radius or center, including fully off-frame discs or radius
    /// `<= 0`.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, alpha: f32) {
        if r <= 0.0 || !cx.is_finite() || !cy.is_finite() || !r.is_finite() {
            return;
        }
        let pad = 1.0; // 1px anti-aliasing band
        let min_x = (cx - r - pad).floor().max(0.0) as usize;
        let min_y = (cy - r - pad).floor().max(0.0) as usize;
        let max_x = ((cx + r + pad).ceil().max(0.0) as usize).min(self.w);
        let max_y = ((cy + r + pad).ceil().max(0.0) as usize).min(self.h);
        if min_x >= max_x || min_y >= max_y {
            return;
        }
        for py in min_y..max_y {
            for px in min_x..max_x {
                let dx = px as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                // Coverage ramps from 1 (well inside) to 0 (well outside)
                // over a ~1px band straddling the true edge.
                let coverage = (r + 0.5 - dist).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    self.blend_pixel(px as i64, py as i64, c, alpha * coverage);
                }
            }
        }
    }
}

fn fade_channel(old: u8, target: u8, amount: f32) -> u8 {
    if old == target {
        return target;
    }
    let diff = target as f32 - old as f32;
    let mut step = diff * amount;
    if step.abs() < 1.0 {
        // Force at least a 1-unit move towards the target so integer
        // truncation/rounding can never stall the fade permanently.
        step = diff.signum();
    }
    (old as f32 + step).round().clamp(0.0, 255.0) as u8
}

fn blend_u8(old: u8, new: u8, alpha: f32) -> u8 {
    (old as f32 + (new as f32 - old as f32) * alpha)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_u32_decodes_channels() {
        let c = Rgb::from_u32(0x112233);
        assert_eq!(c, Rgb::new(0x11, 0x22, 0x33));
    }

    #[test]
    fn fade_to_reaches_target_exactly() {
        // Small amount, large distance (0 -> 255): must still converge in
        // a bounded number of steps, not stall short.
        let mut frame = Frame::new(2, 2);
        frame.fill(Rgb::new(0, 0, 0));
        let target = Rgb::new(255, 255, 255);
        let mut iterations = 0;
        loop {
            frame.fade_to(target, 0.05);
            iterations += 1;
            let px = &frame.pixels[0..3];
            if px == [255, 255, 255] {
                break;
            }
            assert!(
                iterations < 100_000,
                "fade_to failed to converge to target color"
            );
        }
    }

    #[test]
    fn fade_to_never_overshoots() {
        let mut frame = Frame::new(1, 1);
        frame.fill(Rgb::new(10, 200, 5));
        let target = Rgb::new(250, 0, 5);
        for _ in 0..500 {
            frame.fade_to(target, 0.37);
            let px = &frame.pixels[0..3];
            assert!(px[0] <= 250);
            assert_eq!(px[2], 5);
        }
        assert_eq!(&frame.pixels[0..3], &[250, 0, 5]);
    }

    #[test]
    fn fade_to_zero_amount_is_noop() {
        let mut frame = Frame::new(1, 1);
        frame.fill(Rgb::new(10, 20, 30));
        frame.fade_to(Rgb::new(255, 255, 255), 0.0);
        assert_eq!(&frame.pixels[0..3], &[10, 20, 30]);
    }

    #[test]
    fn alpha_channel_always_opaque() {
        let mut frame = Frame::new(16, 16);
        frame.fill(Rgb::new(1, 2, 3));
        frame.fade_to(Rgb::new(200, 200, 200), 0.3);
        frame.rect(-5.0, -5.0, 30.0, 30.0, Rgb::new(9, 9, 9), 0.5);
        frame.disc(8.0, 8.0, 20.0, Rgb::new(50, 60, 70), 0.7);
        for px in frame.pixels.chunks_exact(4) {
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn disc_clips_without_panicking_off_frame() {
        let mut frame = Frame::new(8, 8);
        // Way off to every side, huge radius, degenerate radius, NaN-ish.
        frame.disc(-1000.0, -1000.0, 5.0, Rgb::new(1, 1, 1), 1.0);
        frame.disc(1000.0, 1000.0, 5.0, Rgb::new(1, 1, 1), 1.0);
        frame.disc(4.0, 4.0, 10_000.0, Rgb::new(1, 1, 1), 1.0);
        frame.disc(4.0, 4.0, 0.0, Rgb::new(1, 1, 1), 1.0);
        frame.disc(4.0, 4.0, -3.0, Rgb::new(1, 1, 1), 1.0);
    }

    #[test]
    fn rect_clips_without_panicking_off_frame() {
        let mut frame = Frame::new(8, 8);
        frame.rect(-100.0, -100.0, 10.0, 10.0, Rgb::new(1, 1, 1), 1.0);
        frame.rect(5.0, 5.0, 1000.0, 1000.0, Rgb::new(1, 1, 1), 1.0);
        frame.rect(2.0, 2.0, -5.0, -5.0, Rgb::new(1, 1, 1), 1.0);
        frame.rect(f32::NAN, 2.0, 5.0, 5.0, Rgb::new(1, 1, 1), 1.0);
    }

    #[test]
    fn frame_resize_to_tiny_size_does_not_panic() {
        let mut frame = Frame::new(320, 96);
        frame.resize(4, 4);
        assert_eq!((frame.w, frame.h), (4, 4));
        frame.fill(Rgb::new(1, 1, 1));
        frame.disc(2.0, 2.0, 10.0, Rgb::new(9, 9, 9), 1.0);
        frame.rect(0.0, 0.0, 100.0, 100.0, Rgb::new(9, 9, 9), 1.0);
        frame.resize(0, 0);
        assert_eq!((frame.w, frame.h), (1, 1));
    }

    #[test]
    fn from_hsv_matches_known_primary_colors() {
        assert_eq!(Rgb::from_hsv(0.0, 1.0, 1.0), Rgb::new(255, 0, 0));
        assert_eq!(Rgb::from_hsv(120.0, 1.0, 1.0), Rgb::new(0, 255, 0));
        assert_eq!(Rgb::from_hsv(240.0, 1.0, 1.0), Rgb::new(0, 0, 255));
    }

    #[test]
    fn from_hsv_zero_saturation_is_gray() {
        let c = Rgb::from_hsv(200.0, 0.0, 0.5);
        assert_eq!(c.r, c.g);
        assert_eq!(c.g, c.b);
    }

    #[test]
    fn from_hsv_wraps_and_never_panics_on_extreme_input() {
        let a = Rgb::from_hsv(0.0, 1.0, 1.0);
        let b = Rgb::from_hsv(360.0, 1.0, 1.0);
        let c = Rgb::from_hsv(-360.0, 1.0, 1.0);
        assert_eq!(a, b);
        assert_eq!(a, c);
        // NaN/infinite input must degrade gracefully, never panic.
        let _ = Rgb::from_hsv(f32::NAN, f32::NAN, f32::NAN);
        let _ = Rgb::from_hsv(f32::INFINITY, 2.0, -5.0);
    }

    #[test]
    fn lerp_clamps_t() {
        let a = Rgb::new(0, 0, 0);
        let b = Rgb::new(100, 100, 100);
        assert_eq!(a.lerp(b, -5.0), a);
        assert_eq!(a.lerp(b, 5.0), b);
    }
}
