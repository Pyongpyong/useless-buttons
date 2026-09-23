//! Green code rain. Tiny bitmap glyphs, independent falling columns, white
//! leading characters and fading green tails; no browser fonts are required.
use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;

const GLYPHS: [[u8; 7]; 18] = [
    [14, 17, 19, 21, 25, 17, 14],
    [4, 12, 4, 4, 4, 4, 14],
    [14, 17, 1, 2, 4, 8, 31],
    [30, 1, 1, 14, 1, 1, 30],
    [2, 6, 10, 18, 31, 2, 2],
    [31, 16, 16, 30, 1, 1, 30],
    [14, 16, 16, 30, 17, 17, 14],
    [31, 1, 2, 4, 8, 8, 8],
    [14, 17, 17, 14, 17, 17, 14],
    [14, 17, 17, 15, 1, 1, 14],
    [31, 1, 2, 4, 4, 8, 16],
    [4, 31, 4, 4, 4, 8, 16],
    [17, 17, 17, 1, 2, 4, 8],
    [31, 4, 4, 31, 4, 4, 4],
    [1, 2, 4, 12, 20, 4, 4],
    [16, 18, 17, 16, 16, 8, 7],
    [31, 1, 1, 31, 1, 1, 31],
    [10, 10, 31, 10, 2, 4, 8],
];
struct Column {
    head: f32,
    speed: f32,
    tail: f32,
    seed: u32,
}
pub struct Matrix {
    cols: Vec<Column>,
    rows: usize,
    pixel: usize,
    clock: f32,
    boost: f32,
    hover: Option<usize>,
}
impl Matrix {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut sim = Self {
            cols: Vec::new(),
            rows: 1,
            pixel: 1,
            clock: 0.0,
            boost: 0.0,
            hover: None,
        };
        sim.resize(w, h, rng);
        sim
    }
}
impl Sim for Matrix {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.pixel = h.max(1).div_ceil(90).max(w.max(h).div_ceil(1200)).max(1);
        let count = w.max(1).div_ceil(self.pixel * 9).min(192);
        self.rows = h.max(1).div_ceil(self.pixel * 11);
        self.cols = (0..count)
            .map(|_| {
                let tail = rng.range_f32(4.0, 13.0);
                Column {
                    head: rng.range_f32(0.0, self.rows as f32 + tail),
                    speed: rng.range_f32(8.0, 24.0),
                    tail,
                    seed: rng.next_u32(),
                }
            })
            .collect();
        self.hover = None;
    }
    fn step(&mut self, dt: f32, input: &Input, _: &mut Rng) {
        let dt = if dt.is_finite() {
            dt.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if input.clicks > 0 {
            self.boost = 1.0;
        }
        self.boost = (self.boost - dt * 0.7).max(0.0);
        self.clock = (self.clock + dt).rem_euclid(1000.0);
        self.hover = if input.hover && input.x.is_finite() && input.x >= 0.0 {
            Some((input.x / (self.pixel * 9) as f32) as usize)
        } else {
            None
        };
        let speed = 1.0 + self.boost * 3.0 + if input.down { 1.5 } else { 0.0 };
        for col in &mut self.cols {
            col.head = (col.head + dt * col.speed * speed).rem_euclid(self.rows as f32 + col.tail);
        }
    }
    fn render(&mut self, frame: &mut Frame, _: &Theme) {
        frame.fill(Rgb::new(0, 5, 2));
        for (x, col) in self.cols.iter().enumerate() {
            for y in 0..self.rows {
                let distance = (col.head - y as f32).rem_euclid(self.rows as f32 + col.tail);
                if distance > col.tail {
                    continue;
                }
                let hash = col
                    .seed
                    .wrapping_add((y as u32).wrapping_mul(2654435761))
                    .wrapping_add((self.clock * 12.0) as u32)
                    .wrapping_mul(2246822519);
                let glyph = &GLYPHS[((hash ^ (hash >> 13)) as usize) % GLYPHS.len()];
                let light = (1.0 - distance / col.tail).powf(1.5);
                let focus = self.hover.map_or(false, |i| i.abs_diff(x) <= 1);
                let color = if distance < 1.0 {
                    Rgb::new(185, 255, 215)
                } else {
                    Rgb::new(
                        (light * 15.0) as u8,
                        (30.0 + light * 190.0 + if focus { 35.0 } else { 0.0 }).min(255.0) as u8,
                        (light * 65.0) as u8,
                    )
                };
                for (gy, row) in glyph.iter().enumerate() {
                    for gx in 0..5 {
                        if row & (1 << (4 - gx)) == 0 {
                            continue;
                        }
                        frame.rect(
                            ((x * 9 + gx + 2) * self.pixel) as f32,
                            ((y * 11 + gy + 1) * self.pixel) as f32,
                            self.pixel as f32,
                            self.pixel as f32,
                            color,
                            1.0,
                        );
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
    fn rain_moves_without_interaction_and_click_accelerates() {
        let mut a = Matrix::new(160, 48, &mut Rng::new(3));
        let mut b = Matrix::new(160, 48, &mut Rng::new(3));
        a.cols[0].head = 0.0;
        b.cols[0].head = 0.0;
        let mut frame = Frame::new(160, 48);
        a.render(&mut frame, &Theme::default());
        let before = frame.pixels.clone();
        a.step(0.05, &Input::default(), &mut Rng::new(1));
        b.step(
            0.05,
            &Input {
                clicks: 1,
                ..Input::default()
            },
            &mut Rng::new(1),
        );
        assert!(b.cols[0].head > a.cols[0].head * 2.0);
        a.render(&mut frame, &Theme::default());
        assert!(before != frame.pixels);
        assert!(frame
            .pixels
            .chunks_exact(4)
            .all(|p| p[1] >= p[0] && p[1] >= p[2] && p[3] == 255));
    }
    #[test]
    fn resizing_and_invalid_dt_are_safe_and_population_is_bounded() {
        let mut rng = Rng::new(8);
        let mut sim = Matrix::new(1, 1, &mut rng);
        for (w, h) in [(0, 0), (1, 7), (161, 49), (4000, 200)] {
            sim.resize(w, h, &mut rng);
            for dt in [f32::NAN, f32::INFINITY, -1.0, 500.0] {
                sim.step(dt, &Input::default(), &mut rng);
                assert!(sim.cols.iter().all(|c| c.head.is_finite()
                    && c.head >= 0.0
                    && c.head < sim.rows as f32 + c.tail));
            }
            assert!(sim.cols.len() <= 192);
            sim.render(&mut Frame::new(w, h), &Theme::default());
        }
    }
}
