//! Graphics settings, the letterboxed `View` that maps the fixed 1000x750 game space onto any
//! window size (including fullscreen), and window-config helpers.
use crate::objects::{SCREEN_H, SCREEN_W};
use macroquad::miniquad::conf::Platform;
use macroquad::prelude::*;

const FILE: &str = "graphics.cfg";

/// Window size presets (multiples of the 1000x750 game space).
pub const WINDOW_SCALES: [f32; 4] = [0.8, 1.0, 1.25, 1.5];

#[derive(Clone, Copy)]
pub struct Graphics {
    pub fullscreen: bool,
    pub window_scale: usize, // index into WINDOW_SCALES
    pub vsync: bool,         // applied at startup
    pub antialiasing: bool,  // applied at startup (4x MSAA)
    pub show_fps: bool,
}

impl Default for Graphics {
    fn default() -> Self {
        Self { fullscreen: false, window_scale: 1, vsync: true, antialiasing: true, show_fps: false }
    }
}

impl Graphics {
    pub fn load() -> Self {
        let mut g = Self::default();
        let Ok(text) = std::fs::read_to_string(FILE) else { return g };
        let v: Vec<i32> = text.split_whitespace().filter_map(|s| s.parse().ok()).collect();
        if v.len() == 5 {
            g.fullscreen = v[0] != 0;
            g.window_scale = (v[1].max(0) as usize).min(WINDOW_SCALES.len() - 1);
            g.vsync = v[2] != 0;
            g.antialiasing = v[3] != 0;
            g.show_fps = v[4] != 0;
        }
        g
    }

    pub fn save(&self) {
        let _ = std::fs::write(
            FILE,
            format!("{} {} {} {} {}\n", self.fullscreen as i32, self.window_scale, self.vsync as i32, self.antialiasing as i32, self.show_fps as i32),
        );
    }

    pub fn window_size(&self) -> (f32, f32) {
        let s = WINDOW_SCALES[self.window_scale];
        ((SCREEN_W * s).round(), (SCREEN_H * s).round())
    }

    pub fn window_size_label(&self) -> String {
        let (w, h) = self.window_size();
        format!("{}x{}", w as i32, h as i32)
    }

    /// Startup window configuration.
    pub fn conf(&self) -> Conf {
        let (w, h) = self.window_size();
        Conf {
            window_title: "Asteroids".to_owned(),
            window_width: w as i32,
            window_height: h as i32,
            window_resizable: true,
            fullscreen: self.fullscreen,
            sample_count: if self.antialiasing { 4 } else { 1 },
            platform: Platform { swap_interval: Some(if self.vsync { 1 } else { 0 }), ..Default::default() },
            ..Default::default()
        }
    }

    /// Apply the runtime-changeable settings (fullscreen, window size) to the live window.
    pub fn apply_window(&self) {
        set_fullscreen(self.fullscreen);
        if !self.fullscreen {
            let (w, h) = self.window_size();
            request_new_screen_size(w, h);
        }
    }
}

/// Maps the fixed game space onto the window, centred and letterboxed.
#[derive(Clone, Copy)]
pub struct View {
    pub scale: f32,
    pub offset: Vec2,
}

impl View {
    pub fn fit(screen_w: f32, screen_h: f32) -> Self {
        let scale = (screen_w / SCREEN_W).min(screen_h / SCREEN_H).max(0.01);
        Self { scale, offset: Vec2::new((screen_w - SCREEN_W * scale) / 2.0, (screen_h - SCREEN_H * scale) / 2.0) }
    }

    /// Window pixel position -> game-space position.
    pub fn to_world(&self, p: Vec2) -> Vec2 { (p - self.offset) / self.scale }

    pub fn camera(&self) -> Camera2D {
        let mut cam = Camera2D::from_display_rect(Rect::new(0.0, 0.0, SCREEN_W, SCREEN_H));
        cam.viewport = Some((
            self.offset.x.round() as i32,
            self.offset.y.round() as i32,
            (SCREEN_W * self.scale).round() as i32,
            (SCREEN_H * self.scale).round() as i32,
        ));
        cam
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_letterboxes_and_maps_mouse_positions() {
        // A wide window: the 4:3 game is centred with bars left and right.
        let v = View::fit(2000.0, 750.0);
        assert!((v.scale - 1.0).abs() < 1e-5);
        assert!((v.offset.x - 500.0).abs() < 1e-3 && v.offset.y.abs() < 1e-3);
        let w = v.to_world(Vec2::new(500.0, 0.0));
        assert!(w.x.abs() < 1e-3 && w.y.abs() < 1e-3);
        let c = v.to_world(Vec2::new(1000.0, 375.0));
        assert!((c.x - 500.0).abs() < 1e-3 && (c.y - 375.0).abs() < 1e-3);
    }
}
