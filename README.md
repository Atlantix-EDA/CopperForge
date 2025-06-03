<div align="center">
<img width=280 height=260 src="./assets/media/KiForgeLogo.png"></img>

## *A Modern Hybrid PCB Design Platform*

[![egui_version](https://img.shields.io/badge/egui-0.31.1-blue)](https://github.com/emilk/egui)
[![KiCad Version](https://img.shields.io/badge/KiCad-9.0+-blue)](https://www.kicad.org/)
[![MSRV](https://img.shields.io/badge/MSRV-1.65.0-blue)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

KiForge is a modern EDA tool designed to optimize every phase of PCB development, from initial design through manufacturing. Built with Rust and egui, it delivers fast, memory-safe operations with real-time rendering performance.

**Key capabilities include:**
- PCB design rule checking and optimization
- Real-time bill of materials generation
- Component placement analysis
- CAM operations for manufacturing preparation

KiForge serves as a **companion tool to KiCad**, running alongside KiCad PCB to provide enhanced insights during design and streamline manufacturing preparation. 

The application leverages proven Rust ecosystem libraries including `gerber_types`, `gerber_parser`, and `gerber_viewer` from the `MakerPnP` project, plus the `egui_mobius` stack for reactive state management and event logging.

What sets KiForge apart is its focus on algorithmic PCB manufacturing optimization within a memory-safe, multi-threaded environment. 

Shown below is the loading of a PCB design with over 400+ components, where the KiCad PCB design is loaded, the gerbers are generated within the tool, and the display is updated. 

![KiForge Demo](./assets/media/KiForge_usage.gif)

## Key Features

- **Gerber Import & Visualization**: Import and view multi-layer Gerber files with support for all standard PCB layers
- **Design Rule Checking (DRC)** *(Improvements in Progress)*: Run comprehensive manufacturing checks to catch issues before production
- **Panelization Support** *(Work in Progress)*: Optimize PCB panel layouts to maximize material usage and reduce manufacturing costs
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

### Completed ✅
- Direct KiCad PCB file import
- Multi-layer Gerber visualization
- Interactive layer controls
- Basic DRC functionality

### In Development 🚧
- Real-time BOM display 
- Enhanced DRC with customizable rule sets
- Manufacturing optimization algorithms

### Planned 📋
- Manufacturing cost estimation
- Component library validation
- Advanced panelization tools
- Assembly preparation features
- Integration with external CAM tools


## Requirements

- **Rust**: 1.65.0 or higher (see [rustup.rs](https://rustup.rs/) for installation)
- **KiCad**: 9.0+ (for PCB file import functionality)
- **Operating System**: Linux, macOS, or Windows

## Getting Started

### Installation

1. **Clone the repository:**
   ```bash
   git clone https://www.github.com/saturn77/KiForge.git
   cd KiForge
   ```

2. **Build and run:**
   ```bash
   cargo run --release
   ```

### Basic Usage

1. **Load a PCB file:** Use File → Open to load a KiCad `.kicad_pcb` file
2. **View layers:** Toggle layer visibility using the layer controls panel
3. **Run DRC:** Access Design Rule Check from the DRC panel
4. **Adjust settings:** Configure grid, orientation, and view options

### Supported File Formats

- KiCad PCB files (`.kicad_pcb`)
- Gerber files (`.gbr`, `.ger`)
- Excellon drill files (`.drl`)
- Pick and place files (`.csv`)

## License

See LICENSE file for details.  
