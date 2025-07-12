# KiForge Workspace Architecture Design

## Overview
Transform the current KiForge structure into a cohesive workspace with `kicad-ecs` as the foundational crate for a unified KiCad/KiForge API.

## Proposed Workspace Structure

```
KiForge/                           # Root workspace
├── Cargo.toml                     # Workspace manifest
├── README.md                      # Main documentation
├── LICENSE                        # Workspace license
├── crates/
│   ├── kicad-ecs/                 # Foundation crate (moved from ../kicad-ecs)
│   │   ├── Cargo.toml             # Core KiCad data structures & ECS
│   │   ├── src/lib.rs             # KiCad API, components, systems
│   │   └── examples/              # KiCad integration examples
│   │
│   ├── kiforge-core/              # Core KiForge functionality
│   │   ├── Cargo.toml             # Layer management, display, navigation
│   │   ├── src/lib.rs             # Re-exports from display, layer_ops, etc.
│   │   └── src/
│   │       ├── display/           # Moved from src/display/
│   │       ├── layer_operations/  # Moved from src/layer_operations/
│   │       ├── drc_operations/    # Moved from src/drc_operations/
│   │       ├── navigation/        # Moved from src/navigation/
│   │       └── project/           # Moved from src/project/
│   │
│   ├── kiforge-ui/                # UI components and panels
│   │   ├── Cargo.toml             # egui-based UI components
│   │   ├── src/lib.rs             # Re-exports all UI modules
│   │   └── src/
│   │       ├── panels/            # All panel types
│   │       ├── tabs/              # Tab management
│   │       ├── widgets/           # Custom widgets
│   │       └── themes/            # UI theming
│   │
│   ├── kiforge-plugins/           # Plugin system
│   │   ├── Cargo.toml             # Plugin manager, WASM runtime
│   │   ├── src/lib.rs             # Plugin traits and API
│   │   └── src/
│   │       ├── manager/           # Plugin loading and management
│   │       ├── api/               # Plugin API definitions
│   │       ├── wasm/              # WASM runtime
│   │       └── registry/          # Plugin discovery and metadata
│   │
│   ├── kiforge-export/            # Export functionality
│   │   ├── Cargo.toml             # Export formats and processors
│   │   ├── src/lib.rs             # Export API
│   │   └── src/
│   │       ├── formats/           # Different export formats
│   │       ├── processors/        # Data processing for export
│   │       └── templates/         # Export templates
│   │
│   └── kiforge-common/            # Shared utilities
│       ├── Cargo.toml             # Common types, utilities, macros
│       ├── src/lib.rs             # Common functionality
│       └── src/
│           ├── types/             # Shared data types
│           ├── utils/             # Utility functions
│           └── macros/            # Procedural macros
│
├── kiforge/                       # Main application binary
│   ├── Cargo.toml                 # Depends on workspace crates
│   ├── src/main.rs                # Application entry point
│   └── assets/                    # Application assets
│
├── examples/                      # Workspace examples
│   ├── plugin_dev/                # Plugin development examples
│   ├── custom_export/             # Custom export format examples
│   └── api_usage/                 # KiForge API usage examples
│
├── plugins/                       # Official plugins
│   ├── thermal_analysis/          # Thermal analysis plugin
│   ├── signal_integrity/          # SI analysis plugin
│   └── custom_drc/                # Custom DRC rules plugin
│
├── docs/                          # Documentation
│   ├── architecture.md            # System architecture
│   ├── plugin_dev_guide.md        # Plugin development guide
│   ├── api_reference.md           # API documentation
│   └── migration_guide.md         # Migration from current structure
│
└── tools/                         # Development tools
    ├── plugin_generator/           # Plugin scaffold generator
    └── workspace_manager/          # Workspace management utilities
```

## Crate Dependencies

### Dependency Graph
```
kicad-ecs (foundation)
    ↑
kiforge-common
    ↑
kiforge-core → kiforge-export
    ↑              ↑
kiforge-ui ← kiforge-plugins
    ↑              ↑
    └── kiforge (main binary)
```

### kicad-ecs (Foundation Crate)
- **Purpose**: Core KiCad data structures, IPC communication, ECS components
- **Exports**: KiCad API client, PCB components, ECS systems
- **Dependencies**: bevy_ecs, protobuf, tokio, nng
- **API**: Standardized interface for KiCad data access

### kiforge-core 
- **Purpose**: Core KiForge business logic
- **Exports**: Layer management, display systems, navigation, project management
- **Dependencies**: kicad-ecs, kiforge-common, gerber_viewer, nalgebra
- **API**: High-level PCB manipulation and visualization

