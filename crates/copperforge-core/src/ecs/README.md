# CopperForge ECS Architecture

Pure Entity Component System for PCB layer management using bevy_ecs.

**ECS** separates data (Components) from behavior (Systems), where Entities are just IDs that link components together. This allows efficient querying, parallel processing, and modular functionality - perfect for managing complex PCB layer interactions.

## Core Concepts

### Components
- **`GerberData`** - Parsed gerber layer data
  ```rust
  pub struct GerberData(pub GerberLayer);
  ```
  - Tuple struct wrapping a single `GerberLayer` from gerber-viewer containing:
    - **Traces**: Copper traces and connections
    - **Pads**: Component landing pads  
    - **Fills**: Copper pour regions
    - **Shapes**: Lines, circles, rectangles, and complex polygons
    - **Bounding box**: Spatial boundaries for the layer
- **`LayerInfo`** - Layer type, name, and source file
  ```rust
  pub struct LayerInfo {
      pub layer_type : LayerType,
      pub name       : String,
      pub file_path  : Option<PathBuf>,
  }
  ```
- **`Transform`** - Position, rotation, scale, mirroring
  - Controls how layers are positioned and oriented in the viewer (e.g., quadrant view offsets)
- **`Visibility`** - Show/hide and opacity control
  - Determines whether a layer renders and how transparent it appears
- **`RenderProperties`** - Color and z-order
  - Visual appearance settings including layer color and rendering depth order

### Resources
- **`ViewStateResource`** - Camera/viewport state
- **`ActiveLayer`** - Currently selected layer
- **`LayerAssignments`** - Gerber filename → LayerType mappings
- **`UnassignedGerbers`** - Gerbers awaiting layer assignment

### Layer Types (Multi-layer PCB Support)
```rust
pub enum LayerType {
    Copper(u8),              // 1=top, 2,3,4...=inner/bottom
    Silkscreen(Side),        // Top/Bottom
    Soldermask(Side),        // Top/Bottom  
    Paste(Side),             // Top/Bottom
    MechanicalOutline,       // Board edge
}
```

**Note**: `Copper(u8)` supports unlimited copper layers:
- `Copper(1)` - Top/outer copper layer
- `Copper(2), Copper(3), Copper(4)...` - Inner copper layers  
- `Copper(N)` - Bottom/outer copper layer (where N = total layers)

## Quick Usage

### Create a layer entity:
```rust
let entity = create_layer_entity(
    &mut world,
    LayerType::Copper(1),  // Top copper
    gerber_layer,
    None,                  // raw_gerber_data
    Some(file_path),
    true,                  // visible
);
```

### Query layers:
```rust
let mut query = world.query::<(&LayerInfo, &GerberData, &Visibility)>();
for (info, data, vis) in query.iter(&world) {
    if vis.visible && info.layer_type.is_copper() {
        // Process copper layers
    }
}
```

### Set layer visibility:
```rust
set_layer_visibility(&mut world, LayerType::Copper(1), true);
```

### Get layer by type:
```rust
if let Some(entity) = get_layer_by_type(&mut world, LayerType::Copper(1)) {
    // Work with entity
}
```

## Systems

- **`render_layers_system`** - Core rendering with z-order sorting
- **`z_order_system`** - Updates render order based on layer types
- **`coordinate_update_system`** - Syncs transforms with display manager
- **`assign_gerber_to_layer_system`** - Handles gerber file assignments

## Multi-layer Support

```rust
// Standard 4-layer PCB
let layers = LayerType::standard_4_layer();

// Custom 6-layer PCB  
let layers = LayerType::for_layer_count(6);

// Check if inner layer
if let LayerType::Copper(n) = layer_type {
    if n > 1 && n < total_layers {
        // This is an inner layer
    }
}
```