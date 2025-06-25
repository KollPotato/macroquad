use macroquad::prelude::*;

fn main() {
    macroquad::run("Square", update());
}

async fn update() {
    loop {
        clear_background(Color::WHITE);

        let size = 40.0;

        draw_rectangle(
            (screen_width() - size) / 2.0,
            (screen_height() - size) / 2.0,
            size,
            size,
            Color::BLUE,
        );

        next_frame().await
    }
}
