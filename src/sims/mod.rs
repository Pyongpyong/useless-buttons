//! The `Sim` trait and the concrete simulations.

pub mod bounce;
pub mod dungeon;
pub mod fractal;
pub mod games;
pub mod hyperdrive;
pub mod life;
pub mod matrix;
pub mod sand;
pub mod starry;
pub mod swarm;
pub mod tunnel;
pub mod spectacle;
pub mod voronoi;

use crate::paint::{Frame, Theme};
use crate::rng::Rng;

/// Most presses a single tick remembers the position of. Anything past
/// this still counts in `Input::clicks`, just without a position.
pub const MAX_PRESSES: usize = 8;

/// Where a press landed, as fractions of the frame (`0..=1` on each
/// axis, top-left origin). An axis is NaN when the press has no position
/// on it (the element always sends both; the raw core's `click()` sends
/// neither).
#[derive(Clone, Copy, Debug)]
pub struct Press {
    pub x: f32,
    pub y: f32,
}

impl Default for Press {
    fn default() -> Self {
        Press { x: f32::NAN, y: f32::NAN }
    }
}

impl Press {
    /// Both coordinates known: a pointer press.
    pub fn is_pointed(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// Player input, sampled once per tick. Only games read it — the visual
/// variants are purely ambient.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    /// Presses since the previous tick (almost always 0 or 1).
    pub clicks: u32,
    /// Where the first `min(clicks, MAX_PRESSES)` presses landed.
    pub at: [Press; MAX_PRESSES],
    /// A pointer is being held down on the button right now. Unlike the
    /// presses this isn't an event: it stays set from tick to tick until
    /// the pointer lets go.
    pub held: bool,
}

impl Input {
    /// One press with no position.
    pub fn tap() -> Input {
        Input { clicks: 1, ..Input::default() }
    }

    /// One press at `(x, y)`, as fractions of the frame.
    pub fn press(x: f32, y: f32) -> Input {
        let mut input = Input::tap();
        input.at[0] = Press { x, y };
        input
    }

    /// Held down, with no new presses.
    pub fn hold() -> Input {
        Input { held: true, ..Input::default() }
    }

    /// The presses whose position was recorded.
    pub fn presses(&self) -> &[Press] {
        &self.at[..(self.clicks as usize).min(MAX_PRESSES)]
    }
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

    /// Games only: `true` once the player has beaten it. Must never go
    /// back to `false` — the host unlocks the button's label and clicks
    /// the moment this flips.
    fn cleared(&self) -> bool {
        false
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
    Hyperdrive,
    Tunnel,
    Matrix,
    Blackhole,
    Chrome,
    Plasma,
    StainedGlass,
    Aurora,
    Ripple,
    Hologram,
    Supernova,
    Flappy,
    Runner,
    Timing,
    Crossy,
    Django,
    Balloon,
    Memory,
    Dodge,
    Breakout,
    Invaders,
    Pong,
    Mole,
    Stack,
    Heli,
    Numbers,
    Rhythm,
    Survivor,
    Frog,
    Crawler,
    Shooter,
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
            "hyperdrive" | "lightspeed" | "warp" | "hyperspace" => Variant::Hyperdrive,
            "matrix" | "digital-rain" => Variant::Matrix,
            "tunnel" | "dimension-tunnel" => Variant::Tunnel,
            "blackhole" => Variant::Blackhole,
            "chrome" => Variant::Chrome,
            "plasma" => Variant::Plasma,
            "stained-glass" => Variant::StainedGlass,
            "aurora" => Variant::Aurora,
            "ripple" => Variant::Ripple,
            "hologram" => Variant::Hologram,
            "supernova" => Variant::Supernova,
            "flappy" | "flappy-bird" | "bird" => Variant::Flappy,
            "runner" | "jump" | "platformer" | "wonderboy" | "wonder-boy" => Variant::Runner,
            "timing" | "timing-ring" => Variant::Timing,
            "crossy" | "crossy-road" | "chicken" => Variant::Crossy,
            "django" | "western" | "quickdraw" | "shooting" => Variant::Django,
            "balloon" | "balloons" | "pop" => Variant::Balloon,
            "memory" | "match" | "cards" | "concentration" => Variant::Memory,
            "dodge" | "traffic" | "highway" => Variant::Dodge,
            "breakout" | "arkanoid" | "bricks" => Variant::Breakout,
            "invaders" | "space-invaders" | "space" => Variant::Invaders,
            "pong" | "tennis" => Variant::Pong,
            "mole" | "whack-a-mole" | "whack" => Variant::Mole,
            "stack" | "tower" | "stacker" => Variant::Stack,
            "heli" | "helicopter" | "cave" => Variant::Heli,
            "numbers" | "number-order" | "sequence" => Variant::Numbers,
            "rhythm" | "rhythm-hero" | "osu" | "beat" => Variant::Rhythm,
            "survivor" | "survivors" | "vampire" => Variant::Survivor,
            "frog" | "frogger" | "lilypad" => Variant::Frog,
            "crawler" | "dungeon-crawl" | "monster-hunt" => Variant::Crawler,
            "shooter" | "shmup" | "gradius" => Variant::Shooter,
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
            Variant::Hyperdrive => "hyperdrive",
            Variant::Tunnel => "tunnel",
            Variant::Matrix => "matrix",
            Variant::Blackhole => "blackhole",
            Variant::Chrome => "chrome",
            Variant::Plasma => "plasma",
            Variant::StainedGlass => "stained-glass",
            Variant::Aurora => "aurora",
            Variant::Ripple => "ripple",
            Variant::Hologram => "hologram",
            Variant::Supernova => "supernova",
            Variant::Flappy => "flappy",
            Variant::Runner => "runner",
            Variant::Timing => "timing",
            Variant::Crossy => "crossy",
            Variant::Django => "django",
            Variant::Balloon => "balloon",
            Variant::Memory => "memory",
            Variant::Dodge => "dodge",
            Variant::Breakout => "breakout",
            Variant::Invaders => "invaders",
            Variant::Pong => "pong",
            Variant::Mole => "mole",
            Variant::Stack => "stack",
            Variant::Heli => "heli",
            Variant::Numbers => "numbers",
            Variant::Rhythm => "rhythm",
            Variant::Survivor => "survivor",
            Variant::Frog => "frog",
            Variant::Crawler => "crawler",
            Variant::Shooter => "shooter",
        }
    }

