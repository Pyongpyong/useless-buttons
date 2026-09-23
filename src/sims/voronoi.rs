//! `voronoi` — a Voronoi diagram over drifting sites.
//!
//! Every pixel is colored by whichever site is nearest, so the cell
//! boundaries fall out of the coloring rather than being drawn as
//! geometry: no edge list, no Fortune's algorithm, no incremental
//! structure to keep consistent while the sites move. The sites drift
//! continuously and the whole tessellation re-forms every frame, cells
//! swelling and pinching off as their owners pass each other.
//!
//! Cost is `sites * pixels`, which is why it's sampled at half
//! resolution (2x2 blocks, same as `fractal`) and the site count is
//! capped: a few dozen sites over ~8k samples is nothing, but it grows
//! as the product of the two.

use super::{Input, Sim};
use crate::paint::{Frame, Rgb, Theme};
use crate::rng::Rng;
use std::f32::consts::TAU;

/// Edge of a rendered block, in device px. The diagram is all large flat
/// regions, so halving the resolution costs almost nothing visually.
const BLOCK_PX: usize = 2;

const MIN_SITES: usize = 8;
const MAX_SITES: usize = 28;

const SPEED_MIN: f32 = 6.0; // px/sec
const SPEED_MAX: f32 = 26.0;

/// How close the two nearest sites have to be, in device px, for a pixel
/// to count as sitting on the boundary between them. This is what draws
/// the diagram's edges.
const EDGE_PX: f32 = 1.6;

/// While hovering, the cursor joins in as one more site, so the diagram
/// opens a cell that follows the pointer around. It gets a fixed, hot
/// color so it stands out from the drifting ones.
const CURSOR_HUE: f32 = 6.0;
const CURSOR_SAT: f32 = 0.85;
const CURSOR_VAL: f32 = 1.0;
const MAX_DT: f32 = 1.0 / 20.0;

#[derive(Clone, Copy)]
struct Site {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hue: f32,
    sat: f32,
    val: f32,
}

pub struct Voronoi {
    w: f32,
    h: f32,
    sites: Vec<Site>,
    /// Where the pointer was at the last `step`, if it was over the
    /// button. `render` has no access to `Input`, so hover state has to
    /// be carried across from the step that observed it.
    cursor: Option<(f32, f32)>,
}

fn target_count(w: usize, h: usize) -> usize {
    ((w * h) / 1400).clamp(MIN_SITES, MAX_SITES)
}

impl Voronoi {
    pub fn new(w: usize, h: usize, rng: &mut Rng) -> Self {
        let mut v =
            Voronoi { w: w.max(1) as f32, h: h.max(1) as f32, sites: Vec::new(), cursor: None };
        v.populate(rng);
        v
    }

    fn populate(&mut self, rng: &mut Rng) {
        let n = target_count(self.w as usize, self.h as usize);
        self.sites.clear();
        self.sites.reserve(n);
        for _ in 0..n {
            let dir = rng.range_f32(0.0, TAU);
            let speed = rng.range_f32(SPEED_MIN, SPEED_MAX);
            self.sites.push(Site {
                x: rng.range_f32(0.0, self.w),
                y: rng.range_f32(0.0, self.h),
                vx: dir.cos() * speed,
                vy: dir.sin() * speed,
                hue: rng.range_f32(0.0, 360.0),
                // Vary saturation and value too, not just hue: neighboring
                // cells that happen to land on nearby hues would otherwise
                // be hard to tell apart at equal brightness.
                sat: rng.range_f32(0.45, 0.9),
                val: rng.range_f32(0.55, 0.95),
            });
        }
    }

    fn recolor(&mut self, rng: &mut Rng) {
        for s in &mut self.sites {
            s.hue = rng.range_f32(0.0, 360.0);
            s.sat = rng.range_f32(0.45, 0.9);
            s.val = rng.range_f32(0.55, 0.95);
        }
    }

    #[cfg(test)]
    pub(crate) fn site_count(&self) -> usize {
        self.sites.len()
    }

    #[cfg(test)]
    pub(crate) fn positions(&self) -> Vec<(f32, f32)> {
        self.sites.iter().map(|s| (s.x, s.y)).collect()
    }

    #[cfg(test)]
    pub(crate) fn hues(&self) -> Vec<f32> {
        self.sites.iter().map(|s| s.hue).collect()
    }
}

