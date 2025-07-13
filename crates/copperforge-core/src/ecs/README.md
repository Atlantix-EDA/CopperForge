# KiForge ECS (Entity Component System) Architecture

This directory contains the ECS implementation for KiForge, built on top of Bevy ECS. The ECS architecture provides a flexible and performant way to manage gerber layers, transformations, and rendering.

## Overview

The ECS system separates data (Components) from behavior (Systems), with shared global state managed through Resources. This design allows for efficient querying, parallel processing, and modular functionality.

## Components

### Core Data Components

#### `GerberData`
- **Purpose**: Wraps the actual `GerberLayer` from gerber-viewer
- **Usage**: Contains the parsed gerber file data for rendering
- **Type**: `Component`

#### `LayerInfo`
- **Purpose**: Layer identification and metadata
- **Fields**:
  - `layer_type: LayerType` - Type of PCB layer (TopCopper, BottomCopper, etc.)
  - `name: String` - Display name for the layer
  - `file_path: Option<PathBuf>` - Source file path if loaded from file
- **Usage**: Used for layer management and UI display

### Transform Components

#### `Transform`
- **Purpose**: Primary transformation component for positioning and orientation
- **Fields**:
  - `position: VectorOffset` - X,Y position offset
  - `rotation: f32` - Rotation in radians
  - `scale: f64` - Scaling factor
  - `mirroring: MirroringSettings` - X/Y axis mirroring
  - `origin: VectorOffset` - Transform origin point
- **Usage**: Controls layer positioning in the viewer

#### `ImageTransform` *(New in v0.2.0)*
- **Purpose**: Gerber image-level transformations for legacy transformation commands
- **Fields**:
  - `transform: GerberImageTransform` - Contains legacy gerber transform data
- **Legacy Commands Supported**:
  - **MI** (Mirror Image) - Mirrors the image along X and/or Y axes
  - **SF** (Scale Factor) - Scales the image by specified X and Y factors
  - **OF** (Offset) - Translates/shifts the entire image by X,Y offset
  - **IR** (Image Rotation) - Rotates the entire image by specified angle
  - **AS** (Axis Select) - Selects coordinate system axes (rarely used)
- **Usage**: Handles deprecated Gerber transformation commands from older CAD tools
- **Breaking Change**: Added for gerber-viewer 0.2.0 compatibility

### Rendering Components

#### `Visibility`
- **Purpose**: Controls layer visibility and opacity
- **Fields**:
  - `visible: bool` - Whether the layer should be rendered
  - `opacity: f32` - Layer transparency (0.0 to 1.0)
- **Usage**: Layer display control in UI

#### `RenderProperties`
- **Purpose**: Visual appearance settings for layers
- **Fields**:
  - `color: Color32` - Primary layer color
  - `highlight_color: Option<Color32>` - Color when highlighted/selected
  - `z_order: i32` - Rendering depth order (higher values render on top)
- **Usage**: Controls how layers appear visually

### Optimization Components

#### `BoundingBoxCache`
- **Purpose**: Cached bounding box for performance optimization
- **Fields**:
  - `bounds: BoundingBox` - Pre-calculated layer bounds
- **Usage**: Avoids recalculating bounding boxes during rendering

## Systems

### Rendering Systems

#### `render_layers_system()`
- **Purpose**: Basic ECS-based layer rendering
- **Query**: `(&GerberData, &Transform, &ImageTransform, &Visibility, &RenderProperties, &LayerInfo)`
- **Functionality**:
  - Sorts layers by z-order
  - Filters visible layers
  - Creates composed transforms (render + image transforms)
  - Renders using gerber-viewer's `paint_layer()`

#### `render_layers_system_enhanced()`
- **Purpose**: Advanced rendering with linear horizontal layout support
- **Features**:
  - Linear horizontal layout mode for layer positioning
  - Mechanical outline rendering aligned with each layer
  - Enhanced transform handling with calculated offsets
  - Paste layer hiding in linear layout mode
- **Usage**: Used when `use_ecs_rendering` and linear layout are enabled

