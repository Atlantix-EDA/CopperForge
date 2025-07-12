# KiForge Workspace Crates Structure

## Implementation Plan

### Step 1: Create kiforge-common
```rust
// crates/kiforge-common/Cargo.toml
[package]
name = "kiforge-common"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
nalgebra = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }

// crates/kiforge-common/src/lib.rs
pub mod types;
pub mod error;
pub mod utils;

pub use types::*;
pub use error::*;
```

### Step 2: Create kiforge-core
```rust
// crates/kiforge-core/Cargo.toml  
[package]
name = "kiforge-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
kiforge-common = { workspace = true }
kicad-ecs = { workspace = true }
bevy_ecs = { workspace = true }
gerber_viewer = { workspace = true }
nalgebra = { workspace = true }
serde = { workspace = true }

// crates/kiforge-core/src/lib.rs
pub mod display;
pub mod layer_operations;  
pub mod drc_operations;
pub mod navigation;
pub mod project;

pub mod api {
    // Unified API exports
    pub use kicad_ecs::{KiCadClient, PcbWorld};
    pub use crate::{
        display::{DisplayManager, GridSettings},
        layer_operations::{LayerManager, LayerType},
        project::{ProjectManager, ProjectState},
    };
}
```

### Step 3: Create kiforge-ui
```rust
// crates/kiforge-ui/Cargo.toml
[package]
name = "kiforge-ui"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
kiforge-common = { workspace = true }
kiforge-core = { workspace = true }
egui = { workspace = true }
egui_dock = { workspace = true }
egui_lens = { workspace = true }

// crates/kiforge-ui/src/lib.rs
pub mod panels;
pub mod tabs;
pub mod widgets;
pub mod themes;

pub use panels::*;
pub use tabs::*;
```

### Step 4: Move kicad-ecs
```bash
# Move the existing kicad-ecs into workspace
mv ../kicad-ecs crates/kicad-ecs
```

### Step 5: Update main kiforge binary
```rust
// kiforge/Cargo.toml
[package]
name = "kiforge"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
kiforge-core = { workspace = true }
kiforge-ui = { workspace = true }
kiforge-plugins = { workspace = true }
eframe = { workspace = true }
tokio = { workspace = true }

// kiforge/src/main.rs
use kiforge_core::api::*;
use kiforge_ui::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Simplified main using workspace crates
    let app = KiForgeApp::new();
    app.run().await
}
```

## Migration Commands

### Create Directory Structure
```bash
cd /home/james/raid_one/software_projects/atlantix/Egui/KiForge

# Create crates directory
mkdir -p crates/{kicad-ecs,kiforge-common,kiforge-core,kiforge-ui,kiforge-plugins,kiforge-export}

# Create main binary directory  
mkdir -p kiforge/{src,assets}

# Create supporting directories
mkdir -p {examples,docs,tools,plugins}
```

### Move kicad-ecs
```bash
# Move kicad-ecs into workspace
mv ../kicad-ecs crates/kicad-ecs
```

### Create Initial Crate Files
```bash
# Create basic Cargo.toml files for each crate
for crate in kiforge-common kiforge-core kiforge-ui kiforge-plugins kiforge-export; do
    cat > crates/$crate/Cargo.toml << EOF
[package]
name = "$crate"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
EOF
    mkdir -p crates/$crate/src
    echo "// TODO: Implement $crate" > crates/$crate/src/lib.rs
done
```

### Migrate Existing Code
```bash
# Move current source to kiforge-core (temporary)
cp -r src/* crates/kiforge-core/src/

# Update main binary
cat > kiforge/Cargo.toml << EOF
[package]
name = "kiforge"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "kiforge"
path = "src/main.rs"

[dependencies]
kiforge-core = { workspace = true }
eframe = { workspace = true }
tokio = { workspace = true }
EOF

# Create new main.rs that delegates to workspace crates
cat > kiforge/src/main.rs << EOF
// KiForge main binary - delegates to workspace crates
use kiforge_core::*;

fn main() -> eframe::Result<()> {
    // Delegate to existing main logic for now
    kiforge_core::main()
}
EOF
```

## Testing Migration

### Verify Workspace
```bash
# Test workspace structure
cargo check --workspace

# Test individual crates
cargo check -p kicad-ecs
cargo check -p kiforge-core
cargo check -p kiforge-ui
```

### Build and Run
```bash
# Build workspace
cargo build --workspace

# Run main binary
cargo run --bin kiforge
```

## Benefits of This Structure

### Development Benefits
- **Modular Development**: Work on individual crates independently
- **Clear Dependencies**: Explicit dependency graph
- **Reusable Components**: Other projects can use individual crates
- **Testing**: Test crates in isolation

### API Benefits  
- **Unified Interface**: Common API across KiCad and Gerber data
- **Plugin System**: Standardized plugin development
- **Extensibility**: Easy to add new functionality
- **Documentation**: Clear API boundaries

### Maintenance Benefits
- **Version Management**: Workspace-level dependency management
- **Code Organization**: Clear separation of concerns
- **CI/CD**: Build and test individual components
- **Publishing**: Publish useful crates to crates.io

This structure positions KiForge as a comprehensive platform while making kicad-ecs the foundation for Rust-based KiCad integration.