impl Sim for Voronoi {
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng) {
        self.w = w.max(1) as f32;
        self.h = h.max(1) as f32;
        if self.sites.len() != target_count(w, h) {
            self.populate(rng);
        } else {
            for s in &mut self.sites {
                s.x = s.x.clamp(0.0, self.w);
                s.y = s.y.clamp(0.0, self.h);
            }
        }
    }

    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng) {
        self.cursor = if input.hover && input.x.is_finite() && input.y.is_finite() {
            Some((input.x, input.y))
        } else {
            None
        };

        let dt = if dt.is_finite() { dt.clamp(0.0, MAX_DT) } else { 0.0 };
        if dt <= 0.0 || self.sites.is_empty() || self.w <= 0.0 || self.h <= 0.0 {
            return;
        }

        if input.clicks > 0 {
            self.recolor(rng);
        }

        let (w, h) = (self.w, self.h);
        for s in &mut self.sites {
            s.x += s.vx * dt;
            s.y += s.vy * dt;
            // Bounce rather than wrap: a site that wraps drags its whole
            // cell across the canvas in one frame.
            if s.x < 0.0 {
                s.x = 0.0;
                s.vx = s.vx.abs();
            } else if s.x > w {
                s.x = w;
                s.vx = -s.vx.abs();
            }
            if s.y < 0.0 {
                s.y = 0.0;
                s.vy = s.vy.abs();
            } else if s.y > h {
                s.y = h;
                s.vy = -s.vy.abs();
            }
        }
    }

    fn render(&mut self, frame: &mut Frame, theme: &Theme) {
        if self.sites.is_empty() {
            frame.fill(theme.paper);
            return;
        }

        let edge_color = theme.ink;
        let mut by = 0usize;
        while by < frame.h {
            let mut bx = 0usize;
            while bx < frame.w {
                let px = bx as f32 + BLOCK_PX as f32 * 0.5;
                let py = by as f32 + BLOCK_PX as f32 * 0.5;

                // Nearest and second-nearest site. The gap between the two
                // is what tells us we're on a cell boundary -- no edge
                // geometry needed.
                let mut best = f32::INFINITY;
                let mut second = f32::INFINITY;
                let mut best_i = 0usize;
                for (i, s) in self.sites.iter().enumerate() {
                    let dx = px - s.x;
                    let dy = py - s.y;
                    let d2 = dx * dx + dy * dy;
                    if d2 < best {
                        second = best;
                        best = d2;
                        best_i = i;
                    } else if d2 < second {
                        second = d2;
                    }
                }

                // The pointer competes as one more site while it's over
                // the button, carving its own cell out of whatever it's
                // standing on.
                let mut cursor_owns = false;
                if let Some((cx, cy)) = self.cursor {
                    let dx = px - cx;
                    let dy = py - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 < best {
                        second = best;
                        best = d2;
                        cursor_owns = true;
                    } else if d2 < second {
                        second = d2;
                    }
                }

                let mut color = if cursor_owns {
                    Rgb::from_hsv(CURSOR_HUE, CURSOR_SAT, CURSOR_VAL)
                } else {
                    let site = self.sites[best_i];
                    Rgb::from_hsv(site.hue, site.sat, site.val)
                };

                // Equidistant from two sites => on the border between
                // their cells. Comparing the actual distances (not the
                // squares) keeps the line an even width everywhere
                // instead of thinning out near the sites.
                if second.is_finite() && second.sqrt() - best.sqrt() < EDGE_PX {
                    color = edge_color;
                }

                let bw = BLOCK_PX.min(frame.w - bx) as f32;
                let bh = BLOCK_PX.min(frame.h - by) as f32;
                frame.rect(bx as f32, by as f32, bw, bh, color, 1.0);
                bx += BLOCK_PX;
            }
            by += BLOCK_PX;
        }

        // Mark the sites themselves, so it reads as "points with a
        // diagram around them" rather than abstract colored blobs.
        for s in &self.sites {
            frame.disc(s.x, s.y, 1.8, theme.ink, 0.9);
            frame.disc(s.x, s.y, 0.9, theme.paper, 0.9);
        }
    }

    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Theme;

    fn make(w: usize, h: usize, seed: u32) -> Voronoi {
        let mut rng = Rng::new(seed);
        Voronoi::new(w, h, &mut rng)
    }

    #[test]
    fn site_count_within_bounds_for_any_size() {
        for (w, h) in [(320, 96), (1, 1), (4, 4), (4000, 4000)] {
            let sim = make(w, h, 1);
            assert!(sim.site_count() >= MIN_SITES && sim.site_count() <= MAX_SITES);
        }
    }

    #[test]
    fn sites_keep_moving_and_stay_on_canvas() {
        let mut rng = Rng::new(2);
        let mut sim = make(320, 96, 2);
        let before = sim.positions();
        for _ in 0..2000 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let after = sim.positions();
        let moved = before
            .iter()
            .zip(after.iter())
            .filter(|((x0, y0), (x1, y1))| (x0 - x1).abs() > 0.5 || (y0 - y1).abs() > 0.5)
            .count();
        assert_eq!(moved, before.len(), "some sites never moved");

        for (x, y) in after {
            assert!(x.is_finite() && y.is_finite());
            assert!((0.0..=320.0).contains(&x), "x={x} escaped");
            assert!((0.0..=96.0).contains(&y), "y={y} escaped");
        }
    }

    #[test]
    fn click_rerolls_the_palette() {
        let mut rng = Rng::new(3);
        let mut sim = make(320, 96, 3);
        let before = sim.hues();
        let input = Input { x: 0.0, y: 0.0, hover: false, down: false, clicks: 1 };
        sim.step(1.0 / 60.0, &input, &mut rng);
        let after = sim.hues();
        let changed = before.iter().zip(after.iter()).filter(|(a, b)| (*a - *b).abs() > 1e-6).count();
        assert!(changed > 0, "click left every cell the same color");
    }

    #[test]
    fn every_pixel_belongs_to_some_cell() {
        let mut rng = Rng::new(4);
        let mut sim = make(320, 96, 4);
        let mut frame = Frame::new(320, 96);
        frame.fill(Rgb::new(255, 0, 255));
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        sim.render(&mut frame, &Theme::default());
        for (i, px) in frame.pixels.chunks_exact(4).enumerate() {
            assert!(!(px[0] == 255 && px[1] == 0 && px[2] == 255), "pixel {i} unpainted");
            assert_eq!(px[3], 255);
        }
    }

    #[test]
    fn diagram_has_visible_edges_between_cells() {
        // The boundaries are the diagram; if the edge test never fires,
        // what's on screen is just colored blobs.
        let mut rng = Rng::new(5);
        let mut sim = make(320, 96, 5);
        let mut frame = Frame::new(320, 96);
        let theme = Theme::default();
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        sim.render(&mut frame, &theme);

        let edge_pixels = frame
            .pixels
            .chunks_exact(4)
            .filter(|px| px[0] == theme.ink.r && px[1] == theme.ink.g && px[2] == theme.ink.b)
            .count();
        assert!(edge_pixels > 100, "only {edge_pixels} edge pixels -- no visible diagram");
    }

    #[test]
    fn cells_are_differently_colored() {
        let sim = make(320, 96, 6);
        let hues = sim.hues();
        let spread = hues.iter().fold(0.0f32, |acc, &h| acc.max((h - hues[0]).abs()));
        assert!(spread > 30.0, "all cells landed on nearly the same hue");
    }

    #[test]
    fn large_dt_does_not_explode_or_panic() {
        let mut rng = Rng::new(7);
        let mut sim = make(320, 96, 7);
        for _ in 0..10 {
            sim.step(1.0, &Input::default(), &mut rng);
        }
        for (x, y) in sim.positions() {
            assert!(x.is_finite() && (0.0..=320.0).contains(&x));
            assert!(y.is_finite() && (0.0..=96.0).contains(&y));
        }
    }

    #[test]
    fn resize_to_tiny_does_not_panic() {
        let mut rng = Rng::new(8);
        let mut sim = make(320, 96, 8);
        sim.resize(3, 3, &mut rng);
        for _ in 0..20 {
            sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        }
        let mut frame = Frame::new(3, 3);
        sim.render(&mut frame, &Theme::default());
    }

    #[test]
    fn resize_to_zero_does_not_panic() {
        let mut rng = Rng::new(9);
        let mut sim = make(320, 96, 9);
        sim.resize(0, 0, &mut rng);
        sim.step(1.0 / 60.0, &Input::default(), &mut rng);
        let mut frame = Frame::new(1, 1);
        sim.render(&mut frame, &Theme::default());
    }
}
