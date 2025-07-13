// Platform module
pub mod details;
pub mod banner;

pub mod parameters {
    pub mod gui {
        pub const APPLICATION_NAME: &str = "CopperForge - PCB & CAM for KiCad";
        pub const VERSION: &str = env!("CARGO_PKG_VERSION"); // Single source of truth from Cargo.toml
        #[allow(dead_code)]
        pub const VIEWPORT_X: f32 = 800.0;
        #[allow(dead_code)]
        pub const VIEWPORT_Y: f32 = 600.0;
    }
}