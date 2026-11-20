//! The desktop app's native side: stores, servers and `plait-core`, behind
//! `plait-ui`'s [`plait_ui::Backend`]. The window lives in `main.rs`.

pub mod native;
pub mod settings;

pub use native::NativeBackend;
