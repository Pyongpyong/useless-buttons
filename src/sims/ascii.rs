//! `ascii` — ASCII-art animation in the spirit of the famous `donut.c`.
//! Spinning 3D tori are projected onto a grid of character cells; each
//! cell takes the brightness of the nearest surface point there and shows
//! it as one of `.,-~:;=!*#$@`, drawn with a tiny built-in bitmap font in
//! terminal green. Wide buttons get several donuts, each on its own spin.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// The luminance ramp, darkest to brightest.
const RAMP: [u8; 12] = *b".,-~:;=!*#$@";
/// 5x7 bitmaps for each character of `RAMP`, one row per byte, bit 4 = left.
const GLYPHS: [[u8; 7]; 12] = [
    [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100], // .
    [0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b00100, 0b01000], // ,
    [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000], // -
    [0b00000, 0b00000, 0b01000, 0b10101, 0b00010, 0b00000, 0b00000], // ~
    [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000], // :
    [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b00100, 0b01000], // ;
    [0b00000, 0b00000, 0b11111, 0b00000, 0b11111, 0b00000, 0b00000], // =
    [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100], // !
    [0b00000, 0b00100, 0b10101, 0b01110, 0b10101, 0b00100, 0b00000], // *
    [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010], // #
    [0b00100, 0b01111, 0b10100, 0b01110, 0b00101, 0b11110, 0b00100], // $
    [0b01110, 0b10001, 0b10111, 0b10101, 0b10111, 0b10000, 0b01110], // @
];
/// Character rows across the frame's height.
const ROWS: usize = 13;
/// Characters are this much narrower than they are tall.
const CHAR_ASPECT: f32 = 0.6;
/// Tube and ring radii of the torus, and the viewer's distance.
const R1: f32 = 1.0;
const R2: f32 = 2.0;
const K2: f32 = 5.0;

struct Donut {
    a: f32,
    b: f32,
    speed: (f32, f32),
}

pub struct Ascii {
    w: f32,
    h: f32,
    donuts: Vec<Donut>,
}

impl Ascii {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut s = Self { w: 1.0, h: 1.0, donuts: Vec::new() };
        s.resize(w, h, rng);
        s
    }

    /// Grid size in character cells.
    fn grid(&self) -> (usize, usize, f32, f32) {
        let cell_h = (self.h / ROWS as f32).max(1.0);
        let cell_w = (cell_h * CHAR_ASPECT).max(1.0);
        let cols = ((self.w / cell_w) as usize).max(1);
        (cols, ROWS, cell_w, cell_h)
    }

    /// Rasterize every donut into a grid of ramp indices (None = empty).
    fn cells(&self) -> Vec<Option<u8>> {
        let (cols, rows, _, _) = self.grid();
        let mut lum: Vec<Option<u8>> = vec![None; cols * rows];
        let mut zbuf = vec![0.0f32; cols * rows];
        let n = self.donuts.len().max(1);
        // Scale so a donut fills most of the height without its near side,
        // enlarged by perspective, spilling off the grid.
        let k1 = rows as f32 * K2 * 0.6 / (2.0 * (R1 + R2));
        for (i, d) in self.donuts.iter().enumerate() {
            let cx = cols as f32 * (i as f32 + 0.5) / n as f32;
            let cy = rows as f32 * 0.5;
            let (sa, ca, sb, cb) = (d.a.sin(), d.a.cos(), d.b.sin(), d.b.cos());
            let mut theta = 0.0f32;
            while theta < TAU {
                let (st, ct) = theta.sin_cos();
                let mut phi = 0.0f32;
                while phi < TAU {
                    let (sp, cp) = phi.sin_cos();
                    // Point on the torus, rotated about X (a) then Z (b).
                    let circle_x = R2 + R1 * ct;
                    let circle_y = R1 * st;
                    let x = circle_x * (cb * cp + sa * sb * sp) - circle_y * ca * sb;
                    let y = circle_x * (sb * cp - sa * cb * sp) + circle_y * ca * cb;
                    let z = K2 + ca * circle_x * sp + circle_y * sa;
                    let ooz = 1.0 / z;
                    let xp = (cx + k1 * ooz * x / CHAR_ASPECT) as isize;
                    let yp = (cy - k1 * ooz * y) as isize;
                    // Surface normal dotted with a light up and behind the viewer.
                    let l = cp * ct * sb - ca * ct * sp - sa * st + cb * (ca * st - ct * sa * sp);
                    if l > 0.0 && xp >= 0 && yp >= 0 && (xp as usize) < cols && (yp as usize) < rows {
                        let idx = yp as usize * cols + xp as usize;
                        if ooz > zbuf[idx] {
                            zbuf[idx] = ooz;
                            lum[idx] = Some(((l * 8.0) as usize).min(RAMP.len() - 1) as u8);
                        }
                    }
                    phi += 0.03;
                }
                theta += 0.08;
            }
        }
        lum
    }
}

