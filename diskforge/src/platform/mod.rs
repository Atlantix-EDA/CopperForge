//! Platform Crate 
//! 
//! The platform crate is a collection of modules that provide functionality for the Egui Mobius Event Manager.
//! The module provides system information such as operating system, architecture, and CPU information, 
//! number of cores, and memory information.
//! 
//! It also provides a banner module that displays a welcome message and the current date and time.
//! 
//! The parameters module contains constants for the GUI, including version, viewport size, and window title.
//! 
//! The config_operations module provides functions for managing application configuration,
//! including saving and loading color settings.
//! 
//! Example usage:
//! ```no_run
//! use platform::banner::Banner;
//! use platform::parameters::gui;
//! use platform::config_operations;
//!
//! let mut banner = Banner::new();
//! banner.format();
//! banner.print();
//! println!("Egui Mobius Event Manager Version: {}", gui::VERSION);
//! 
//! // Load default configuration
//! let colors = config_operations::load_default_color_configuration();
//! ```
//!
pub mod banner;
pub mod details; 
pub mod parameters; 
pub mod config_operations;
//pub use pins::Fpga;