#### `execute_render_system()`
- **Purpose**: Main entry point for ECS rendering
- **Parameters**:
  - `world: &mut World` - ECS world
  - `painter: &Painter` - egui painter
  - `view_state: ViewState` - Current view transformation
  - `display_manager: &DisplayManager` - Display settings
  - `use_enhanced_rendering: bool` - Toggle enhanced features

### Transform Composition Functions

#### `create_gerber_transform_composed()`
- **Purpose**: Creates GerberTransform from ECS Transform + ImageTransform
- **Breaking Change**: Updated for gerber-viewer 0.2.0 to handle image transforms
- **Note**: Currently uses basic composition; could be enhanced for complex matrix operations

#### `create_gerber_transform_with_offset_composed()`
- **Purpose**: Creates GerberTransform with linear horizontal layout positioning
- **Usage**: Used in linear layout mode for proper layer positioning with calculated offsets

## Resources

### Global State Resources

#### `ViewStateResource`
- **Purpose**: Global view transformation state
- **Usage**: Shared camera/viewport state across systems

#### `RenderConfig`
- **Purpose**: Global rendering configuration
- **Usage**: Shared rendering settings

#### `ActiveLayer`
- **Purpose**: Currently selected/active layer
- **Type**: `LayerType`
- **Usage**: UI interaction and selection state

## Factories

### Entity Creation Patterns

#### `create_gerber_layer_entity()`
- **Purpose**: Complete layer entity creation with all components
- **Components Added**: GerberData, LayerInfo, Transform, ImageTransform, Visibility, RenderProperties, BoundingBoxCache
- **Usage**: Primary factory for creating new layer entities

#### `create_layer_from_info()`
- **Purpose**: Creates entity from legacy LayerInfo structure
- **Usage**: Migration path from old layer management

#### `create_mechanical_outline_entity()`
- **Purpose**: Specialized factory for mechanical outline layers
- **Usage**: Mechanical outline layers in linear horizontal layout

## Migration and Compatibility

### Legacy Integration

The ECS system is designed to work alongside the existing LayerManager:

- `migrate_layers_to_ecs()` - Populates ECS world from LayerManager data
- `sync_with_ecs()` - Keeps LayerManager synchronized with ECS entities
- Dual rendering paths support both legacy and ECS rendering

### gerber-viewer 0.2.0 Updates

- Added `ImageTransform` component for legacy Gerber commands
- Updated transform composition to handle both render and image transforms
- Enhanced factories to include ImageTransform by default
- Updated all rendering systems to query and use ImageTransform

## Performance Considerations

### Query Optimization
- Systems use specific component queries to minimize iteration
- Z-order sorting happens once per frame
- Visibility filtering reduces rendering overhead

### Caching
- BoundingBoxCache avoids expensive recalculations
- Transform composition is done per frame but uses efficient operations

### Parallel Processing
- Bevy ECS enables parallel system execution (future enhancement)
- Component-oriented design allows for efficient memory access patterns

## Future Enhancements

### Planned Features
- Matrix-based transform composition for complex transformations
- Parallel rendering system execution
- Enhanced image transform support for legacy Gerber files
- Component-based layer effects and filters
- Performance profiling and optimization systems

### Integration Points
- kicad-ecs component integration for PCB component analysis
- Export system integration for multi-format output
- DRC system integration for design rule checking

## Usage Examples

### Creating a Layer Entity
```rust
let entity = create_gerber_layer_entity(
    &mut world,
    LayerType::TopCopper,
    gerber_layer,
    None, // raw_gerber_data
    Some(file_path),
    true, // visible
);
```

### Rendering Layers
```rust
execute_render_system(
    &mut app.ecs_world,
    &painter,
    app.view_state,
    &app.display_manager,
    app.use_ecs_rendering,
);
```

### Querying Layers
```rust
let mut query = world.query::<(&GerberData, &Transform, &Visibility)>();
for (gerber_data, transform, visibility) in query.iter(&world) {
    // Process layer data
}
```