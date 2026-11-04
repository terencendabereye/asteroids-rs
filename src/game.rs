use crate::graphics::{Graphics, View};
use crate::graphics::{LEVEL_NAMES};
use crate::haptics::{Haptics, TriggerMode};
use crate::post::PostFx;
use crate::input::{key_name, button_name, AimMode, Input, MenuNav, ACTIONS};
use crate::objects::*;
use crate::ui::*;
use macroquad::prelude::*;
use macroquad::rand::gen_range;

#[derive(Copy, Clone, PartialEq, Eq)]
enum State { Title, Playing, Paused, GameOver, Settings }

#[derive(Copy, Clone, PartialEq, Eq)]
enum Page { Hub, Controls, Graphics }

#[derive(Copy, Clone, PartialEq, Eq)]
enum Mode { Classic, Ufo }

// Difficulty curve: asteroid count (hence density) and speed both grow with level.
fn asteroid_count(level: u32) -> u32 { (3 + level).min(14) }
fn asteroid_speed_scale(level: u32) -> f32 { (1.0 + 0.08 * (level as f32 - 1.0)).min(1.8) }

pub struct Game {
    // world
    ship: Option<Spaceship>,
    asteroids: Vec<Asteroid>,
    lasers: Vec<Laser>,
    ufos: Vec<Ufo>,
    level: u32,
    banner_timer: f32,
    death_timer: f32,
    ufo_timer: f32,
    mode: Mode,
    // app
    input: Input,
    nav: MenuNav,
    state: State,
    settings_return: State,
    quit: bool,
    capturing: bool,
    title: Menu,
    pause: Menu,
    over: Menu,
    settings: Menu,
    haptics: Haptics,
    cursor_hidden: bool,
    gfx: Graphics,
    post: PostFx,
    view: View,
    page: Page,
}

impl Game {
    pub fn new(gfx: Graphics) -> Self {
        let settings = Menu::new("SETTINGS", 70.0, &[]);
        let mut g = Self {
            ship: None, asteroids: vec![], lasers: vec![], ufos: vec![],
            level: 0, banner_timer: 0.0, death_timer: 0.0, ufo_timer: 0.0, mode: Mode::Classic,
            input: Input::new(), nav: MenuNav::default(),
            state: State::Title, settings_return: State::Title, quit: false, capturing: false,
            title: Menu::new("ASTEROIDS", 160.0, &["Classic", "UFO Mode", "Settings", "Quit"]),
            pause: Menu::new("PAUSED", 200.0, &["Resume", "Settings", "Quit to Title"]),
            over: Menu::new("GAME OVER", 200.0, &["Retry", "Settings", "Quit to Title"]),
            settings,
            haptics: Haptics::new(),
            cursor_hidden: false,
            gfx,
            post: PostFx::new(),
            view: View::fit(SCREEN_W, SCREEN_H),
            page: Page::Hub,
        };
        init_text();
        g.show_title();
        g
    }

    /// One frame. Returns false when the game wants to quit.
    /// Print what the OS and gilrs/hidapi see, then listen for gamepad events (`--diagnose`).
    pub fn diagnose(&mut self) {
        println!("gilrs gamepads:");
        self.input.poll(&self.view);
        for l in self.input.diagnose() { println!("{l}"); }
        println!("HID devices (Sony / gamepad-like):");
        for l in crate::haptics::diagnose() { println!("{l}"); }
        println!("Listening for gamepad events for 10 s - press buttons now...");
        self.input.log_events(10.0);
        println!("done.");
    }

    pub fn max_fps(&self) -> u32 { self.gfx.max_fps_value() }

    /// Jump straight to a screen (used by the `--scene` screenshot option).
    pub fn debug_scene(&mut self, scene: &str) {
        match scene {
            "play" => self.start_game(Mode::Classic),
            "ufo" => self.start_game(Mode::Ufo),
            "pause" => { self.start_game(Mode::Classic); self.state = State::Paused; }
            "settings" | "controls" | "graphics" => {
                self.open_settings(State::Title);
                match scene { "controls" => self.set_page(Page::Controls), "graphics" => self.set_page(Page::Graphics), _ => {} }
            }
            _ => {}
        }
    }

