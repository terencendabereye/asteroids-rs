//! UI layer: menus and text helpers. `Menu` is a `Drawable`, so it sits in the same hierarchy.
use crate::input::MenuNav;
use crate::objects::{Drawable, GameObject, SCREEN_H, SCREEN_W};
use macroquad::prelude::*;

/// Draw text with its top edge at `y` (macroquad's own `y` is the baseline).
pub fn draw_text_top(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(text, x, y + size * 0.8, size, color);
}

pub fn text_width(text: &str, size: f32) -> f32 {
    measure_text(text, None, size as u16, 1.0).width
}

pub fn draw_centered(text: &str, y: f32, size: f32, color: Color) {
    draw_text_top(text, (SCREEN_W - text_width(text, size)) / 2.0, y, size, color);
}

#[derive(Clone)]
pub struct Row {
    pub a: String, // label (centred when b is empty, left column otherwise)
    pub b: String, // optional extra columns (settings table)
    pub c: String,
}

impl Row {
    pub fn label(a: &str) -> Self { Self { a: a.into(), b: String::new(), c: String::new() } }
    pub fn table(a: &str, b: &str, c: &str) -> Self { Self { a: a.into(), b: b.into(), c: c.into() } }
}

pub struct Menu {
    pub title: String,
    pub rows: Vec<Row>,
    pub selected: usize,
    pub hint: String,
    /// Vertical distance between rows, and from the title to the first row.
    pub spacing: f32,
    pub first_row: f32,
    position: Vec2,
    t: f32,
}

impl Menu {
    pub fn new(title: &str, y: f32, labels: &[&str]) -> Self {
        Self {
            title: title.into(),
            rows: labels.iter().map(|l| Row::label(l)).collect(),
            selected: 0,
            hint: String::new(),
            spacing: 44.0,
            first_row: 100.0,
            position: Vec2::new(SCREEN_W / 2.0, y),
            t: 0.0,
        }
    }

    fn row_y(&self, i: usize) -> f32 { self.position.y + self.first_row + i as f32 * self.spacing }

    fn row_at(&self, p: Vec2) -> Option<usize> {
        (0..self.rows.len()).find(|&i| {
            let y = self.row_y(i);
            p.x >= SCREEN_W / 2.0 - 360.0 && p.x <= SCREEN_W / 2.0 + 360.0 && p.y >= y - 6.0 && p.y <= y - 6.0 + self.spacing - 4.0
        })
    }

    /// Apply keyboard / pad / mouse navigation; returns the activated row index, if any.
    /// Hovering a row selects it and clicking it activates it.
    pub fn handle(&mut self, nav: &MenuNav) -> Option<usize> {
        let n = self.rows.len();
        if nav.up { self.selected = (self.selected + n - 1) % n; }
        if nav.down { self.selected = (self.selected + 1) % n; }
        let hovered = self.row_at(nav.mouse);
        if nav.mouse_moved || nav.clicked {
            if let Some(i) = hovered { self.selected = i; }
        }
        if nav.clicked && hovered.is_some() { return hovered; }
        nav.confirm.then_some(self.selected)
    }
}

impl GameObject for Menu {
    fn position(&self) -> Vec2 { self.position }
    fn update(&mut self, dt: f32) { self.t += dt; }
    fn alive(&self) -> bool { true }
    fn kill(&mut self) {}
}

impl Drawable for Menu {
    fn color(&self) -> Color { WHITE }

    fn draw(&self) {
        draw_centered(&self.title, self.position.y, 56.0, WHITE);
        let table = self.rows.first().is_some_and(|r| !r.b.is_empty());
        let (col_a, col_b, col_c) = (SCREEN_W / 2.0 - 330.0, SCREEN_W / 2.0 - 20.0, SCREEN_W / 2.0 + 170.0);
        for (i, r) in self.rows.iter().enumerate() {
            let y = self.row_y(i);
            let sel = i == self.selected;
            let col = if sel { Color::new(1.0, 1.0, 1.0, 0.78 + 0.22 * (self.t * 6.0).sin()) } else { GRAY };
            if sel { draw_rectangle_lines(SCREEN_W / 2.0 - 360.0, y - 6.0, 720.0, self.spacing - 6.0, 1.0, DARKGRAY); }
            if table && !r.b.is_empty() {
                let val = if sel { SKYBLUE } else { Color::from_rgba(80, 120, 160, 255) };
                draw_text_top(&r.a, col_a, y, 24.0, col);
                draw_text_top(&r.b, col_b, y, 24.0, val);
                draw_text_top(&r.c, col_c, y, 24.0, val);
            } else {
                draw_centered(&r.a, y, 28.0, col);
            }
        }
        if !self.hint.is_empty() { draw_centered(&self.hint, SCREEN_H - 50.0, 18.0, GRAY); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_hover_selects_and_click_activates_a_row() {
        let mut m = Menu::new("T", 100.0, &["a", "b", "c"]);
        let row_b = Vec2::new(SCREEN_W / 2.0, 100.0 + 100.0 + 44.0 + 10.0); // inside row 1
        let mut nav = MenuNav::default();
        nav.mouse = row_b;
        nav.mouse_moved = true;
        assert_eq!(m.handle(&nav), None); // hover only selects
        assert_eq!(m.selected, 1);
        nav.mouse_moved = false;
        nav.clicked = true;
        assert_eq!(m.handle(&nav), Some(1)); // click activates
        nav.mouse = Vec2::new(5.0, 5.0); // click outside every row does nothing
        assert_eq!(m.handle(&nav), None);
    }
}
