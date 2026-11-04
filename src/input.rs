//! Input layer: logical actions -> keyboard key / gamepad button (remappable), plus fixed
//! menu navigation that can never be remapped away. Gamepads come from `gilrs` (pure Rust).
use gilrs::{Axis, Button, EventType, GamepadId, Gilrs};
use crate::graphics::View;
use macroquad::prelude::*;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action { RotateLeft, RotateRight, Thrust, Brake, Fire, MoveUp, MoveDown, MoveLeft, MoveRight }

pub const ACTIONS: [Action; 9] = [
    Action::RotateLeft, Action::RotateRight, Action::Thrust, Action::Brake, Action::Fire,
    Action::MoveUp, Action::MoveDown, Action::MoveLeft, Action::MoveRight,
];

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::RotateLeft => "Rotate Left",
            Action::RotateRight => "Rotate Right",
            Action::Thrust => "Accelerate",
            Action::Brake => "Brake",
            Action::Fire => "Fire",
            Action::MoveUp => "Move Up",
            Action::MoveDown => "Move Down",
            Action::MoveLeft => "Move Left",
            Action::MoveRight => "Move Right",
        }
    }
    /// Move actions are used with stick/mouse aim, the others with rotate-key aim (Fire with both).
    /// Rebinding only swaps with actions in the same group.
    fn is_move(self) -> bool { matches!(self, Action::MoveUp | Action::MoveDown | Action::MoveLeft | Action::MoveRight) }
    fn conflicts_with(self, other: Action) -> bool { self == Action::Fire || other == Action::Fire || self.is_move() == other.is_move() }
    fn index(self) -> usize { ACTIONS.iter().position(|a| *a == self).unwrap() }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum AimMode { Keys, Stick, Mouse }

impl AimMode {
    pub fn name(self) -> &'static str {
        match self { AimMode::Keys => "Rotate keys", AimMode::Stick => "Right stick", AimMode::Mouse => "Mouse" }
    }
    pub fn next(self) -> AimMode {
        match self { AimMode::Keys => AimMode::Stick, AimMode::Stick => AimMode::Mouse, AimMode::Mouse => AimMode::Keys }
    }
    fn to_i(self) -> i32 { match self { AimMode::Keys => 0, AimMode::Stick => 1, AimMode::Mouse => 2 } }
    fn from_i(i: i32) -> AimMode { match i { 1 => AimMode::Stick, 2 => AimMode::Mouse, _ => AimMode::Keys } }
}

// ── names (used for display and for the config file) ────────────────
const KEYS: &[(KeyCode, &str)] = &[
    (KeyCode::A, "A"), (KeyCode::B, "B"), (KeyCode::C, "C"), (KeyCode::D, "D"), (KeyCode::E, "E"),
    (KeyCode::F, "F"), (KeyCode::G, "G"), (KeyCode::H, "H"), (KeyCode::I, "I"), (KeyCode::J, "J"),
    (KeyCode::K, "K"), (KeyCode::L, "L"), (KeyCode::M, "M"), (KeyCode::N, "N"), (KeyCode::O, "O"),
    (KeyCode::P, "P"), (KeyCode::Q, "Q"), (KeyCode::R, "R"), (KeyCode::S, "S"), (KeyCode::T, "T"),
    (KeyCode::U, "U"), (KeyCode::V, "V"), (KeyCode::W, "W"), (KeyCode::X, "X"), (KeyCode::Y, "Y"),
    (KeyCode::Z, "Z"),
    (KeyCode::Key0, "0"), (KeyCode::Key1, "1"), (KeyCode::Key2, "2"), (KeyCode::Key3, "3"),
    (KeyCode::Key4, "4"), (KeyCode::Key5, "5"), (KeyCode::Key6, "6"), (KeyCode::Key7, "7"),
    (KeyCode::Key8, "8"), (KeyCode::Key9, "9"),
    (KeyCode::Space, "SPACE"), (KeyCode::Enter, "ENTER"), (KeyCode::Tab, "TAB"),
    (KeyCode::Left, "LEFT"), (KeyCode::Right, "RIGHT"), (KeyCode::Up, "UP"), (KeyCode::Down, "DOWN"),
    (KeyCode::LeftShift, "L-SHIFT"), (KeyCode::RightShift, "R-SHIFT"),
    (KeyCode::LeftControl, "L-CTRL"), (KeyCode::RightControl, "R-CTRL"),
    (KeyCode::LeftAlt, "L-ALT"), (KeyCode::RightAlt, "R-ALT"),
    (KeyCode::Comma, ","), (KeyCode::Period, "."), (KeyCode::Slash, "/"), (KeyCode::Semicolon, ";"),
    (KeyCode::Apostrophe, "'"), (KeyCode::LeftBracket, "["), (KeyCode::RightBracket, "]"),
];

