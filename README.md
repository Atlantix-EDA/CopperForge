<div align="center">
<img width=280 height=260 src="./assets/media/KiForgeLogo.png"></img>

## *A Modern Hybrid PCB Design Platform*

[![egui_version](https://img.shields.io/badge/egui-0.31.1-blue)](https://github.com/emilk/egui)
[![KiCad Version](https://img.shields.io/badge/KiCad-9.0+-blue)](https://www.kicad.org/)
[![MSRV](https://img.shields.io/badge/MSRV-1.65.0-blue)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

KiForge is designed to support all phases of design optimization beginning with the PCB design flow and continuing to CAM and manfuacturing. It's considering a modern software EDA tool - built with Rust, egui, and associated crates, it leverages the fast memory management, memory safety, and fast rendering operates that the Rust ecosystem provides. 

Features include PCB design rule checking and optimizations, real time bill of materials, component placement information, and CAM operations for manufacturing. 

Designers and engineers can employ KiForge to be a **companion design tool to KiCad**, running it in parallel to KiCad PCB to obtain informatin that they during PCB design as well as packaging the design for manufacturing. 

This software application takes from various Rust based projects -- the `MakerPnP` project with `gerber_types` and `gerber_parser`, and the `gerber_viewer`. It takes from the `egui_mobius` software stack - `egui_mobius_reactive` and `egui_lens` to support the integrated event logger and reactive state management. 

What will make KiForge different from other applications is the use of algorithms for PCB manufacturing optimization, all in a memory safe multi-threaded environment. 

Shown below is a the loading of a PCB design with over 400+ components,
where the KiCad pcb design is loaded and the gerbers are generated 
within the tool and the display is updated. 

![](https://github.com/saturn77/KiForge/blob/master/assets/media/KiForge_usage.gif)

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

- [ ] Direct KiCad pcb file import (done)
- [ ] Real time BOM display 
- [ ] Manfuacutring Cost Estimate 
- [ ] Identification of library issues, such as lack of 3D symbols
- [ ] Enhanced DRC with customizable rule sets - algorithmic approaches


## Getting Started

```bash
git clone https://www.github.com/saturn77/KiForge.git
cd KiForge
cargo run
```

## License

See LICENSE file for details.  