    pub fn frame(&mut self) -> bool {
        let dt = get_frame_time().min(0.05);
        self.view = View::fit(screen_width(), screen_height());
        if is_key_pressed(KeyCode::F11) {
            self.gfx.fullscreen = !self.gfx.fullscreen;
            self.gfx.save();
            self.gfx.apply_window();
        }
        self.input.poll(&self.view);
        self.nav.poll(&self.input);
        self.update(dt);
        self.update_feedback(dt);
        self.draw();
        !self.quit
    }

    /// Rumble, adaptive triggers and cursor visibility follow the game state.
    fn update_feedback(&mut self, dt: f32) {
        let playing = self.state == State::Playing;
        self.haptics.set_vibration_enabled(self.input.vibration);
        self.haptics.set_triggers_enabled(self.input.adaptive_triggers);
        self.haptics.set_trigger(if playing { TriggerMode::Resist } else { TriggerMode::Off });
        self.haptics.update(dt);

        let hide = playing && self.input.aim == AimMode::Mouse;
        if hide != self.cursor_hidden {
            show_mouse(!hide);
            self.cursor_hidden = hide;
        }
    }

    // ── state changes ──────────────────────────────────────────────
    fn clear_world(&mut self) {
        self.ship = None;
        self.asteroids.clear();
        self.lasers.clear();
        self.ufos.clear();
    }

    fn show_title(&mut self) {
        self.clear_world();
        for _ in 0..7 {
            let p = Vec2::new(gen_range(0.0, SCREEN_W), gen_range(0.0, SCREEN_H));
            self.asteroids.push(Asteroid::new(p, 3, 1.0));
        }
        self.state = State::Title;
        self.title.selected = 0;
    }

    fn start_game(&mut self, mode: Mode) {
        self.clear_world();
        self.mode = mode;
        self.ufo_timer = 12.0;
        self.ship = Some(Spaceship::new());
        self.level = 0;
        self.next_level();
        self.state = State::Playing;
    }

    fn open_settings(&mut self, from: State) {
        self.settings_return = from;
        self.settings.selected = 0;
        self.capturing = false;
        self.page = Page::Hub;
        self.state = State::Settings;
    }

    fn next_level(&mut self) {
        self.level += 1;
        self.banner_timer = 2.0;
        let speed = asteroid_speed_scale(self.level);
        let center = self.ship.as_ref().map(|s| s.position()).unwrap_or(Vec2::new(SCREEN_W / 2.0, SCREEN_H / 2.0));
        for _ in 0..asteroid_count(self.level) {
            // Spawn away from the player.
            let p = loop {
                let p = Vec2::new(gen_range(0.0, SCREEN_W), gen_range(0.0, SCREEN_H));
                if p.distance(center) >= 200.0 { break p; }
            };
            self.asteroids.push(Asteroid::new(p, 3, speed));
        }
    }

    // ── per-frame update: exactly one state handler runs per frame ──
    fn update(&mut self, dt: f32) {
        match self.state {
            State::Title => self.update_title(dt),
            State::Playing => self.update_playing(dt),
            State::Paused => self.update_paused(dt),
            State::GameOver => self.update_game_over(dt),
            State::Settings => self.update_settings(dt),
        }
    }

    fn update_title(&mut self, dt: f32) {
        for a in &mut self.asteroids { a.update(dt); }
        self.title.update(dt);
        match self.title.handle(&self.nav) {
            Some(0) => self.start_game(Mode::Classic),
            Some(1) => self.start_game(Mode::Ufo),
            Some(2) => self.open_settings(State::Title),
            Some(3) => self.quit = true,
            _ => {}
        }
    }

    fn update_playing(&mut self, dt: f32) {
        if self.input.pause_pressed() {
            self.state = State::Paused;
            self.pause.selected = 0;
            self.haptics.stop_rumble();
            return;
        }

        let Some(ship) = self.ship.as_mut() else { return };
        ship.handle_input(&self.input, dt);
        if ship.consume_fired() { self.haptics.rumble(0.0, 0.25, 0.05); }
        self.lasers.extend(ship.take_shots());
        ship.update(dt);
        let ship_pos = ship.position();

        for a in &mut self.asteroids { a.update(dt); }
        for l in &mut self.lasers { l.update(dt); }
        for u in &mut self.ufos {
            u.target = ship_pos;
            u.update(dt);
            self.lasers.extend(u.take_shots());
        }

        self.collide();
        self.asteroids.retain(|a| a.alive());
        self.lasers.retain(|l| l.alive());
        self.ufos.retain(|u| u.alive());

        if self.banner_timer > 0.0 { self.banner_timer -= dt; }
        if self.mode == Mode::Ufo { self.spawn_ufo_if_due(dt); }
        if self.asteroids.is_empty() && self.state == State::Playing { self.next_level(); }
    }

