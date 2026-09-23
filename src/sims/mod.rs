//! The `Sim` trait and the concrete simulations.

pub mod bounce;
pub mod dungeon;
pub mod fractal;
pub mod life;
pub mod sand;
pub mod starry;
pub mod swarm;
pub mod voronoi;

use crate::paint::{Frame, Theme};
use crate::rng::Rng;

/// Pointer state, in frame-local device pixels, sampled once per tick.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub x: f32,
    pub y: f32,
    pub hover: bool,
    pub down: bool,
    /// Number of clicks since the previous tick (almost always 0 or 1).
    pub clicks: u32,
}

/// A self-contained, resizable, steppable, renderable simulation.
pub trait Sim {
    /// Called on construction and whenever the canvas' device-pixel size
    /// changes. Implementations must cope with any size, including 0 or
    /// tiny values like 1x1 or 4x4 — never panic.
    fn resize(&mut self, w: usize, h: usize, rng: &mut Rng);

    /// Advance the simulation by `dt` seconds. `dt` is *not* pre-clamped by
    /// the caller — a backgrounded tab can hand this a value of 1 second
    /// or more, and the sim must not explode (NaNs, escaped bounds,
    /// unbounded work) when that happens.
    fn step(&mut self, dt: f32, input: &Input, rng: &mut Rng);

    /// Paint the current state into `frame` using only `theme`'s three
    /// colors.
    fn render(&mut self, frame: &mut Frame, theme: &Theme);

    /// Suggested render rate. The host may still drive `tick` at a
    /// different (usually lower, for `prefers-reduced-motion`) rate.
    fn preferred_fps(&self) -> f32 {
        60.0
    }
}

/// The supported flavors of nonsense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    Swarm,
    Sand,
    Life,
    Fractal,
    Bounce,
    Dungeon,
    Starry,
    Voronoi,
}

impl Variant {
    /// Parse a variant name leniently: trims whitespace, ignores case, and
    /// accepts a couple of natural aliases. Anything unrecognized falls
    /// back to `Swarm` rather than erroring — a typo in a `variant`
    /// attribute must never render a broken button.
    pub fn parse(s: &str) -> Variant {
        match s.trim().to_ascii_lowercase().as_str() {
            "sand" | "falling-sand" | "fallingsand" => Variant::Sand,
            "life" | "conway" | "gol" | "game-of-life" => Variant::Life,
            "fractal" | "mandelbrot" => Variant::Fractal,
            "bounce" | "pixel-bounce" | "pixelbounce" => Variant::Bounce,
            "dungeon" | "dungeon-explorer" | "maze" | "doom" => Variant::Dungeon,
            "starry" | "starry-night" | "vangogh" | "van-gogh" => Variant::Starry,
            "voronoi" | "voronoi-diagram" | "cells" => Variant::Voronoi,
            "swarm" | "boids" | "" => Variant::Swarm,
            _ => Variant::Swarm,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Variant::Swarm => "swarm",
            Variant::Sand => "sand",
            Variant::Life => "life",
            Variant::Fractal => "fractal",
            Variant::Bounce => "bounce",
            Variant::Dungeon => "dungeon",
            Variant::Starry => "starry",
            Variant::Voronoi => "voronoi",
        }
    }

    /// Construct a freshly sized instance of this variant.
    pub fn make(self, w: usize, h: usize, rng: &mut Rng) -> Box<dyn Sim> {
        match self {
            Variant::Swarm => Box::new(swarm::Swarm::new(w, h, rng)),
            Variant::Sand => Box::new(sand::Sand::new(w, h, rng)),
            Variant::Life => Box::new(life::Life::new(w, h, rng)),
            Variant::Fractal => Box::new(fractal::Fractal::new(w, h, rng)),
            Variant::Bounce => Box::new(bounce::Bounce::new(w, h, rng)),
            Variant::Dungeon => Box::new(dungeon::Dungeon::new(w, h, rng)),
            Variant::Starry => Box::new(starry::Starry::new(w, h, rng)),
            Variant::Voronoi => Box::new(voronoi::Voronoi::new(w, h, rng)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_case_and_whitespace_tolerant() {
        assert_eq!(Variant::parse("swarm"), Variant::Swarm);
        assert_eq!(Variant::parse("  Swarm "), Variant::Swarm);
        assert_eq!(Variant::parse("SWARM"), Variant::Swarm);
        assert_eq!(Variant::parse("Sand"), Variant::Sand);
        assert_eq!(Variant::parse(" SAND\n"), Variant::Sand);
        assert_eq!(Variant::parse("Life"), Variant::Life);
        assert_eq!(Variant::parse("conway"), Variant::Life);
        assert_eq!(Variant::parse("Fractal"), Variant::Fractal);
        assert_eq!(Variant::parse("mandelbrot"), Variant::Fractal);
        assert_eq!(Variant::parse("Bounce"), Variant::Bounce);
        assert_eq!(Variant::parse("pixel-bounce"), Variant::Bounce);
        assert_eq!(Variant::parse("Dungeon"), Variant::Dungeon);
        assert_eq!(Variant::parse("maze"), Variant::Dungeon);
        assert_eq!(Variant::parse("doom"), Variant::Dungeon);
        assert_eq!(Variant::parse("Starry"), Variant::Starry);
        assert_eq!(Variant::parse("starry-night"), Variant::Starry);
        assert_eq!(Variant::parse("Voronoi"), Variant::Voronoi);
        assert_eq!(Variant::parse("cells"), Variant::Voronoi);
    }

    #[test]
    fn parse_falls_back_to_swarm_for_unknown() {
        assert_eq!(Variant::parse("totally-not-a-variant"), Variant::Swarm);
        assert_eq!(Variant::parse(""), Variant::Swarm);
        assert_eq!(Variant::parse("SWRAM"), Variant::Swarm);
        assert_eq!(Variant::parse("123"), Variant::Swarm);
    }
}
