//! Most common types that can be glob-imported `use macroquad::prelude::*` for convenience.

pub use crate::camera::*;
pub use crate::file::*;
pub use crate::input::*;
pub use crate::material::*;
pub use crate::math::*;
pub use crate::models::*;
pub use crate::shapes::*;

#[cfg(feature = "text")]
pub use crate::text::*;

pub use crate::texture::*;
pub use crate::time::*;
pub use crate::window::*;

pub use crate::color::Color;
pub use crate::logging::*;
pub use crate::quad_gl::{DrawMode, GlPipeline, QuadGl};
pub use crate::{color_u8, DroppedFile};
pub use glam;

#[cfg(feature = "image")]
pub use image::ImageFormat;

pub use miniquad::{
    conf::Conf, Comparison, PipelineParams, ShaderError, ShaderSource, UniformDesc, UniformType,
};

#[cfg(feature = "rand")]
pub use quad_rand as rand;