    fn update_paused(&mut self, dt: f32) {
        self.pause.update(dt);
        if self.input.pause_pressed() { self.state = State::Playing; return; }
        match self.pause.handle(&self.nav) {
            Some(0) => self.state = State::Playing,
            Some(1) => self.open_settings(State::Paused),
            Some(2) => self.show_title(),
            _ => {}
        }
    }

    fn update_game_over(&mut self, dt: f32) {
        self.over.update(dt);
        self.death_timer += dt;
        if self.death_timer < 0.8 { return; } // ignore input briefly so fire-spamming can't hit Retry
        match self.over.handle(&self.nav) {
            Some(0) => self.start_game(self.mode),
            Some(1) => self.open_settings(State::GameOver),
            Some(2) => self.show_title(),
            _ => {}
        }
    }

    fn set_page(&mut self, page: Page) {
        self.page = page;
        self.settings.selected = 0;
    }

    fn settings_back(&mut self) {
        match self.page {
            Page::Hub => self.state = self.settings_return,
            _ => self.set_page(Page::Hub),
        }
    }

    /// Rebuild the settings rows for the current page (values change as options are toggled).
    fn build_settings_rows(&mut self) {
        let on_off = |b: bool| if b { "On" } else { "Off" };
        let (spacing, first_row, title, hint) = match self.page {
            Page::Hub => (44.0, 100.0, "SETTINGS", "Click, or ENTER / A to open    ESC / B / right-click: back"),
            Page::Controls => (34.0, 90.0, "CONTROLS",
                "ENTER / A / click: change    ESC / B: back    Move keys: stick / mouse aim"),
            Page::Graphics => (40.0, 90.0, "GRAPHICS", "ENTER / A / click: change    F11: fullscreen    ESC / B: back"),
        };
        self.settings.status = if self.page == Page::Controls {
            vec![format!("Gamepad: {}", self.input.pad_status()), format!("DualSense HID: {}", self.haptics.status())]
        } else {
            Vec::new()
        };
        self.settings.title = title.into();
        self.settings.hint = hint.into();
        self.settings.spacing = spacing;
        self.settings.first_row = first_row;
        let rows = &mut self.settings.rows;
        rows.clear();
        match self.page {
            Page::Hub => {
                rows.push(Row::label("Controls"));
                rows.push(Row::label("Graphics"));
                rows.push(Row::label("Back"));
            }
            Page::Controls => {
                for (i, a) in ACTIONS.iter().enumerate() {
                    let b = self.input.binding(*a);
                    let (mut key, mut btn) = (key_name(b.key), button_name(b.button));
                    if self.capturing && self.settings.selected == i { key = "press key / button...".into(); btn = key.clone(); }
                    rows.push(Row::table(a.name(), &key, &btn));
                }
                rows.push(Row::table("Aim with", self.input.aim.name(), " "));
                rows.push(Row::table("Vibration", on_off(self.input.vibration),
                    if self.haptics.connected() { "DualSense found" } else { "no DualSense found" }));
                rows.push(Row::table("Adaptive triggers (R2)", on_off(self.input.adaptive_triggers), "DualSense only"));
                rows.push(Row::label("Reset controls to defaults"));
                rows.push(Row::label("Back"));
            }
            Page::Graphics => {
                let g = self.gfx;
                rows.push(Row::table("Fullscreen", on_off(g.fullscreen), "F11"));
                rows.push(Row::table("Window size", &g.window_size_label(), if g.fullscreen { "(windowed only)" } else { " " }));
                rows.push(Row::table("Max FPS", &g.max_fps_label(),
                    if g.vsync && g.max_fps_value() != 60 { "VSync caps to display rate" } else { " " }));
                rows.push(Row::table("VSync", on_off(g.vsync), "applies on restart"));
                rows.push(Row::table("Anti-aliasing", on_off(g.antialiasing), "2x supersampling"));
                rows.push(Row::table("Bloom / glow", LEVEL_NAMES[g.bloom], " "));
                rows.push(Row::table("CRT warp", LEVEL_NAMES[g.curvature], " "));
                rows.push(Row::table("Scanlines", LEVEL_NAMES[g.scanlines], " "));
                rows.push(Row::table("Show FPS", on_off(g.show_fps), " "));
                rows.push(Row::label("Back"));
            }
        }
    }

