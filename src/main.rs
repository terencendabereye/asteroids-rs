mod game;
mod graphics;
mod haptics;
mod input;
mod objects;
mod ui;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    graphics::Graphics::load().conf()
}

#[macroquad::main(window_conf)]
async fn main() {
    // Closing the window must go through our loop so the controller's motors/triggers are reset.
    prevent_quit();
    macroquad::rand::srand((get_time() * 1_000_000.0) as u64 ^ 0x9E37_79B9);
    let mut game = game::Game::new(graphics::Graphics::load());
    while !is_quit_requested() && game.frame() {
        next_frame().await;
    }
    drop(game);
}
