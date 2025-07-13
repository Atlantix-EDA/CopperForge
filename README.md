<div align="center">
<img width=280 height=260 src="./assets/media/ForgeCopper.png"></img>

## *A Modern Hybrid PCB Design Platform*

[![egui_version](https://img.shields.io/badge/egui-0.31.1-blue)](https://github.com/emilk/egui)
[![KiCad Version](https://img.shields.io/badge/KiCad-9.0+-blue)](https://www.kicad.org/)
[![MSRV](https://img.shields.io/badge/MSRV-1.65.0-blue)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

**CopperForge** is a modern EDA platform designed to fill the void between traditional PCB design tools and manufacturing preparation, facilitating design and production workflows with a focus on real-time collaboration and extensibility. Built with Rust and the `egui` framework, CopperForge provides a responsive, modular environment for PCB design, visualization, and manufacturing preparation.



**Key capabilities include:**
- Multi-layer Gerber file import and visualization
- Interactive viewing with zoom, measurement, and layer controls
- **Real-time bill of materials (BOM) with live KiCad integration**
- PCB design rule checking and optimization (in progress)
- Component placement analysis with coordinate display
- CAM operations for manufacturing preparation

CopperForge serves as a **companion tool to KiCad**, running alongside KiCad PCB to provide enhanced insights during design and streamline manufacturing preparation. 

CopperForge is also being built to handle other open source PCB design tools, such as **LibrePCB** and others, to provide a unified platform for PCB development.

The application leverages proven Rust ecosystem libraries including `gerber_types`, `gerber_parser`, and `gerber_viewer` from the `MakerPnP` project, plus the `egui_mobius` stack with `egui_lens` for reactive state management and event logging. The `kicad-ecs` library is used for IPC communication with KiCad, enabling real-time updates and interactions. This crate was formerly separated into its own repository, but has been merged into CopperForge to provide a more cohesive development experience.

What sets CopperForge apart is its focus on algorithmic PCB manufacturing optimization within a memory-safe, multi-threaded environment. The `ECS` design paradigm allows for flexible component management and real-time updates, while the `egui` framework provides a modern, responsive user interface.

Shown below is the loading of a PCB design with over 400+ components, where the KiCad PCB design is loaded, the gerbers are generated within the tool, and the display is updated. 

![CopperForge Demo](./assets/media/KiForge_usage.gif)



## Real-time BOM Integration

CopperForge features a powerful real-time Bill of Materials (BOM) system that communicates directly with a running KiCad instance through IPC (Inter-Process Communication). This provides live component data for assembly and manufacturing preparation.

### BOM Features:
- **Live KiCad Integration**: Direct communication with KiCad PCB via IPC protocol
- **Real-time Updates**: Component data refreshes automatically as you modify your design in KiCad
- **Comprehensive Component Data**: Item number, reference designator, value, footprint, and description
- **Precise Coordinates**: X/Y position and rotation angle for pick-and-place operations
- **Advanced Filtering**: Search and filter components by reference, value, footprint, or description
- **Multiple Units**: Display coordinates in either millimeters or mils
- **Connection Status**: Visual indicators for KiCad connection status
- **Auto-refresh**: Configurable refresh intervals for live updates
- **Thread-safe Architecture**: Built with `egui_mobius` signal/slot pattern for responsive UI

### Usage:
1. Open your PCB design in KiCad
2. Launch CopperForge and navigate to the BOM tab
3. Click "Connect" to establish live communication with KiCad
4. View real-time component data with filtering and search capabilities
5. Use coordinate data for pick-and-place machine programming


## Roadmap
CopperForge is an evolving platform with a focus on enhancing PCB design and manufacturing preparation. Our roadmap includes both ongoing development and planned features to expand the platform's capabilities.
Please see our [ROADMAP.md](./ROADMAP.md) for more details on current and future features.

## Requirements

- **Rust**: 1.65.0 or higher (see [rustup.rs](https://rustup.rs/) for installation)
- **KiCad**: 9.0+ (for PCB file import functionality)
- **Operating System**: Linux, macOS, or Windows

## Getting Started

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://github.com/Atlantix-EDA/CopperForge.git
   cd CopperForge
   ```

2. **Build and run:**
   ```bash
   cargo run --release
   ```

### Basic Usage

1. **Load a PCB file:** Use File → Open to load a KiCad `.kicad_pcb` file
2. **View layers:** Toggle layer visibility using the layer controls panel
3. **Real-time BOM:** Open KiCad with your PCB, then use the BOM tab to connect and view live component data
4. **Run DRC:** Access Design Rule Check from the DRC panel
5. **Adjust settings:** Configure grid, orientation, and view options

## Contributing
We welcome contributions! Please see our [CONTRIBUTING.md](./CONTRIBUTING.md) for guidelines on how to get involved.

## License

See LICENSE file for details.  