//!
//! `macroquad` is a simple and easy to use game library for Rust programming language.
//!
//! `macroquad` attempts to avoid any rust-specific programming concepts like lifetimes/borrowing, making it very friendly for rust beginners.
//!
//! ## Supported platforms
//!
//! * PC: Windows/Linux/MacOS
//! * HTML5
//! * Android
//! * IOS
//!
//! ## Features
//!
//! * Same code for all supported platforms, no platform dependent defines required
//! * Efficient 2D rendering with automatic geometry batching
//! * Minimal amount of dependencies: build after `cargo clean` takes only 16s on x230(~6years old laptop)
//! * Immediate mode UI library included
//! * Single command deploy for both WASM and Android [build instructions](https://github.com/not-fl3/miniquad/#building-examples)
//! # Example
//! ```no_run
//! use macroquad::prelude::*;
//!
//! #[macroquad::main("BasicShapes")]
//! async fn main() {
//!     loop {
//!         clear_background(RED);
//!
//!         draw_line(40.0, 40.0, 100.0, 200.0, 15.0, BLUE);
//!         draw_rectangle(screen_width() / 2.0 - 60.0, 100.0, 120.0, 60.0, GREEN);
//!         draw_circle(screen_width() - 30.0, screen_height() - 30.0, 15.0, YELLOW);
//!         draw_text("HELLO", 20.0, 20.0, 20.0, DARKGRAY);
//!
//!         next_frame().await
//!     }
//! }
//!```

use conf::Config;
use miniquad::*;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::pin::Pin;

mod exec;
mod quad_gl;
mod tobytes;

pub mod audio;
pub mod camera;
pub mod color;
pub mod file;
pub mod input;
pub mod material;
pub mod math;
pub mod models;
pub mod shapes;

#[cfg(feature = "text")]
pub mod text;

pub mod texture;
pub mod time;
pub mod window;

pub mod prelude;
pub use error::Error;

mod error;

#[cfg(not(feature = "log"))]
/// Logging macros, available with miniquad "log-impl" feature.
pub mod logging {
    pub use miniquad::{debug, error, info, trace, warn};
}

#[cfg(feature = "log")]
pub use ::log as logging;

pub use miniquad;

use crate::{color::Color, quad_gl::QuadGl, texture::TextureHandle};
use glam::{vec2, Mat4, Vec2};

pub(crate) mod thread_assert {
    static mut THREAD_ID: Option<std::thread::ThreadId> = None;

    pub fn set_thread_id() {
        unsafe {
            THREAD_ID = Some(std::thread::current().id());
        }
    }

    pub fn same_thread() {
        unsafe {
            thread_local! {
                static CURRENT_THREAD_ID: std::thread::ThreadId = std::thread::current().id();
            }

            assert!(THREAD_ID.is_some());
            assert!(THREAD_ID.unwrap() == CURRENT_THREAD_ID.with(|id| *id));
        }
    }
}

pub struct Context {
    audio_context: audio::AudioContext,

    screen_width: f32,
    screen_height: f32,

    simulate_mouse_with_touch: bool,

    keys_down: HashSet<KeyCode>,
    keys_pressed: HashSet<KeyCode>,
    keys_released: HashSet<KeyCode>,
    mouse_down: HashSet<MouseButton>,
    mouse_pressed: HashSet<MouseButton>,
    mouse_released: HashSet<MouseButton>,
    touches: HashMap<u64, input::Touch>,
    chars_pressed_queue: Vec<char>,
    chars_pressed_ui_queue: Vec<char>,
    mouse_position: Vec2,
    last_mouse_position: Option<Vec2>,
    mouse_wheel: Vec2,

    prevent_quit_event: bool,
    quit_requested: bool,

    cursor_grabbed: bool,

    input_events: Vec<Vec<MiniquadInputEvent>>,

    gl: QuadGl,
    camera_matrix: Option<Mat4>,

    pc_assets_folder: Option<String>,

    start_time: f64,
    last_frame_time: f64,
    frame_time: f64,

    camera_stack: Vec<camera::CameraState>,

    unwind: bool,
    recovery_future: Option<Pin<Box<dyn Future<Output = ()>>>>,

    quad_context: Box<dyn miniquad::RenderingBackend>,

    default_filter_mode: crate::quad_gl::FilterMode,
    textures: crate::texture::TexturesContext,

    update_on: conf::UpdateTrigger,

    dropped_files: Vec<DroppedFile>,
}

