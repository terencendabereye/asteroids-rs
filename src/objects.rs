//! Game-object hierarchy. Rust has no class inheritance, so the tree is expressed as a chain of
//! traits (each one a supertrait of the one above) with shared state in the `RigidBody` struct:
//!
//! ```text
//! GameObject            position, update(dt), alive
//! ├─ Drawable           colour, draw()
//! │  ├─ Menu (ui.rs)    selectable list drawn by the UI layer
//! │  └─ Body            owns a RigidBody: velocity, rotation, spin, radius, screen wrap
//! │     ├─ Asteroid     gray, spinning, jagged polygon, splits when hit
//! │     ├─ Laser        short lifetime; red for the player, yellow when fired by a UFO
//! │     ├─ Ufo          green saucer (UFO mode): crosses the screen, fires aimed lasers
//! │     └─ Controllable handle_input()
//! │        └─ Player    score, lives, respawn invulnerability
//! │           └─ Spaceship   blue ship: rotate/aim, thrust, brake, fire lasers
//! ```
use crate::input::{Action, AimMode, Input};
use macroquad::prelude::*;
use macroquad::rand::gen_range;
use std::f32::consts::{PI, TAU};

pub const SCREEN_W: f32 = 1000.0;
pub const SCREEN_H: f32 = 750.0;

// ───────────────────────────── GameObject ─────────────────────────────
pub trait GameObject {
    fn position(&self) -> Vec2;
    fn update(&mut self, dt: f32);
    fn alive(&self) -> bool;
    fn kill(&mut self);
}

// ───────────────────────────── Drawable ───────────────────────────────
pub trait Drawable: GameObject {
    #[allow(dead_code)]
    fn color(&self) -> Color;
    fn draw(&self);
}

// ───────────────────────────── RigidBody / Body ───────────────────────
#[derive(Clone)]
pub struct RigidBody {
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,         // radians
    pub angular_velocity: f32, // radians / second
    pub radius: f32,           // collision radius
    pub color: Color,
    pub alive: bool,
}

impl RigidBody {
    pub fn new(position: Vec2, color: Color, radius: f32) -> Self {
        Self { position, velocity: Vec2::ZERO, rotation: 0.0, angular_velocity: 0.0, radius, color, alive: true }
    }

    /// Move, spin and wrap around the screen edges (toroidal, as in the arcade original).
    pub fn integrate(&mut self, dt: f32) {
        self.position += self.velocity * dt;
        self.rotation += self.angular_velocity * dt;
        let r = self.radius;
        if self.position.x < -r { self.position.x = SCREEN_W + r; }
        if self.position.x > SCREEN_W + r { self.position.x = -r; }
        if self.position.y < -r { self.position.y = SCREEN_H + r; }
        if self.position.y > SCREEN_H + r { self.position.y = -r; }
    }

    pub fn collides_with(&self, other: &RigidBody) -> bool {
        let r = self.radius + other.radius;
        self.position.distance_squared(other.position) < r * r
    }
}

pub trait Body: Drawable {
    fn body(&self) -> &RigidBody;
    #[allow(dead_code)]
    fn body_mut(&mut self) -> &mut RigidBody;
    fn collides_with(&self, other: &dyn Body) -> bool {
        self.body().collides_with(other.body())
    }
}

/// Implements the `GameObject` plumbing for a type that has an `rb: RigidBody` field.
macro_rules! game_object {
    ($t:ty, |$s:ident, $dt:ident| $update:block) => {
        impl GameObject for $t {
            fn position(&self) -> Vec2 { self.rb.position }
            fn alive(&self) -> bool { self.rb.alive }
            fn kill(&mut self) { self.rb.alive = false; }
            fn update(&mut $s, $dt: f32) $update
        }
        impl Body for $t {
            fn body(&self) -> &RigidBody { &self.rb }
            fn body_mut(&mut self) -> &mut RigidBody { &mut self.rb }
        }
    };
}

fn polar(center: Vec2, angle: f32, dist: f32) -> Vec2 {
    center + Vec2::new(angle.cos(), angle.sin()) * dist
}

// ───────────────────────────── Asteroid ───────────────────────────────
pub struct Asteroid {
    rb: RigidBody,
    pub size: u32, // 3 = large, 2 = medium, 1 = small
    shape: [f32; 10],
    speed_scale: f32,
}

