# librepcb-ecs

Entity Component System (ECS) architecture for LibrePCB design data.

## Status: Placeholder

This crate is currently a **placeholder** for future LibrePCB integration. It establishes the API patterns and component structure that will be used when LibrePCB provides suitable integration points.

## Overview

`librepcb-ecs` will provide an ECS-based approach to working with LibrePCB board data, enabling:
- **Real-time connection** to running LibrePCB instances  
- Live updates as you edit your PCB in LibrePCB
- Flexible component queries and filtering
- Extensible analysis systems
- Integration with the KiForge workspace

## Planned Integration Methods

LibrePCB integration options being considered:
1. **File-based integration** - Parse LibrePCB project files directly
2. **API integration** - If/when LibrePCB provides an API similar to KiCad's
3. **Plugin integration** - LibrePCB plugin that exposes data
4. **Import/Export** - Convert LibrePCB data to standardized formats

## Architecture

The crate follows the same patterns as `kicad-ecs`:

### ECS Mapping
- **LibrePCB Component** → **ECS Entity**
- **Component Properties** → **ECS Components**:
  - `LibrePcbComponentId` - Unique identifier  
  - `LibrePcbComponentInfo` - Name, value, device
  - `LibrePcbPosition` - X, Y coordinates and rotation
  - `LibrePcbLayer` - Layer information
  - `LibrePcbComponentFlags` - DNP, exclude from BOM, etc.

## Quick Start (Placeholder)

```rust
use librepcb_ecs::*;

fn main() -> Result<()> {
    // Create ECS world for LibrePCB data
    let mut pcb_world = LibrePcbWorld::new();
    
    // This will work when LibrePCB integration is implemented
    // pcb_world.connect_to_librepcb()?;
    // pcb_world.load_project("path/to/project.lppz")?;
    
    // Query components using ECS
    let components = pcb_world.get_components();
    for (id, info, pos) in components {
        println!("{} at ({:.1}, {:.1})mm", info.name, pos.x, pos.y);
    }
    
    Ok(())
}
```

## Comparison with kicad-ecs

| Feature | kicad-ecs | librepcb-ecs |
|---------|-----------|--------------|
| **Status** | ✅ Working | 🔄 Placeholder |
| **API Integration** | ✅ IPC Socket | 🔄 TBD |
| **File Integration** | ❌ | 🔄 Planned |
| **Component ECS** | ✅ | 🔄 Placeholder |
| **Live Updates** | ✅ | 🔄 Planned |

## Examples

```bash
# Run the basic placeholder example
cargo run --example basic -p librepcb-ecs
```

## Contributing

As LibrePCB evolves and provides integration opportunities, this crate will be updated. Contributions are welcome for:
- LibrePCB file format parsing
- Integration research and prototyping
- API design improvements
- Documentation and examples

## License

Licensed under either of:
- Apache License, Version 2.0 ([LICENSE-APACHE](../../LICENSE-APACHE))
- MIT license ([LICENSE-MIT](../../LICENSE-MIT))

at your option.