    fn update_settings(&mut self, dt: f32) {
        self.settings.update(dt);
        self.build_settings_rows();

        if self.capturing {
            let a = ACTIONS[self.settings.selected];
            if is_key_pressed(KeyCode::Escape) { self.capturing = false; return; }
            if let Some(k) = self.input.pressed_key() {
                self.input.set_key(a, k); self.input.save(); self.capturing = false;
            } else if let Some(b) = self.input.pressed_button() {
                self.input.set_button(a, b); self.input.save(); self.capturing = false;
            }
            return;
        }

        if self.nav.back { self.settings_back(); return; }
        let Some(act) = self.settings.handle(&self.nav) else { return };
        let n = ACTIONS.len();
        match self.page {
            Page::Hub => match act {
                0 => self.set_page(Page::Controls),
                1 => self.set_page(Page::Graphics),
                _ => self.settings_back(),
            },
            Page::Controls => {
                if act < n {
                    self.capturing = true;
                    clear_input_queue(); // so the Enter that opened capture isn't bound
                } else if act == n {
                    self.input.aim = self.input.aim.next(); self.input.save();
                } else if act == n + 1 {
                    self.input.vibration = !self.input.vibration; self.input.save();
                    if self.input.vibration { self.haptics.rumble(0.5, 0.5, 0.15); } // sample buzz
                } else if act == n + 2 {
                    self.input.adaptive_triggers = !self.input.adaptive_triggers; self.input.save();
                } else if act == n + 3 {
                    self.input.reset_defaults(); self.input.save();
                } else {
                    self.settings_back();
                }
            }
            Page::Graphics => {
                match act {
                    0 => { self.gfx.fullscreen = !self.gfx.fullscreen; self.gfx.apply_window(); }
                    1 => {
                        self.gfx.window_scale = (self.gfx.window_scale + 1) % crate::graphics::WINDOW_SCALES.len();
                        self.gfx.apply_window();
                    }
                    2 => self.gfx.max_fps = (self.gfx.max_fps + 1) % crate::graphics::MAX_FPS.len(),
                    3 => self.gfx.vsync = !self.gfx.vsync,
                    4 => self.gfx.antialiasing = !self.gfx.antialiasing,
                    5 => self.gfx.bloom = (self.gfx.bloom + 1) % 4,
                    6 => self.gfx.curvature = (self.gfx.curvature + 1) % 4,
                    7 => self.gfx.scanlines = (self.gfx.scanlines + 1) % 4,
                    8 => self.gfx.show_fps = !self.gfx.show_fps,
                    _ => { self.settings_back(); return; }
                }
                self.gfx.save();
            }
        }
    }

    // ── UFO mode ───────────────────────────────────────────────────
    fn spawn_ufo_if_due(&mut self, dt: f32) {
        if !self.ufos.is_empty() { return; }
        self.ufo_timer -= dt;
        if self.ufo_timer > 0.0 { return; }
        self.ufo_timer = (14.0 - self.level as f32).max(5.0);                  // UFOs come more often later
        let aim_error = (0.5 - 0.05 * self.level as f32).max(0.08);            // and aim better
        self.ufos.push(Ufo::new(aim_error));
    }

    // ── collisions ─────────────────────────────────────────────────
    fn collide(&mut self) {
        let Some(ship) = self.ship.as_mut() else { return };
        let ship_body = ship.body_ref().clone();
        let vulnerable = !ship.invulnerable();
        let mut hit_player = false;
        let mut score = 0;
        let mut spawned: Vec<Asteroid> = Vec::new();

        let split = |a: &Asteroid, out: &mut Vec<Asteroid>| {
            if a.size > 1 {
                for _ in 0..2 { out.push(Asteroid::new(a.position(), a.size - 1, a.speed_scale())); }
            }
        };

        // player lasers vs asteroids / UFOs; enemy lasers vs the player
        for l in self.lasers.iter_mut().filter(|l| l.alive()) {
            if l.hostile {
                if vulnerable && l.body().collides_with(&ship_body) { l.kill(); hit_player = true; }
                continue;
            }
            if let Some(a) = self.asteroids.iter_mut().find(|a| a.alive() && l.collides_with(*a)) {
                l.kill();
                a.kill();
                score += a.points();
                self.haptics.rumble(0.1 + 0.1 * a.size as f32, 0.2, 0.12);
                split(a, &mut spawned);
            } else if let Some(u) = self.ufos.iter_mut().find(|u| u.alive() && l.collides_with(*u)) {
                l.kill();
                u.kill();
                score += u.points();
                self.haptics.rumble(0.6, 0.6, 0.25);
            }
        }

        // asteroids / UFOs ramming the player
        if vulnerable {
            for a in self.asteroids.iter_mut().filter(|a| a.alive()) {
                if a.body().collides_with(&ship_body) {
                    a.kill();
                    self.haptics.rumble(0.1 + 0.1 * a.size as f32, 0.2, 0.12);
                    split(a, &mut spawned);
                    hit_player = true;
                }
            }
            for u in self.ufos.iter_mut().filter(|u| u.alive()) {
                if u.body().collides_with(&ship_body) { u.kill(); hit_player = true; }
            }
        }

        ship.add_score(score);
        self.asteroids.extend(spawned);
        if hit_player {
            self.haptics.rumble(1.0, 1.0, 0.5);
            if ship.die() {
                self.state = State::GameOver;
                self.death_timer = 0.0;
                self.over.selected = 0;
            }
        }
    }

