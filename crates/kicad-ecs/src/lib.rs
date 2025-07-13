//! # kicad-ecs
//! 
//! Entity Component System architecture for KiCad PCB design data.
//! 
//! This crate provides an ECS-based approach to working with KiCad board data,
//! enabling flexible queries, extensible analysis, and high-performance processing.

pub mod client;
pub mod components;
pub mod systems;
pub mod world;
pub mod tracing;

pub mod prelude {
    pub use crate::client::KiCadClient;
    pub use crate::components::*;
    pub use crate::world::PcbWorld;
    pub use bevy_ecs::prelude::*;
    pub use eyre::Result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        // TODO: Add tests
    }
}