#[derive(Clone)]
enum MiniquadInputEvent {
    MouseMotion {
        x: f32,
        y: f32,
    },
    MouseWheel {
        x: f32,
        y: f32,
    },
    MouseButtonDown {
        x: f32,
        y: f32,
        btn: MouseButton,
    },
    MouseButtonUp {
        x: f32,
        y: f32,
        btn: MouseButton,
    },
    Char {
        character: char,
        modifiers: KeyMods,
        repeat: bool,
    },
    KeyDown {
        keycode: KeyCode,
        modifiers: KeyMods,
        repeat: bool,
    },
    KeyUp {
        keycode: KeyCode,
        modifiers: KeyMods,
    },
    Touch {
        phase: TouchPhase,
        id: u64,
        x: f32,
        y: f32,
    },
    WindowMinimized,
    WindowRestored,
}

impl MiniquadInputEvent {
    fn repeat<T: miniquad::EventHandler>(&self, t: &mut T) {
        use crate::MiniquadInputEvent::*;
        match self {
            MouseMotion { x, y } => t.mouse_motion_event(*x, *y),
            MouseWheel { x, y } => t.mouse_wheel_event(*x, *y),
            MouseButtonDown { x, y, btn } => t.mouse_button_down_event(*btn, *x, *y),
            MouseButtonUp { x, y, btn } => t.mouse_button_up_event(*btn, *x, *y),
            Char {
                character,
                modifiers,
                repeat,
            } => t.char_event(*character, *modifiers, *repeat),
            KeyDown {
                keycode,
                modifiers,
                repeat,
            } => t.key_down_event(*keycode, *modifiers, *repeat),
            KeyUp { keycode, modifiers } => t.key_up_event(*keycode, *modifiers),
            Touch { phase, id, x, y } => t.touch_event(*phase, *id, *x, *y),
            WindowMinimized => t.window_minimized_event(),
            WindowRestored => t.window_restored_event(),
        }
    }
}

impl Context {
    fn new(
        update_on: conf::UpdateTrigger,
        default_filter_mode: crate::FilterMode,
        draw_call_vertex_capacity: usize,
        draw_call_index_capacity: usize,
    ) -> Context {
        let mut ctx: Box<dyn miniquad::RenderingBackend> =
            miniquad::window::new_rendering_backend();
        let (screen_width, screen_height) = miniquad::window::screen_size();

        Context {
            screen_width,
            screen_height,

            simulate_mouse_with_touch: true,

            keys_down: HashSet::new(),
            keys_pressed: HashSet::new(),
            keys_released: HashSet::new(),
            chars_pressed_queue: Vec::new(),
            chars_pressed_ui_queue: Vec::new(),
            mouse_down: HashSet::new(),
            mouse_pressed: HashSet::new(),
            mouse_released: HashSet::new(),
            touches: HashMap::new(),
            mouse_position: vec2(0., 0.),
            last_mouse_position: None,
            mouse_wheel: vec2(0., 0.),

            prevent_quit_event: false,
            quit_requested: false,

            cursor_grabbed: false,

            input_events: Vec::new(),

            camera_matrix: None,
            gl: QuadGl::new(
                &mut *ctx,
                draw_call_vertex_capacity,
                draw_call_index_capacity,
            ),

            camera_stack: vec![],

            audio_context: audio::AudioContext::new(),

            pc_assets_folder: None,

            start_time: miniquad::date::now(),
            last_frame_time: miniquad::date::now(),
            frame_time: 1. / 60.,

            unwind: false,
            recovery_future: None,

            quad_context: ctx,

            default_filter_mode,
            textures: crate::texture::TexturesContext::new(),
            update_on,

            dropped_files: Vec::new(),
        }
    }

    /// Returns the handle for this texture.
    pub(crate) fn raw_miniquad_id(&self, handle: &TextureHandle) -> miniquad::TextureId {
        match handle {
            TextureHandle::Unmanaged(texture) => *texture,
            TextureHandle::Managed(texture) => self
                .textures
                .texture(texture.0)
                .unwrap_or(self.gl.white_texture),
            TextureHandle::ManagedWeak(texture) => self
                .textures
                .texture(*texture)
                .unwrap_or(self.gl.white_texture),
        }
    }

    /// Returns the files which have been dropped onto the window.
    pub fn dropped_files(&mut self) -> Vec<DroppedFile> {
        std::mem::take(&mut self.dropped_files)
    }