pub fn key_name(k: KeyCode) -> String {
    KEYS.iter().find(|(c, _)| *c == k).map(|(_, n)| n.to_string()).unwrap_or_else(|| "?".into())
}

// (button, config name, display name)
const BUTTONS: &[(Button, &str, &str)] = &[
    (Button::DPadUp, "dpad_up", "D-PAD UP"),
    (Button::DPadRight, "dpad_right", "D-PAD RIGHT"),
    (Button::DPadDown, "dpad_down", "D-PAD DOWN"),
    (Button::DPadLeft, "dpad_left", "D-PAD LEFT"),
    (Button::North, "north", "Y / TRIANGLE"),
    (Button::East, "east", "B / CIRCLE"),
    (Button::South, "south", "A / CROSS"),
    (Button::West, "west", "X / SQUARE"),
    (Button::LeftTrigger, "lb", "LB"),
    (Button::LeftTrigger2, "lt", "LT"),
    (Button::RightTrigger, "rb", "RB"),
    (Button::RightTrigger2, "rt", "RT"),
    (Button::Select, "select", "SELECT"),
    (Button::Mode, "mode", "HOME"),
    (Button::LeftThumb, "lstick", "L-STICK"),
    (Button::RightThumb, "rstick", "R-STICK"),
];

pub fn button_name(b: Option<Button>) -> String {
    match b {
        None => "-".into(),
        Some(b) => BUTTONS.iter().find(|(c, _, _)| *c == b).map(|(_, _, n)| n.to_string()).unwrap_or_else(|| "BTN".into()),
    }
}

#[derive(Copy, Clone)]
pub struct Binding { pub key: KeyCode, pub button: Option<Button> }

const CONFIG_FILE: &str = "controls.cfg";
const DEADZONE: f32 = 0.25;

pub struct Input {
    gilrs: Option<Gilrs>,
    pad: Option<GamepadId>,
    pressed: Vec<Button>, // buttons that went down this frame
    bindings: [Binding; 9],
    pub aim: AimMode,
    pub vibration: bool,
    pub adaptive_triggers: bool,
    // mouse, in game-space coordinates
    pub mouse: Vec2,
    pub mouse_moved: bool,
    pub clicked: bool,
    pub right_clicked: bool,
    pub wheel: f32,
}

fn default_bindings() -> [Binding; 9] {
    [
        Binding { key: KeyCode::Left, button: Some(Button::DPadLeft) },
        Binding { key: KeyCode::Right, button: Some(Button::DPadRight) },
        Binding { key: KeyCode::Up, button: Some(Button::DPadUp) },
        Binding { key: KeyCode::Down, button: Some(Button::DPadDown) },
        Binding { key: KeyCode::Space, button: Some(Button::South) },
        Binding { key: KeyCode::W, button: Some(Button::DPadUp) },
        Binding { key: KeyCode::S, button: Some(Button::DPadDown) },
        Binding { key: KeyCode::A, button: Some(Button::DPadLeft) },
        Binding { key: KeyCode::D, button: Some(Button::DPadRight) },
    ]
}

impl Input {
    pub fn new() -> Self {
        let mut s = Self {
            gilrs: Gilrs::new().ok(), // no gamepad subsystem is not fatal
            pad: None,
            pressed: Vec::new(),
            bindings: default_bindings(),
            aim: AimMode::Keys,
            vibration: true,
            adaptive_triggers: true,
            mouse: Vec2::ZERO,
            mouse_moved: false,
            clicked: false,
            right_clicked: false,
            wheel: 0.0,
        };
        s.load();
        s
    }

