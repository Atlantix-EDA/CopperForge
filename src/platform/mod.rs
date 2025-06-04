// Platform module
pub mod details;
pub mod banner;

pub mod parameters {
    pub mod gui {
        pub const APPLICATION_NAME: &str = "KiForge - PCB & CAM for KiCad";
        pub const VERSION: &str = "1.0.0";
        #[allow(dead_code)]
        pub const VIEWPORT_X: f32 = 800.0;
        #[allow(dead_code)]
        pub const VIEWPORT_Y: f32 = 600.0;
    }
}