### kiforge-ui
- **Purpose**: UI components and panels
- **Exports**: Panels, tabs, widgets, themes
- **Dependencies**: kiforge-core, egui, egui_dock, egui_lens
- **API**: Reusable UI components for PCB applications

### kiforge-plugins
- **Purpose**: Plugin system and API
- **Exports**: Plugin manager, WASM runtime, plugin traits
- **Dependencies**: kiforge-core, wasmtime, libloading
- **API**: Plugin development framework

## Unified KiForge API Design

### Core API Structure
```rust
// kiforge-core/src/lib.rs
pub mod api {
    pub use kicad_ecs::{KiCadClient, PcbWorld, ComponentInfo, Position};
    pub use crate::{
        display::{DisplayManager, GridSettings},
        layer_operations::{LayerManager, LayerType},
        project::{ProjectManager, ProjectState},
    };
}

// Unified API for both KiCad and KiForge
pub trait PcbDataApi {
    // KiCad integration (via kicad-ecs)
    async fn connect_kicad(&mut self) -> Result<(), ApiError>;
    async fn sync_from_kicad(&mut self) -> Result<(), ApiError>;
    
    // KiForge-specific (Gerber-based)
    fn load_gerbers(&mut self, path: &Path) -> Result<(), ApiError>;
    fn get_layers(&self) -> Vec<LayerInfo>;
    
    // Common interface
    fn get_components(&self) -> Vec<ComponentData>;
    fn get_bounding_box(&self) -> BoundingBox;
    fn export_data(&self, format: ExportFormat) -> Result<Vec<u8>, ApiError>;
}
```

### Plugin API
```rust
// kiforge-plugins/src/api.rs
pub trait KiForgePluginApi {
    // Data access (unified across KiCad and Gerber)
    fn get_pcb_data(&self) -> &dyn PcbDataApi;
    fn get_components(&self) -> Vec<ComponentData>;
    fn get_layers(&self) -> Vec<LayerData>;
    
    // UI integration
    fn register_panel(&mut self, panel: Box<dyn PluginPanel>) -> Result<(), ApiError>;
    fn register_menu_item(&mut self, item: MenuItem) -> Result<(), ApiError>;
    
    // Export integration
    fn register_export_format(&mut self, format: ExportFormat) -> Result<(), ApiError>;
    
    // Event system
    fn subscribe_to_events(&mut self, handler: Box<dyn EventHandler>) -> Result<(), ApiError>;
}
```

## Migration Strategy

### Phase 1: Workspace Setup
1. Create new Cargo.toml workspace manifest
2. Move kicad-ecs into crates/kicad-ecs/
3. Create kiforge-common with shared types
4. Update import paths

### Phase 2: Crate Extraction  
1. Extract kiforge-core from current src/
2. Extract kiforge-ui from current src/ui/
3. Update main.rs to use workspace crates
4. Verify functionality

### Phase 3: Plugin System
1. Create kiforge-plugins crate
2. Implement plugin manager
3. Create plugin API traits
4. Add WASM runtime support

### Phase 4: API Unification
1. Create unified PcbDataApi
2. Implement common interface for KiCad and Gerber data
3. Update all consumers to use unified API
4. Add comprehensive documentation

## Benefits

### For Developers
- **Modular Architecture**: Clear separation of concerns
- **Reusable Components**: Core functionality as libraries
- **Plugin Ecosystem**: Extensible architecture
- **Common API**: Unified interface for KiCad and KiForge data

### For Users
- **Extensibility**: Easy plugin development
- **Consistency**: Common patterns across all functionality
- **Performance**: Optimized crates for specific purposes
- **Maintenance**: Cleaner dependency management

### For the Ecosystem
- **Foundation Crate**: kicad-ecs becomes standard for KiCad Rust integration
- **API Standardization**: Common patterns for PCB tools
- **Plugin Marketplace**: Standardized plugin interface
- **Community Growth**: Lower barrier to contribution

## Implementation Notes

### Current Integration Points
- KiForge uses kicad-ecs for BOM data via IPC
- KiForge has its own ECS for layer management
- Need to unify these two ECS systems

### API Compatibility
- Maintain backward compatibility during migration
- Provide migration guide for existing integrations
- Version crates independently

### Testing Strategy
- Integration tests across crate boundaries
- Plugin API tests with sample plugins
- Performance benchmarks for each crate
- Documentation tests for API examples

This architecture would position KiForge as a comprehensive platform for PCB development while establishing kicad-ecs as the de facto standard for KiCad integration in Rust.