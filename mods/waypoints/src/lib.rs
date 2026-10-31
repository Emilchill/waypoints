//! Waypoints for Cinnabar: a death point plus your own named waypoints, kept per world and
//! per dimension, with colors, icons, beams and the distance when you look at them.

pub mod config;
pub mod hud;
pub mod json;
pub mod panel;
pub mod mods_core;
pub mod font;
pub mod label;
pub mod shapes;
pub mod surface_controls;

#[cfg(target_arch = "wasm32")]
mod guest;