impl Sim for Ascii {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        let (cols, rows, _, _) = self.grid();
        // One donut per roughly square stretch of the grid.
        let n = ((cols as f32 * CHAR_ASPECT / rows as f32 / 1.1).round() as usize).clamp(1, 4);
        if self.donuts.len() != n {
            self.donuts = (0..n)
                .map(|_| Donut { a: rng.range_f32(0.0, TAU), b: rng.range_f32(0.0, TAU), speed: (rng.range_f32(0.8, 1.4), rng.range_f32(0.4, 0.8)) })
                .collect();
        }
    }

    fn step(&mut self, dt: f32, _: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.1) } else { 0.0 };
        for d in &mut self.donuts {
            d.a = (d.a + d.speed.0 * dt) % TAU;
            d.b = (d.b + d.speed.1 * dt) % TAU;
        }
    }

    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        frame.fill(Rgb::new(2, 10, 5));
        let (cols, _, cell_w, cell_h) = self.grid();
        // Whole-pixel glyph scale: as big as the 5x7 font fits in a cell.
        let px = (cell_w / 5.4).min(cell_h / 7.2).floor().max(1.0);
        let (gx, gy) = ((cell_w - 5.0 * px) * 0.5, (cell_h - 7.0 * px) * 0.5);
        for (i, cell) in self.cells().into_iter().enumerate() {
            let Some(k) = cell else { continue };
            let (col, row) = (i % cols, i / cols);
            let t = k as f32 / (RAMP.len() - 1) as f32;
            let c = Rgb::new(30, 120, 60).lerp(Rgb::new(200, 255, 210), t);
            let (x0, y0) = (col as f32 * cell_w + gx, row as f32 * cell_h + gy);
            for (r, bits) in GLYPHS[k as usize].iter().enumerate() {
                for b in 0..5 {
                    if bits & (0b10000 >> b) != 0 {
                        frame.rect(x0 + b as f32 * px, y0 + r as f32 * px, px, px, c, 1.0);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ramp_and_font_line_up() {
        assert_eq!(RAMP.len(), GLYPHS.len());
        // Brighter characters have more ink.
        let ink = |g: &[u8; 7]| g.iter().map(|r| r.count_ones()).sum::<u32>();
        assert!(ink(&GLYPHS[0]) < ink(&GLYPHS[11]));
    }

    #[test]
    fn a_donut_fills_a_ring_of_cells_with_a_hole_in_the_middle() {
        let mut rng = Rng::new(1);
        let mut sim = Ascii::new(140, 140, &mut rng);
        // Tilted a quarter turn about X, the ring faces the viewer.
        sim.donuts = vec![Donut { a: std::f32::consts::FRAC_PI_2, b: 0.0, speed: (0.0, 0.0) }];
        let (cols, rows, _, _) = sim.grid();
        let cells = sim.cells();
        let filled = cells.iter().filter(|c| c.is_some()).count();
        assert!(filled > cols * rows / 8, "only {filled} cells drawn");
        // Face-on (no rotation), the very center is the hole.
        assert!(cells[(rows / 2) * cols + cols / 2].is_none());
    }

    #[test]
    fn renders_opaque_changing_frames_at_odd_and_tiny_sizes() {
        let mut rng = Rng::new(2);
        let mut sim = Ascii::new(320, 96, &mut rng);
        for (w, h) in [(321, 97), (1, 1), (0, 0), (4, 7)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, -1.0, 1000.0, 0.1] {
                sim.step(dt, &Input::default(), &mut rng);
            }
            let mut frame = Frame::new(w, h);
            sim.render(&mut frame, &Theme::default());
            assert!(frame.pixels.chunks_exact(4).all(|p| p[3] == 255));
        }
        sim.resize(200, 140, &mut rng);
        let mut frame = Frame::new(200, 140);
        sim.render(&mut frame, &Theme::default());
        let before = frame.pixels.clone();
        sim.step(0.1, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        assert_ne!(before, frame.pixels);
    }
}