    /// Games lock the button's label and clicks until they're cleared;
    /// everything else is a purely visual background.
    pub fn is_game(self) -> bool {
        matches!(
            self,
            Variant::Flappy
                | Variant::Runner
                | Variant::Timing
                | Variant::Crossy
                | Variant::Django
                | Variant::Balloon
                | Variant::Memory
                | Variant::Dodge
                | Variant::Breakout
                | Variant::Invaders
                | Variant::Pong
                | Variant::Mole
                | Variant::Stack
                | Variant::Heli
                | Variant::Numbers
                | Variant::Rhythm
                | Variant::Survivor
                | Variant::Frog
                | Variant::Crawler
                | Variant::Shooter
        )
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
            Variant::Hyperdrive => Box::new(hyperdrive::Hyperdrive::new(w, h, rng)),
            Variant::Tunnel => Box::new(tunnel::Tunnel::new(w, h, rng)),
            Variant::Matrix => Box::new(matrix::Matrix::new(w, h, rng)),
            Variant::Blackhole => Box::new(spectacle::Spectacle::new(spectacle::Effect::Blackhole, w, h, rng)),
            Variant::Chrome => Box::new(spectacle::Spectacle::new(spectacle::Effect::Chrome, w, h, rng)),
            Variant::Plasma => Box::new(spectacle::Spectacle::new(spectacle::Effect::Plasma, w, h, rng)),
            Variant::StainedGlass => Box::new(spectacle::Spectacle::new(spectacle::Effect::Glass, w, h, rng)),
            Variant::Aurora => Box::new(spectacle::Spectacle::new(spectacle::Effect::Aurora, w, h, rng)),
            Variant::Ripple => Box::new(spectacle::Spectacle::new(spectacle::Effect::Ripple, w, h, rng)),
            Variant::Hologram => Box::new(spectacle::Spectacle::new(spectacle::Effect::Hologram, w, h, rng)),
            Variant::Supernova => Box::new(spectacle::Spectacle::new(spectacle::Effect::Supernova, w, h, rng)),
            Variant::Flappy => Box::new(games::flappy::Flappy::new(w, h, rng)),
            Variant::Runner => Box::new(games::runner::Runner::new(w, h, rng)),
            Variant::Timing => Box::new(games::timing::Timing::new(w, h, rng)),
            Variant::Crossy => Box::new(games::crossy::Crossy::new(w, h, rng)),
            Variant::Django => Box::new(games::django::Django::new(w, h, rng)),
            Variant::Balloon => Box::new(games::balloon::BalloonPop::new(w, h, rng)),
            Variant::Memory => Box::new(games::memory::Memory::new(w, h, rng)),
            Variant::Dodge => Box::new(games::dodge::Dodge::new(w, h, rng)),
            Variant::Breakout => Box::new(games::breakout::Breakout::new(w, h, rng)),
            Variant::Invaders => Box::new(games::invaders::Invaders::new(w, h, rng)),
            Variant::Pong => Box::new(games::pong::Pong::new(w, h, rng)),
            Variant::Mole => Box::new(games::mole::MoleWhack::new(w, h, rng)),
            Variant::Stack => Box::new(games::stack::Stack::new(w, h, rng)),
            Variant::Heli => Box::new(games::heli::Heli::new(w, h, rng)),
            Variant::Numbers => Box::new(games::numbers::Numbers::new(w, h, rng)),
            Variant::Rhythm => Box::new(games::rhythm::Rhythm::new(w, h, rng)),
            Variant::Survivor => Box::new(games::survivor::Survivor::new(w, h, rng)),
            Variant::Frog => Box::new(games::frog::Frog::new(w, h, rng)),
            Variant::Crawler => Box::new(games::crawler::Crawler::new(w, h, rng)),
            Variant::Shooter => Box::new(games::shooter::Shooter::new(w, h, rng)),
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
        assert_eq!(Variant::parse("Hyperdrive"), Variant::Hyperdrive);
        assert_eq!(Variant::parse("lightspeed"), Variant::Hyperdrive);
        assert_eq!(Variant::parse("warp"), Variant::Hyperdrive);
        assert_eq!(Variant::parse(" Tunnel "), Variant::Tunnel);
        assert_eq!(Variant::parse("dimension-tunnel"), Variant::Tunnel);
        assert_eq!(Variant::parse(" MATRIX "), Variant::Matrix);
        assert_eq!(Variant::parse("digital-rain"), Variant::Matrix);
        assert_eq!(Variant::parse(" BLACKHOLE "), Variant::Blackhole);
        assert_eq!(Variant::Blackhole.as_str(), "blackhole");
        assert_eq!(Variant::parse(" CHROME "), Variant::Chrome);
        assert_eq!(Variant::Chrome.as_str(), "chrome");
        assert_eq!(Variant::parse(" PLASMA "), Variant::Plasma);
        assert_eq!(Variant::Plasma.as_str(), "plasma");
        assert_eq!(Variant::parse(" STAINED-GLASS "), Variant::StainedGlass);
        assert_eq!(Variant::StainedGlass.as_str(), "stained-glass");
        assert_eq!(Variant::parse(" AURORA "), Variant::Aurora);
        assert_eq!(Variant::Aurora.as_str(), "aurora");
        assert_eq!(Variant::parse(" RIPPLE "), Variant::Ripple);
        assert_eq!(Variant::Ripple.as_str(), "ripple");
        assert_eq!(Variant::parse(" HOLOGRAM "), Variant::Hologram);
        assert_eq!(Variant::Hologram.as_str(), "hologram");
        assert_eq!(Variant::parse(" SUPERNOVA "), Variant::Supernova);
        assert_eq!(Variant::Supernova.as_str(), "supernova");
        assert_eq!(Variant::parse(" Flappy "), Variant::Flappy);
        assert_eq!(Variant::parse("wonderboy"), Variant::Runner);
        assert_eq!(Variant::parse("TIMING"), Variant::Timing);
        assert_eq!(Variant::parse("crossy-road"), Variant::Crossy);
        assert_eq!(Variant::parse(" Django"), Variant::Django);
        assert_eq!(Variant::parse("balloons"), Variant::Balloon);
        assert_eq!(Variant::parse("MEMORY"), Variant::Memory);
        assert_eq!(Variant::parse("highway"), Variant::Dodge);
        assert_eq!(Variant::parse("Arkanoid"), Variant::Breakout);
        assert_eq!(Variant::parse("space-invaders"), Variant::Invaders);
        assert_eq!(Variant::parse(" PONG "), Variant::Pong);
        assert_eq!(Variant::parse("whack-a-mole"), Variant::Mole);
        assert_eq!(Variant::parse("Tower"), Variant::Stack);
        assert_eq!(Variant::parse("helicopter"), Variant::Heli);
        assert_eq!(Variant::parse("number-order"), Variant::Numbers);
        assert_eq!(Variant::parse("rhythm-hero"), Variant::Rhythm);
        assert_eq!(Variant::parse("vampire"), Variant::Survivor);
        assert_eq!(Variant::parse("Frogger"), Variant::Frog);
        assert_eq!(Variant::parse("dungeon-crawl"), Variant::Crawler);
        assert_eq!(Variant::parse("shmup"), Variant::Shooter);
        assert_eq!(Variant::parse("doom"), Variant::Dungeon, "the visual dungeon keeps its alias");
        assert!(Variant::Timing.is_game() && !Variant::Swarm.is_game());
    }

    #[test]
    fn parse_falls_back_to_swarm_for_unknown() {
        assert_eq!(Variant::parse("totally-not-a-variant"), Variant::Swarm);
        assert_eq!(Variant::parse(""), Variant::Swarm);
        assert_eq!(Variant::parse("SWRAM"), Variant::Swarm);
        assert_eq!(Variant::parse("123"), Variant::Swarm);
    }
}
