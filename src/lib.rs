//! Minecraft schematic documents, editing, codecs, validation, and native rendering.
//!
//! Load a catalog with catalog::MinecraftData before creating model::Schematic.
//! Region cells use local coordinates; region origins and render filters use world coordinates.

pub mod catalog;
mod convert;
mod edit;
pub mod formats;
pub mod helpers;
pub mod model;
pub mod nbt;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;
mod sprite_ids;
mod versions;

/// An operation result with a human-readable error message.
pub type Result<T> = std::result::Result<T, String>;
pub mod transform;
pub mod validate;