impl Asteroid {
    pub fn new(position: Vec2, size: u32, speed_scale: f32) -> Self {
        let mut rb = RigidBody::new(position, GRAY, 14.0 * size as f32);
        let angle = gen_range(0.0, TAU);
        let speed = gen_range(30.0, 70.0) * (4 - size) as f32 * speed_scale;
        rb.velocity = Vec2::new(angle.cos(), angle.sin()) * speed;
        rb.angular_velocity = gen_range(-1.5, 1.5);
        let mut shape = [1.0; 10];
        for s in shape.iter_mut() { *s = gen_range(0.7, 1.15); }
        Self { rb, size, shape, speed_scale }
    }

    pub fn points(&self) -> u32 { match self.size { 3 => 20, 2 => 50, _ => 100 } }
    pub fn speed_scale(&self) -> f32 { self.speed_scale }
}

game_object!(Asteroid, |self, dt| { self.rb.integrate(dt); });

impl Drawable for Asteroid {
    fn color(&self) -> Color { self.rb.color }
    fn draw(&self) {
        let n = self.shape.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let a1 = self.rb.rotation + i as f32 * TAU / n as f32;
            let a2 = self.rb.rotation + j as f32 * TAU / n as f32;
            let p1 = polar(self.rb.position, a1, self.rb.radius * self.shape[i]);
            let p2 = polar(self.rb.position, a2, self.rb.radius * self.shape[j]);
            draw_line(p1.x, p1.y, p2.x, p2.y, 2.0, self.rb.color);
        }
    }
}

// ───────────────────────────── Laser ──────────────────────────────────
pub struct Laser {
    rb: RigidBody,
    pub hostile: bool, // fired by an enemy
    life: f32,
}

impl Laser {
    const SPEED: f32 = 650.0;

    pub fn new(position: Vec2, angle: f32, hostile: bool) -> Self {
        let mut rb = RigidBody::new(position, if hostile { YELLOW } else { RED }, 3.0);
        rb.rotation = angle;
        rb.velocity = Vec2::new(angle.cos(), angle.sin()) * Self::SPEED;
        Self { rb, hostile, life: 1.0 }
    }
}

game_object!(Laser, |self, dt| {
    self.rb.integrate(dt);
    self.life -= dt;
    if self.life <= 0.0 { self.rb.alive = false; }
});

impl Drawable for Laser {
    fn color(&self) -> Color { self.rb.color }
    fn draw(&self) {
        let tail = polar(self.rb.position, self.rb.rotation, -12.0);
        draw_line(tail.x, tail.y, self.rb.position.x, self.rb.position.y, 3.0, self.rb.color);
    }
}

// ───────────────────────────── Ufo ────────────────────────────────────
pub struct Ufo {
    rb: RigidBody,
    pub target: Vec2, // where the player is; set by the game each frame
    aim_error: f32,   // max aiming error in radians (shrinks with level)
    base_y: f32,
    t: f32,
    shot_timer: f32,
    shots: Vec<Laser>,
}

impl Ufo {
    pub fn new(aim_error: f32) -> Self {
        let from_left = gen_range(0, 2) == 0;
        let mut rb = RigidBody::new(Vec2::ZERO, GREEN, 18.0);
        let base_y = gen_range(80.0, SCREEN_H - 80.0);
        rb.position = Vec2::new(if from_left { -rb.radius } else { SCREEN_W + rb.radius }, base_y);
        rb.velocity = Vec2::new(if from_left { 110.0 } else { -110.0 }, 0.0);
        Self { rb, target: Vec2::new(SCREEN_W / 2.0, SCREEN_H / 2.0), aim_error, base_y, t: 0.0, shot_timer: 1.0, shots: Vec::new() }
    }

    pub fn points(&self) -> u32 { 200 }
    pub fn take_shots(&mut self) -> Vec<Laser> { std::mem::take(&mut self.shots) }
}