    fn begin_frame(&mut self) {
        let color = Color::BLACK;

        get_quad_context().clear(Some((color.r, color.g, color.b, color.a)), None, None);

        self.gl.reset();
    }

    fn end_frame(&mut self) {
        self.perform_render_passes();

        let screen_mat = self.pixel_perfect_projection_matrix();
        self.gl.draw(get_quad_context(), screen_mat);

        get_quad_context().commit_frame();

        self.mouse_wheel = Vec2::new(0., 0.);
        self.keys_pressed.clear();
        self.keys_released.clear();
        self.mouse_pressed.clear();
        self.mouse_released.clear();
        self.last_mouse_position = Some(crate::prelude::mouse_position_local());

        self.quit_requested = false;

        self.textures.garbage_collect(get_quad_context());

        // remove all touches that were Ended or Cancelled
        self.touches.retain(|_, touch| {
            touch.phase != input::TouchPhase::Ended && touch.phase != input::TouchPhase::Cancelled
        });

        // change all Started or Moved touches to Stationary
        for touch in self.touches.values_mut() {
            if touch.phase == input::TouchPhase::Started || touch.phase == input::TouchPhase::Moved
            {
                touch.phase = input::TouchPhase::Stationary;
            }
        }

        self.dropped_files.clear();
    }

    pub(crate) fn pixel_perfect_projection_matrix(&self) -> glam::Mat4 {
        let (width, height) = miniquad::window::screen_size();

        let dpi = miniquad::window::dpi_scale();

        glam::Mat4::orthographic_rh_gl(0., width / dpi, height / dpi, 0., -1., 1.)
    }

    pub(crate) fn projection_matrix(&self) -> glam::Mat4 {
        if let Some(matrix) = self.camera_matrix {
            matrix
        } else {
            self.pixel_perfect_projection_matrix()
        }
    }

    pub(crate) fn perform_render_passes(&mut self) {
        let matrix = self.projection_matrix();

        self.gl.draw(get_quad_context(), matrix);
    }
}

#[unsafe(no_mangle)]
static mut CONTEXT: Option<Context> = None;

// This is required for #[macroquad::test]
//
// unfortunately #[cfg(test)] do not work with integration tests
// so this module should be publicly available
#[doc(hidden)]
pub mod test {
    pub static mut MUTEX: Option<std::sync::Mutex<()>> = None;
    pub static ONCE: std::sync::Once = std::sync::Once::new();
}

fn get_context() -> &'static mut Context {
    thread_assert::same_thread();

    unsafe { CONTEXT.as_mut().unwrap_or_else(|| panic!()) }
}

fn get_quad_context() -> &'static mut dyn miniquad::RenderingBackend {
    thread_assert::same_thread();

    unsafe {
        assert!(CONTEXT.is_some());
    }

    unsafe { &mut *CONTEXT.as_mut().unwrap().quad_context }
}

struct Stage {
    main_future: Pin<Box<dyn Future<Output = ()>>>,
}

impl EventHandler for Stage {
    fn resize_event(&mut self, width: f32, height: f32) {
        get_context().screen_width = width;
        get_context().screen_height = height;

        if miniquad::window::blocking_event_loop() {
            miniquad::window::schedule_update();
        }
    }