    /// Call once per frame: pumps gamepad events and records this frame's button presses.
    pub fn poll(&mut self, view: &View) {
        let (mx, my) = mouse_position();
        let m = view.to_world(Vec2::new(mx, my));
        self.mouse_moved = m.distance(self.mouse) > 0.5;
        self.mouse = m;
        self.clicked = is_mouse_button_pressed(MouseButton::Left);
        self.right_clicked = is_mouse_button_pressed(MouseButton::Right);
        self.wheel = mouse_wheel().1;

        self.pressed.clear();
        if let Some(g) = self.gilrs.as_mut() {
            while let Some(ev) = g.next_event() {
                match ev.event {
                    EventType::ButtonPressed(b, _) => { self.pad = Some(ev.id); self.pressed.push(b); }
                    EventType::Connected => { if self.pad.is_none() { self.pad = Some(ev.id); } }
                    EventType::Disconnected => { if self.pad == Some(ev.id) { self.pad = None; } }
                    _ => {}
                }
            }
            if self.pad.is_none() {
                self.pad = g.gamepads().find(|(_, gp)| gp.is_connected()).map(|(id, _)| id);
            }
        }
    }

    pub fn pad_available(&self) -> bool { self.pad.is_some() }

    fn pad_down(&self, b: Button) -> bool {
        match (&self.gilrs, self.pad) { (Some(g), Some(id)) => g.gamepad(id).is_pressed(b), _ => false }
    }
    fn axis(&self, a: Axis) -> f32 {
        match (&self.gilrs, self.pad) { (Some(g), Some(id)) => g.gamepad(id).value(a), _ => 0.0 }
    }
    pub fn pad_pressed(&self, b: Button) -> bool { self.pressed.contains(&b) }

    // ── bindings ───────────────────────────────────────────────────
    pub fn binding(&self, a: Action) -> Binding { self.bindings[a.index()] }
    pub fn reset_defaults(&mut self) { self.bindings = default_bindings(); }

    /// Rebind; if another action already uses the key/button the two are swapped.
    pub fn set_key(&mut self, a: Action, key: KeyCode) {
        let old = self.bindings[a.index()].key;
        for (i, b) in self.bindings.iter_mut().enumerate() {
            if i != a.index() && ACTIONS[i].conflicts_with(a) && b.key == key { b.key = old; }
        }
        self.bindings[a.index()].key = key;
    }
    pub fn set_button(&mut self, a: Action, button: Button) {
        let old = self.bindings[a.index()].button;
        for (i, b) in self.bindings.iter_mut().enumerate() {
            if i != a.index() && ACTIONS[i].conflicts_with(a) && b.button == Some(button) { b.button = old; }
        }
        self.bindings[a.index()].button = Some(button);
    }

    // ── gameplay queries ───────────────────────────────────────────
    /// 0..1 strength of an action from keyboard, bound pad button, or left stick.
    pub fn strength(&self, a: Action) -> f32 {
        let b = self.binding(a);
        if is_key_down(b.key) { return 1.0; }
        if !self.pad_available() { return 0.0; }
        if let Some(btn) = b.button {
            if self.pad_down(btn) { return 1.0; }
        }
        let x = self.axis(Axis::LeftStickX);
        let y = self.axis(Axis::LeftStickY); // gilrs: up is positive
        let v = match a {
            Action::RotateLeft => -x,
            Action::RotateRight => x,
            Action::Thrust => y,
            Action::Brake => -y,
            Action::MoveUp => y,
            Action::MoveDown => -y,
            Action::MoveLeft => -x,
            Action::MoveRight => x,
            Action::Fire => return 0.0, // Fire has no stick binding
        };
        if v < DEADZONE { 0.0 } else { ((v - DEADZONE) / (1.0 - DEADZONE)).min(1.0) }
    }
    pub fn down(&self, a: Action) -> bool { self.strength(a) > 0.5 }

    /// Screen-space movement direction (y down), length 0..1, for stick/mouse aim modes.
    pub fn move_vector(&self) -> Vec2 {
        let v = Vec2::new(
            self.strength(Action::MoveRight) - self.strength(Action::MoveLeft),
            self.strength(Action::MoveDown) - self.strength(Action::MoveUp),
        );
        if v.length() > 1.0 { v.normalize() } else { v }
    }

    /// Fire: the bound key/button, R2 on a pad, or left click in mouse-aim mode.
    pub fn fire_down(&self) -> bool {
        self.down(Action::Fire)
            || (self.aim == AimMode::Mouse && is_mouse_button_down(MouseButton::Left))
            || self.pad_down(Button::RightTrigger2)
    }