// A UFO leaves the screen instead of wrapping, so it doesn't use RigidBody::integrate.
game_object!(Ufo, |self, dt| {
    self.t += dt;
    self.rb.position.x += self.rb.velocity.x * dt;
    self.rb.position.y = self.base_y + (self.t * 2.0).sin() * 60.0;
    if self.rb.position.x < -self.rb.radius * 2.0 || self.rb.position.x > SCREEN_W + self.rb.radius * 2.0 {
        self.rb.alive = false;
    }
    self.shot_timer -= dt;
    if self.shot_timer <= 0.0 {
        self.shot_timer = 1.6;
        let d = self.target - self.rb.position;
        let ang = d.y.atan2(d.x) + gen_range(-self.aim_error, self.aim_error);
        self.shots.push(Laser::new(self.rb.position, ang, true));
    }
});

impl Drawable for Ufo {
    fn color(&self) -> Color { self.rb.color }
    fn draw(&self) {
        let (x, y, r, c) = (self.rb.position.x, self.rb.position.y, self.rb.radius, self.rb.color);
        draw_ellipse_lines(x, y, r, r * 0.4, 0.0, 2.0, c); // hull
        draw_line(x - r * 0.45, y - r * 0.3, x - r * 0.25, y - r * 0.75, 2.0, c); // dome
        draw_line(x - r * 0.25, y - r * 0.75, x + r * 0.25, y - r * 0.75, 2.0, c);
        draw_line(x + r * 0.25, y - r * 0.75, x + r * 0.45, y - r * 0.3, 2.0, c);
    }
}

// ───────────────────────────── Controllable ───────────────────────────
pub trait Controllable: Body {
    /// Called once per frame before `update`; reads the input layer.
    fn handle_input(&mut self, input: &Input, dt: f32);
}

// ───────────────────────────── Player ─────────────────────────────────
pub trait Player: Controllable {
    fn score(&self) -> u32;
    fn add_score(&mut self, points: u32);
    fn lives(&self) -> i32;
    fn invulnerable(&self) -> bool;
    /// Lose a life and respawn at the centre. Returns true if out of lives.
    fn die(&mut self) -> bool;
}

// ───────────────────────────── Spaceship ──────────────────────────────
pub struct Spaceship {
    rb: RigidBody,
    score: u32,
    lives: i32,
    invuln_timer: f32,
    cooldown: f32,
    brake: f32,
    thrusting: bool,
    thrust_angle: f32, // direction of the current thrust (for the flame)
    fired: bool,
    shots: Vec<Laser>,
}

impl Spaceship {
    const TURN_RATE: f32 = 4.0;
    const AIM_TURN_RATE: f32 = 14.0;
    const THRUST: f32 = 350.0;
    const MAX_SPEED: f32 = 450.0;

    pub fn new() -> Self {
        let mut rb = RigidBody::new(Vec2::new(SCREEN_W / 2.0, SCREEN_H / 2.0), BLUE, 12.0);
        rb.rotation = -PI / 2.0; // pointing up
        Self { rb, score: 0, lives: 3, invuln_timer: 3.0, cooldown: 0.0, brake: 0.0, thrusting: false, thrust_angle: -PI / 2.0, fired: false, shots: Vec::new() }
    }

    pub fn position(&self) -> Vec2 { self.rb.position }
    pub fn body_ref(&self) -> &RigidBody { &self.rb }
    /// True once after each shot; the game uses it for rumble.
    pub fn consume_fired(&mut self) -> bool { std::mem::take(&mut self.fired) }
    pub fn take_shots(&mut self) -> Vec<Laser> { std::mem::take(&mut self.shots) }
}

game_object!(Spaceship, |self, dt| {
    // Light constant drag; braking damps velocity towards zero much harder.
    let drag = 0.6_f32.powf(dt) * 0.03_f32.powf(self.brake * dt);
    self.rb.velocity *= drag;
    let speed = self.rb.velocity.length();
    if speed > Self::MAX_SPEED { self.rb.velocity *= Self::MAX_SPEED / speed; }
    self.rb.integrate(dt);
    if self.invuln_timer > 0.0 { self.invuln_timer -= dt; }
});