    // ── drawing ────────────────────────────────────────────────────
    fn draw(&mut self) {
        clear_background(BLACK); // whole window, including the letterbox bars
        let rs = self.post.begin(&self.view, &self.gfx); // everything below goes to the offscreen scene
        set_text_scale(rs);
        for a in &self.asteroids { a.draw(); }
        for l in &self.lasers { l.draw(); }
        for u in &self.ufos { u.draw(); }
        if let Some(s) = &self.ship { s.draw(); }

        if self.ship.is_some() && self.state != State::Title { self.draw_hud(); }
        if self.state == State::Playing && self.input.aim == AimMode::Mouse {
            let (mx, my) = (self.input.mouse.x, self.input.mouse.y);
            draw_circle_lines(mx, my, 10.0, 1.5, RED);
            draw_line(mx - 16.0, my, mx + 16.0, my, 1.5, RED);
            draw_line(mx, my - 16.0, mx, my + 16.0, 1.5, RED);
        }

        match self.state {
            State::Title => {
                self.title.draw();
                draw_centered("ARROWS / D-PAD + SPACE / A     ESC / START to pause", SCREEN_H - 50.0, 18.0, GRAY);
            }
            State::Playing => {}
            State::Paused => { dim(); self.pause.draw(); }
            State::GameOver => {
                dim();
                self.over.draw();
                if let Some(s) = &self.ship {
                    draw_centered(&format!("Score {}   -   Level {}", s.score(), self.level), 150.0, 28.0, RED);
                }
            }
            State::Settings => { dim(); self.settings.draw(); }
        }
        if self.gfx.show_fps {
            draw_text_top(&format!("{} FPS", get_fps()), SCREEN_W - 100.0, SCREEN_H - 28.0, 18.0, GRAY);
        }
        self.post.present(&self.view, &self.gfx);
    }

    fn draw_hud(&self) {
        let Some(ship) = &self.ship else { return };
        draw_text_top(&format!("SCORE {:06}", ship.score()), 20.0, 15.0, 26.0, WHITE);
        draw_centered(&format!("LEVEL {}", self.level), 15.0, 26.0, WHITE);
        for i in 0..ship.lives().max(0) {
            let (x, y) = (30.0 + i as f32 * 28.0, 62.0);
            draw_line(x, y - 12.0, x - 8.0, y + 8.0, 2.0, BLUE);
            draw_line(x, y - 12.0, x + 8.0, y + 8.0, 2.0, BLUE);
            draw_line(x - 8.0, y + 8.0, x + 8.0, y + 8.0, 2.0, BLUE);
        }
        if self.mode == Mode::Ufo { draw_text_top("UFO MODE", SCREEN_W - 140.0, 20.0, 18.0, GREEN); }
        if self.state == State::Playing && self.banner_timer > 0.0 {
            let a = self.banner_timer.min(1.0);
            let t = format!("LEVEL {}", self.level);
            draw_text_top(&t, (SCREEN_W - text_width(&t, 64.0)) / 2.0, SCREEN_H / 2.0 - 150.0, 64.0, Color::new(SKYBLUE.r, SKYBLUE.g, SKYBLUE.b, a));
        }
    }
}

fn dim() {
    draw_rectangle(0.0, 0.0, SCREEN_W, SCREEN_H, Color::new(0.0, 0.0, 0.0, 0.75));
}