    /// Aim direction (radians) for the stick/mouse modes. `None` if there is no aim input right
    /// now (stick centred, mouse on top of the ship), so the caller keeps its heading.
    pub fn aim_angle(&self, from: Vec2) -> Option<f32> {
        match self.aim {
            AimMode::Stick => {
                let (x, y) = (self.axis(Axis::RightStickX), -self.axis(Axis::RightStickY)); // screen y is down
                if x.hypot(y) < 0.3 { None } else { Some(y.atan2(x)) } // radial deadzone
            }
            AimMode::Mouse => {
                let d = self.mouse - from;
                if d.length() < 8.0 { None } else { Some(d.y.atan2(d.x)) }
            }
            AimMode::Keys => None,
        }
    }

    // ── fixed controls ─────────────────────────────────────────────
    pub fn pause_pressed(&self) -> bool { is_key_pressed(KeyCode::Escape) || self.pad_pressed(Button::Start) }

    /// First bindable pad button pressed this frame (START is reserved for pause).
    pub fn pressed_button(&self) -> Option<Button> {
        self.pressed.iter().copied().find(|b| BUTTONS.iter().any(|(c, _, _)| c == b))
    }

    /// First bindable keyboard key pressed this frame.
    pub fn pressed_key(&self) -> Option<KeyCode> {
        get_last_key_pressed().filter(|k| KEYS.iter().any(|(c, _)| c == k))
    }

    // ── persistence ────────────────────────────────────────────────
    pub fn save(&self) {
        let mut out = String::from("v2\n");
        for b in &self.bindings {
            let key = key_name(b.key);
            let btn = b.button.and_then(|x| BUTTONS.iter().find(|(c, _, _)| *c == x)).map(|(_, n, _)| *n).unwrap_or("none");
            out.push_str(&format!("{} {}\n", key.replace(' ', "_"), btn));
        }
        out.push_str(&format!("{} {} {}\n", self.aim.to_i(), self.vibration as i32, self.adaptive_triggers as i32));
        let _ = std::fs::write(CONFIG_FILE, out);
    }

    fn load(&mut self) {
        let Ok(text) = std::fs::read_to_string(CONFIG_FILE) else { return };
        let mut lines = text.lines();
        if lines.next() != Some("v2") { return; } // older/unknown format: keep defaults
        let mut loaded = default_bindings();
        for slot in loaded.iter_mut() {
            let Some(line) = lines.next() else { return };
            let mut parts = line.split_whitespace();
            let (Some(k), Some(b)) = (parts.next(), parts.next()) else { return };
            let Some(key) = KEYS.iter().find(|(_, n)| *n == k).map(|(c, _)| *c) else { return };
            slot.key = key;
            slot.button = BUTTONS.iter().find(|(_, n, _)| *n == b).map(|(c, _, _)| *c);
        }
        self.bindings = loaded;
        // The settings line is optional so older files keep their bindings.
        if let Some(line) = lines.next() {
            let v: Vec<i32> = line.split_whitespace().filter_map(|s| s.parse().ok()).collect();
            if v.len() == 3 {
                self.aim = AimMode::from_i(v[0]);
                self.vibration = v[1] != 0;
                self.adaptive_triggers = v[2] != 0;
            }
        }
    }
}

/// Menu navigation: arrows/W/S/Enter/Esc, or D-pad / left stick / A / B.
#[derive(Default)]
pub struct MenuNav {
    pub mouse: Vec2,
    pub mouse_moved: bool,
    pub clicked: bool,
    pub up: bool,
    pub down: bool,
    pub confirm: bool,
    pub back: bool,
    prev_up: bool,
    prev_down: bool,
}

impl MenuNav {
    pub fn poll(&mut self, input: &Input) {
        let y = input.axis(Axis::LeftStickY); // up is positive
        let (stick_up, stick_down) = (y > 0.6, y < -0.6);
        self.up = is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::W) || input.pad_pressed(Button::DPadUp) || (stick_up && !self.prev_up);
        self.down = is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::S) || input.pad_pressed(Button::DPadDown) || (stick_down && !self.prev_down);
        self.confirm = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) || input.pad_pressed(Button::South);
        self.back = is_key_pressed(KeyCode::Escape) || input.pad_pressed(Button::East) || input.right_clicked;
        self.up |= input.wheel > 0.0;
        self.down |= input.wheel < 0.0;
        self.mouse = input.mouse;
        self.mouse_moved = input.mouse_moved;
        self.clicked = input.clicked;
        self.prev_up = stick_up;
        self.prev_down = stick_down;
    }
}