    fn raw_mouse_motion(&mut self, x: f32, y: f32) {
        let context = get_context();

        if context.cursor_grabbed {
            context.mouse_position += Vec2::new(x, y);

            let event = MiniquadInputEvent::MouseMotion {
                x: context.mouse_position.x,
                y: context.mouse_position.y,
            };
            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(event.clone()));
        }
    }

    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        let context = get_context();

        if !context.cursor_grabbed {
            context.mouse_position = Vec2::new(x, y);

            context
                .input_events
                .iter_mut()
                .for_each(|arr| arr.push(MiniquadInputEvent::MouseMotion { x, y }));
        }

        if context.update_on.mouse_motion {
            miniquad::window::schedule_update();
        }
    }

    fn mouse_wheel_event(&mut self, x: f32, y: f32) {
        let context = get_context();

        context.mouse_wheel.x = x;
        context.mouse_wheel.y = y;

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::MouseWheel { x, y }));

        if context.update_on.mouse_wheel {
            miniquad::window::schedule_update();
        }
    }

    fn mouse_button_down_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        let context = get_context();

        context.mouse_down.insert(btn);
        context.mouse_pressed.insert(btn);

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::MouseButtonDown { x, y, btn }));

        if !context.cursor_grabbed {
            context.mouse_position = Vec2::new(x, y);
        }

        if context.update_on.mouse_down {
            miniquad::window::schedule_update();
        }
    }

    fn mouse_button_up_event(&mut self, btn: MouseButton, x: f32, y: f32) {
        let context = get_context();

        context.mouse_down.remove(&btn);
        context.mouse_released.insert(btn);

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::MouseButtonUp { x, y, btn }));

        if !context.cursor_grabbed {
            context.mouse_position = Vec2::new(x, y);
        }
        if context.update_on.mouse_up {
            miniquad::window::schedule_update();
        }
    }

    fn touch_event(&mut self, phase: TouchPhase, id: u64, x: f32, y: f32) {
        let context = get_context();

        context.touches.insert(
            id,
            input::Touch {
                id,
                phase: phase.into(),
                position: Vec2::new(x, y),
            },
        );

        if context.simulate_mouse_with_touch {
            if phase == TouchPhase::Started {
                self.mouse_button_down_event(MouseButton::Left, x, y);
            }

            if phase == TouchPhase::Ended {
                self.mouse_button_up_event(MouseButton::Left, x, y);
            }

            if phase == TouchPhase::Moved {
                self.mouse_motion_event(x, y);
            }
        } else if context.update_on.touch {
            miniquad::window::schedule_update();
        };

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::Touch { phase, id, x, y }));
    }

    fn char_event(&mut self, character: char, modifiers: KeyMods, repeat: bool) {
        let context = get_context();

        context.chars_pressed_queue.push(character);
        context.chars_pressed_ui_queue.push(character);

        context.input_events.iter_mut().for_each(|arr| {
            arr.push(MiniquadInputEvent::Char {
                character,
                modifiers,
                repeat,
            });
        });
    }

    fn key_down_event(&mut self, keycode: KeyCode, modifiers: KeyMods, repeat: bool) {
        let context = get_context();
        context.keys_down.insert(keycode);
        if !repeat {
            context.keys_pressed.insert(keycode);
        }

        context.input_events.iter_mut().for_each(|arr| {
            arr.push(MiniquadInputEvent::KeyDown {
                keycode,
                modifiers,
                repeat,
            });
        });
        if context
            .update_on
            .specific_key
            .as_ref()
            .map_or(context.update_on.key_down, |keys| keys.contains(&keycode))
        {
            miniquad::window::schedule_update();
        }
    }

    fn key_up_event(&mut self, keycode: KeyCode, modifiers: KeyMods) {
        let context = get_context();
        context.keys_down.remove(&keycode);
        context.keys_released.insert(keycode);

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::KeyUp { keycode, modifiers }));

        if miniquad::window::blocking_event_loop() {
            //miniquad::window::schedule_update();
        }
    }

    fn update(&mut self) {
        // Unless called every frame, cursor will not remain grabbed
        miniquad::window::set_cursor_grab(get_context().cursor_grabbed);

        #[cfg(not(target_arch = "wasm32"))]
        {
            // TODO: consider making it a part of miniquad?
            std::thread::yield_now();
        }
    }

    fn files_dropped_event(&mut self) {
        let context = get_context();
        for i in 0..miniquad::window::dropped_file_count() {
            context.dropped_files.push(DroppedFile {
                path: miniquad::window::dropped_file_path(i),
                bytes: miniquad::window::dropped_file_bytes(i),
            });
        }
    }

    fn draw(&mut self) {
        {
            use std::panic;

            get_context().begin_frame();

            fn maybe_unwind(unwind: bool, f: impl FnOnce() + Sized + panic::UnwindSafe) -> bool {
                if unwind {
                    panic::catch_unwind(f).is_ok()
                } else {
                    f();
                    true
                }
            }

            let result = maybe_unwind(
                get_context().unwind,
                AssertUnwindSafe(|| {
                    if exec::resume(&mut self.main_future).is_some() {
                        self.main_future = Box::pin(async move {});
                        miniquad::window::quit();
                        return;
                    }
                }),
            );

            if !result {
                if let Some(recovery_future) = get_context().recovery_future.take() {
                    self.main_future = recovery_future;
                }
            }

            get_context().end_frame();

            get_context().frame_time = date::now() - get_context().last_frame_time;
            get_context().last_frame_time = date::now();

            #[cfg(any(target_arch = "wasm32", target_os = "linux"))]
            {
                unsafe {
                    miniquad::gl::glFlush();
                    miniquad::gl::glFinish();
                }
            }
        }
    }

    fn window_restored_event(&mut self) {
        let context = get_context();

        #[cfg(target_os = "android")]
        context.audio_context.resume();
        #[cfg(target_os = "android")]
        if miniquad::window::blocking_event_loop() {
            miniquad::window::schedule_update();
        }

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::WindowRestored));
    }

    fn window_minimized_event(&mut self) {
        let context = get_context();

        #[cfg(target_os = "android")]
        context.audio_context.pause();

        // Clear held down keys and button and announce them as released
        context.mouse_released.extend(context.mouse_down.drain());
        context.keys_released.extend(context.keys_down.drain());

        // Announce all touches as released
        for (_, touch) in context.touches.iter_mut() {
            touch.phase = input::TouchPhase::Ended;
        }

        context
            .input_events
            .iter_mut()
            .for_each(|arr| arr.push(MiniquadInputEvent::WindowMinimized));
    }

    fn quit_requested_event(&mut self) {
        let context = get_context();
        if context.prevent_quit_event {
            miniquad::window::cancel_quit();
            context.quit_requested = true;
        }
    }
}

