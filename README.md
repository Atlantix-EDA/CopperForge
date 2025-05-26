# KiForge

A manufacturing support platform for KiCad PCB designs, focused on streamlining the production workflow from design to fabrication. 

## Overview

KiForge is designed to bridge the gap between KiCad PCB designs and manufacturing processes. It provides essential tools for validating designs, preparing production files, and optimizing panel layouts for cost-effective manufacturing.

This software application takes from various Rust based projects -- the `MakerPnP` project with `gerber_types` and `gerber_parser`, and the `gerber_viewer`. It takes from the `egui_mobius` software stack - `egui_mobius_reactive` and `egui_lens` to support the integrated event logger and reactive state management. 

What will make KiForge different from other applications is the use of algorithms for PCB manufacturing optimization, all in a memory safe multi-threaded environment. 

## Key Features

- **Gerber Import & Visualization**: Import and view multi-layer Gerber files with support for all standard PCB layers
- **Design Rule Checking (DRC)**: Run comprehensive manufacturing checks to catch issues before production
- **Panelization Support**: Optimize PCB panel layouts to maximize material usage and reduce manufacturing costs
- **Component Data Import** *(Work in Progress)*: Import and visualize PCB part data for assembly preparation

## Current Capabilities

- Multi-layer Gerber viewing with support for:
  - Copper layers (Top/Bottom)
  - Silkscreen layers
  - Solder mask layers
  - Solder paste layers
  - Board outline
  - Drill files
- Interactive layer controls with visibility toggles
- Grid overlay for measurement and alignment
- Zoom and pan controls for detailed inspection

## Roadmap

- [ ] Enhanced DRC with customizable rule sets
- [ ] Advanced panelization algorithms
- [ ] BOM integration and component visualization
- [ ] Pick-and-place file generation
- [ ] Manufacturing cost estimation
- [ ] Direct KiCad project import

## Getting Started

```bash
git clone https://www.github.com/saturn77/KiForge.git
cd KiForge
cargo run
```

## License

See LICENSE file for details.  
