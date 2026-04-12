<div align="center">
<img width=280 height=260 src="./assets/media/ForgeCopper.png"></img>

# CopperForge

A KiCad companion tool for project management, gerber generation and viewing, and fabrication output.

[![egui](https://img.shields.io/badge/egui-0.33-blue)](https://github.com/emilk/egui)
[![KiCad](https://img.shields.io/badge/KiCad-10-blue)](https://www.kicad.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

## What it does

The process of managing a design particularly Gerber file generation, bill of materials, and functionality such as design rule checking are 
what **Forges** a printed circuit board design into its final stage
before going for fabrication and or assembly. The name **CopperForge** was chosen carefully to capture the essence of this process.

CopperForge also enables the tracking of projects, furthering the 
idea of accessing or handling all the gerber files and release 
packages for **multiple boards** as is often the case with most
real projects.  

![alt text](assets/media/KiForge_usage.gif)
- **Project management** -- create or reference KiCad Projects.
- **Gerber processing** -- generate, load and inspect gerber files.
- **BOM extraction** -- connect to a running KiCad instance via IPC, pull the bill of materials live
- **DRC visualization** -- basic design rule check results on gerber layers

Built with Rust and [egui](https://github.com/emilk/egui). Uses the [egui-citizen](https://github.com/saturn77/egui-citizen) framework
which is an evolution of **egui-mobius.**  

## Architecture

CopperForge uses a plain `LayerStore` (a `Vec<PcbLayer>`) for gerber layer management -- no ECS, no framework overhead. Each dock panel implements the `Citizen` trait from egui-citizen, providing persistent panel identity and lifecycle state with message dispatch.

The use of egui-citizen facilitites flexible and maintainable design, and the abilitiy to evolve the software in a more accessible manner. 

```
CopperForgeApp
  +-- Dispatcher (citizen lifecycle, flip-flop activation)
  +-- LayerStore (PCB layers, rendering, gerber assignment)
  +-- SharedServices (display, DRC, project management)
  +-- Panels (citizen structs: GerberView, BOM, DRC, Projects, ...)
```

### Crate structure

| Crate | Purpose |
|-------|---------|
| `copperforge-core` | App logic, panels, layer store, domain modules |
| `kicad-ecs` | KiCad IPC client (protobuf API, BOM extraction) |

### Dependencies

| Category | Crates |
|----------|--------|
| UI | egui 0.33, eframe 0.33, egui_dock 0.18 |
| Citizen pattern | egui_citizen, egui_mobius_reactive |
| Gerber handling | gerber_viewer 0.5, gerber_parser 0.4, gerber-types 0.7 |
| KiCad IPC | kicad-ecs (protobuf, nng sockets) |
| Storage | sled (project database) |


## KiCad 10 support

CopperForge detects KiCad installed via PATH, Flatpak, or Snap. Gerber filename detection supports KiCad 10's `--no-protel-ext` naming convention (`Top Layer.gbr`, `Bottom Solder.gbr`, etc.) alongside traditional KiCad and Protel patterns.

## Building

Requires Rust 1.85+ and cmake (for kicad-ecs nng dependency).

```bash
git clone https://github.com/Atlantix-EDA/CopperForge.git
cd CopperForge
cargo run
```



## Status

CopperForge is under active development. Current work is focused on:

- Release management (tagged gerber snapshots per fabrication run)
- Vendor integration (PCBWay, Sierra Proto Express, JLCPCB gerber packaging)
- Shell and terminal panels

## License

MIT