impl Controllable for Spaceship {
    fn handle_input(&mut self, input: &Input, dt: f32) {
        if input.aim == AimMode::Keys {
            self.rb.angular_velocity =
                (input.strength(Action::RotateRight) - input.strength(Action::RotateLeft)) * Self::TURN_RATE;
        } else {
            // Stick / mouse aim: turn quickly towards the aim direction; rotate keys are ignored.
            self.rb.angular_velocity = 0.0;
            if let Some(target) = input.aim_angle(self.rb.position) {
                let mut diff = target - self.rb.rotation;
                while diff > PI { diff -= TAU; }
                while diff < -PI { diff += TAU; }
                let step = Self::AIM_TURN_RATE * dt;
                self.rb.rotation += diff.clamp(-step, step);
            }
        }

        let dir = Vec2::new(self.rb.rotation.cos(), self.rb.rotation.sin());
        if input.aim == AimMode::Keys {
            // Classic: thrust along the heading, brake damps velocity.
            let thrust = input.strength(Action::Thrust);
            self.thrusting = thrust > 0.0;
            self.thrust_angle = self.rb.rotation;
            self.rb.velocity += dir * Self::THRUST * thrust * dt;
            self.brake = input.strength(Action::Brake);
        } else {
            // Twin-stick: WASD / left stick move the ship in screen directions; it eases to a stop
            // when you let go.
            let m = input.move_vector();
            self.thrusting = m.length() > 0.0;
            if self.thrusting { self.thrust_angle = m.y.atan2(m.x); }
            self.rb.velocity += m * Self::THRUST * dt;
            self.brake = if self.thrusting { 0.0 } else { 0.6 };
        }

        self.cooldown -= dt;
        if input.fire_down() && self.cooldown <= 0.0 {
            self.cooldown = 0.2;
            self.fired = true;
            let nose = self.rb.position + dir * self.rb.radius;
            self.shots.push(Laser::new(nose, self.rb.rotation, false));
        }
    }
}

impl Player for Spaceship {
    fn score(&self) -> u32 { self.score }
    fn add_score(&mut self, points: u32) { self.score += points; }
    fn lives(&self) -> i32 { self.lives }
    fn invulnerable(&self) -> bool { self.invuln_timer > 0.0 }
    fn die(&mut self) -> bool {
        self.lives -= 1;
        self.rb.position = Vec2::new(SCREEN_W / 2.0, SCREEN_H / 2.0);
        self.rb.velocity = Vec2::ZERO;
        self.rb.rotation = -PI / 2.0;
        self.invuln_timer = 3.0;
        self.lives <= 0
    }
}

impl Drawable for Spaceship {
    fn color(&self) -> Color { self.rb.color }
    fn draw(&self) {
        if self.invulnerable() && ((get_time() * 10.0) as i64 % 2 == 1) { return; } // blink
        let (pos, rot, r, c) = (self.rb.position, self.rb.rotation, self.rb.radius, self.rb.color);
        let nose = polar(pos, rot, r * 1.3);
        let left = polar(pos, rot + 2.5, r);
        let right = polar(pos, rot - 2.5, r);
        let back = polar(pos, rot + PI, r * 0.5);
        for (a, b) in [(nose, left), (left, back), (back, right), (right, nose)] {
            draw_line(a.x, a.y, b.x, b.y, 2.5, c);
        }
        if self.thrusting && ((get_time() * 30.0) as i64 % 2 == 1) {
            let fa = self.thrust_angle + PI;
            let tip = polar(pos, fa, r * 1.6);
            for off in [0.4, -0.4] {
                let base = polar(pos, fa + off, r * 0.8);
                draw_line(base.x, base.y, tip.x, tip.y, 2.0, SKYBLUE);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_wrap_around_the_screen() {
        let mut b = RigidBody::new(Vec2::new(SCREEN_W + 50.0, 10.0), GRAY, 10.0);
        b.integrate(0.0);
        assert!(b.position.x < 0.0);
    }

    #[test]
    fn laser_expires_and_ufo_leaves_the_screen() {
        let mut l = Laser::new(Vec2::new(100.0, 100.0), 0.0, false);
        l.update(1.1);
        assert!(!l.alive());
        let mut u = Ufo::new(0.1);
        for _ in 0..200 { u.update(0.1); }
        assert!(!u.alive());
    }

    #[test]
    fn dying_costs_a_life_and_grants_invulnerability() {
        let mut s = Spaceship::new();
        assert!(!s.die() && !s.die());
        assert!(s.die()); // third life gone
        assert!(s.invulnerable());
    }
}
