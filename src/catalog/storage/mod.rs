#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(target_arch = "wasm32")]
pub(super) use browser::Cache;
#[cfg(not(target_arch = "wasm32"))]
pub(super) use native::Cache;