pub mod conf {
    #[derive(Default, Debug)]
    pub struct UpdateTrigger {
        pub key_down: bool,
        pub mouse_down: bool,
        pub mouse_up: bool,
        pub mouse_motion: bool,
        pub mouse_wheel: bool,
        pub specific_key: Option<Vec<crate::KeyCode>>,
        pub touch: bool,
    }

    #[derive(Debug)]
    pub struct Config {
        pub miniquad_conf: miniquad::conf::Conf,
        /// With miniquad_conf.platform.blocking_event_loop = true,
        /// next_frame().await will never finish and will wait forever with
        /// zero CPU usage.
        /// update_on will tell macroquad when to proceed with the event loop.
        pub update_on: Option<UpdateTrigger>,
        pub default_filter_mode: crate::FilterMode,
        /// Macroquad performs automatic and static batching for each
        /// draw_* call. For each draw call, it pre-allocate a huge cpu/gpu
        /// buffer to add vertices to. When it exceeds the buffer, it allocates the
        /// new one, marking the new draw call.
        ///
        /// Some examples when altering those values migh be convinient:
        /// - for huge 3d models that do not fit into a single draw call, increasing
        ///     the buffer size might be easier than splitting the model.
        /// - when each draw_* call got its own material,
        ///     buffer size might be reduced to save some memory
        pub draw_call_vertex_capacity: usize,
        pub draw_call_index_capacity: usize,
    }

    impl Default for Config {
        fn default() -> Self {
            Self {
                miniquad_conf: miniquad::conf::Conf::default(),
                update_on: Some(UpdateTrigger::default()),
                default_filter_mode: crate::FilterMode::Linear,
                draw_call_vertex_capacity: 10000,
                draw_call_index_capacity: 5000,
            }
        }
    }
}

impl From<miniquad::conf::Conf> for conf::Config {
    fn from(conf: miniquad::conf::Conf) -> conf::Config {
        conf::Config {
            miniquad_conf: conf,
            update_on: None,
            default_filter_mode: crate::FilterMode::Linear,
            draw_call_vertex_capacity: 10000,
            draw_call_index_capacity: 5000,
        }
    }
}

pub fn run(title: &str, future: impl Future<Output = ()> + 'static) {
    run_with_config(
        conf::Config {
            miniquad_conf: miniquad::conf::Conf {
                window_title: title.to_string(),
                ..Default::default()
            },
            ..Default::default()
        },
        future,
    );
}

pub fn run_with_config(config: Config, future: impl Future<Output = ()> + 'static) {
    let conf::Config {
        miniquad_conf,
        update_on,
        default_filter_mode,
        draw_call_vertex_capacity,
        draw_call_index_capacity,
    } = config.into();

    miniquad::start(miniquad_conf, move || {
        thread_assert::set_thread_id();
        let context = Context::new(
            update_on.unwrap_or_default(),
            default_filter_mode,
            draw_call_vertex_capacity,
            draw_call_index_capacity,
        );
        unsafe { CONTEXT = Some(context) };

        Box::new(Stage {
            main_future: Box::pin(async {
                future.await;
                unsafe {
                    if let Some(ctx) = CONTEXT.as_mut() {
                        ctx.gl.reset();
                    }
                }
            }),
        })
    });
}

/// Information about a dropped file.
pub struct DroppedFile {
    pub path: Option<PathBuf>,
    pub bytes: Option<Vec<u8>>,
}
