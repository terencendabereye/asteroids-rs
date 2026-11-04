mod game;
mod graphics;
mod haptics;
mod input;
mod objects;
mod post;
mod ui;

use macroquad::prelude::*;
use std::time::{Duration, Instant};

fn window_conf() -> Conf {
    graphics::Graphics::load().conf()
}

/// Caps the frame rate (macroquad has no limiter of its own; VSync alone ties you to the display).
struct FrameLimiter {
    next: Instant,
}

impl FrameLimiter {
    fn wait(&mut self, fps: u32) {
        let now = Instant::now();
        if fps == 0 {
            self.next = now;
            return;
        }
        let step = Duration::from_secs_f64(1.0 / fps as f64);
        if now < self.next {
            // Sleep most of the wait, spin the last couple of ms for accuracy.
            let remaining = self.next - now;
            if remaining > Duration::from_millis(2) {
                std::thread::sleep(remaining - Duration::from_millis(2));
            }
            while Instant::now() < self.next {
                std::hint::spin_loop();
            }
        }
        self.next = (self.next + step).max(Instant::now()); // don't try to "catch up" after a stall
    }
}

/// Developer options: `--screenshot out.png [--scene title|play|ufo|pause|settings|controls|graphics] [--frames N]`
struct Shot {
    path: String,
    scene: String,
    frames: u32,
}

fn parse_args() -> Option<Shot> {
    let args: Vec<String> = std::env::args().collect();
    let get = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    Some(Shot {
        path: get("--screenshot")?,
        scene: get("--scene").unwrap_or_else(|| "title".into()),
        frames: get("--frames").and_then(|f| f.parse().ok()).unwrap_or(45),
    })
}

#[macroquad::main(window_conf)]
async fn main() {
    // Closing the window must go through our loop so the controller's motors/triggers are reset.
    prevent_quit();
    macroquad::rand::srand((get_time() * 1_000_000.0) as u64 ^ 0x9E37_79B9);
    let shot = parse_args();
    let mut game = game::Game::new(graphics::Graphics::load());
    if let Some(s) = &shot {
        game.debug_scene(&s.scene);
    }
    let mut limiter = FrameLimiter { next: Instant::now() };
    let mut frame = 0u32;
    while !is_quit_requested() && game.frame() {
        frame += 1;
        if let Some(s) = &shot {
            if frame >= s.frames {
                get_screen_data().export_png(&s.path);
                break;
            }
        }
        limiter.wait(game.max_fps());
        next_frame().await;
    }
    drop(game);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_holds_the_requested_frame_rate_and_unlimited_does_not_wait() {
        let mut l = FrameLimiter { next: Instant::now() };
        let start = Instant::now();
        for _ in 0..21 { l.wait(200); } // 200 fps -> 5 ms per frame
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        assert!((95.0..140.0).contains(&ms), "20 frames at 200fps took {ms} ms");

        let start = Instant::now();
        for _ in 0..1000 { l.wait(0); }
        assert!(start.elapsed() < Duration::from_millis(50));
    }
}
