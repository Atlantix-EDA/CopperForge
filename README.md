<div align="center">
<img width=280 height=260 src="./assets/media/ForgeCopper.png"></img>

## *Professional PCB Design Workflow Platform for KiCad*

[![egui_version](https://img.shields.io/badge/egui-0.31.1-blue)](https://github.com/emilk/egui)
[![KiCad Version](https://img.shields.io/badge/KiCad-9.0+-blue)](https://www.kicad.org/)
[![MSRV](https://img.shields.io/badge/MSRV-1.65.0-blue)](https://blog.rust-lang.org/2022/11/03/Rust-1.65.0.html)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

**CopperForge** is a comprehensive desktop application that transforms KiCad into a professional-grade PCB design platform. It bridges the gap between schematic capture and manufacturing by providing integrated project management, curated component libraries, real-time BOM analysis, and advanced visualization—all in a modern, memory-safe Rust application.

> **The Vision:** Altium Designer workflow efficiency with KiCad's open-source freedom.

## Why CopperForge?

Traditional KiCad workflows involve tedious manual library management, disconnected tools for BOM generation, and separate gerber viewers. CopperForge eliminates these pain points by providing:

- ✅ **Turnkey Project Creation** - New KiCad projects configured in seconds
- ✅ **Curated Component Libraries** - E96 resistors (0402-2512) + extensive IC library pre-installed
- ✅ **One-Command Component Import** - Download from Digi-Key/Mouser → import in 30 seconds
- ✅ **Real-time BOM** - Live bill of materials while you design
- ✅ **Integrated Gerber Preview** - Manufacturing output visualization without leaving your workflow
- ✅ **Professional Project Management** - Version tracking, metadata, and organization

## The CopperForge Ecosystem

CopperForge integrates multiple specialized repositories into a cohesive platform:

```
┌─────────────────────────────────────────────────────────────┐
│                      CopperForge Core                        │
│         Project Management • KiCad Integration               │
│              Workflow Automation • UI Framework              │
└──────────────┬──────────────────────────────┬───────────────┘
               │                              │
    ┌──────────▼──────────┐        ┌─────────▼──────────┐
    │  Component Libraries │        │  Analysis & Viz    │
    │  • atlantix-eda      │        │  • gerber_parser   │
    │  • kiverse           │        │  • gerber_viewer   │
    │  • Import Pipeline   │        │  • BOM extraction  │
    └──────────────────────┘        └────────────────────┘
```

### Component Libraries
- **[atlantix-eda](https://github.com/Atlantix-EDA/atlantix-eda)**: Precision E96 resistor library (0402, 0603, 0805, 1206, 1210, 2512 packages)
- **[kiverse](https://github.com/Atlantix-EDA/kiverse)**: Comprehensive IC component library with automated import tooling

### Visualization & Analysis
- **gerber_types**, **gerber_parser**, **gerber_viewer**: Multi-layer manufacturing visualization (from MakerPnP project)
- **egui_mobius** + **egui_lens**: Reactive state management and modern UI framework
- **kicad-ecs**: IPC communication for real-time KiCad integration

## Killer Feature: Automated Component Import

The component import pipeline is a **massive productivity multiplier**. Traditional workflow vs. CopperForge:

**Traditional Way (20-40 minutes per component):**
1. Download component files
2. Manually extract symbol from library file
3. Hand-edit footprint
4. Create library if it doesn't exist
5. Update library tables
6. Test and verify
7. Repeat for every component...

**CopperForge Way (30 seconds):**
```bash
cd ~/.kicad_libs/kiverse
python3 import_component.py ADUM360N0BRQZ-RL7.zip
```

Done. The component is now in your global library, available in **all** KiCad projects.

### What Gets Imported Automatically:
- ✓ Schematic symbol (extracted and merged)
- ✓ PCB footprint (with proper naming)
- ✓ Footprint references (automatically linked)
- ✓ Added to global library tables
- ✓ Available immediately in all projects

### Supported Sources:
- SamacSys zip files
- Digi-Key component downloads
- Mouser component packages
- Any KiCad-format component archive

**See it in action:**

![Auto Import](./assets/auto_import.png)

*One command, instant results. The component is extracted, merged into the library, and ready to use.*

## Key Capabilities

### 1. Smart Project Creation
Create production-ready KiCad projects with:
- Pre-configured symbol and footprint libraries
- Global library management (works with **all** your projects)
- Project metadata and version tracking
- Git-friendly structure
- Professional README generation

### 2. Real-time Bill of Materials (BOM)

Live IPC integration with KiCad provides manufacturing-grade BOM data:

- **Live Updates**: Component data syncs as you design
- **Comprehensive Data**: Reference, value, footprint, description, coordinates
- **Pick-and-Place Ready**: X/Y position and rotation for assembly
- **Advanced Filtering**: Search by any field
- **Export Options**: CSV for procurement and assembly
- **Unit Flexibility**: Millimeters or mils
- **Thread-safe Architecture**: Built with signal/slot pattern for responsive UI

**Usage:**
1. Open your PCB in KiCad
2. Launch CopperForge → BOM tab
3. Click "Connect" for live component data
4. Export for manufacturing or procurement

### 3. Manufacturing Visualization

Multi-layer Gerber rendering with:
- Interactive zoom and pan
- Layer toggles and isolation
- Measurement tools
- Component coordinate display
- Manufacturing-ready output verification

### 4. Professional Workflow

- Project database with search and filtering
- Version tracking and metadata
- Tag-based organization
- Recent projects quick access
- Git integration ready

## Demo: Complete Workflow

![CopperForge Demo](./assets/media/KiForge_usage.gif)

**What you're seeing:**
- PCB design with 400+ components loaded
- Gerbers generated in-tool
- Real-time visualization
- Smooth, responsive UI at 60 FPS

## Requirements

- **Rust**: 1.65.0 or higher ([rustup.rs](https://rustup.rs/))
- **KiCad**: 9.0+ (for PCB integration)
- **Operating System**: Linux, macOS, or Windows

## Getting Started

### Installation

```bash
# 1. Clone the repository
git clone https://github.com/Atlantix-EDA/CopperForge.git
cd CopperForge

# 2. Build and run
cargo run --release
```

### Quick Start Guide

**Create Your First Project:**
1. Launch CopperForge
2. Go to Projects tab → "Create New Project"
3. Enter project details and choose libraries (atlantix-eda + kiverse recommended)
4. Click "Create" - your KiCad project is ready with all libraries configured

**Import a Component:**
1. Download component zip from Digi-Key or SamacSys
2. Run: `python3 ~/.kicad_libs/kiverse/import_component.py component.zip`
3. Component is now available in all your KiCad projects

**View Real-time BOM:**
1. Open your PCB in KiCad
2. CopperForge → BOM tab → "Connect"
3. See live component data as you design

**Preview Manufacturing Output:**
1. Export gerbers from KiCad
2. CopperForge → Open gerber files
3. Verify layers and manufacturing data

## Architecture

CopperForge is built with modern Rust best practices:

- **Memory Safety**: Zero-cost abstractions with Rust's ownership model
- **Multi-threading**: Parallel gerber processing and BOM updates
- **Reactive UI**: egui with signal/slot pattern for responsive experience
- **Modular Design**: Clean separation between core, UI, and integration layers
- **IPC Communication**: Real-time KiCad integration via Unix sockets

## Roadmap

CopperForge is actively developed with exciting features planned:

**Current (v0.1.x):**
- ✅ Project creation and management
- ✅ Global library configuration
- ✅ Component import pipeline
- ✅ Real-time BOM
- ✅ Gerber visualization

**Near-term (v0.2.x):**
- 🚧 Enhanced DRC (Design Rule Check)
- 🚧 LibrePCB integration
- 🚧 Advanced pick-and-place export
- 🚧 Component cost tracking

**Future:**
- 📋 Multi-user collaboration features
- 📋 Cloud library sync
- 📋 AI-powered component recommendations
- 📋 Automated panelization

See [ROADMAP.md](./ROADMAP.md) for detailed planning.

## Who Is CopperForge For?

**Hobbyists** who want professional tools without the enterprise price tag

**Freelance Designers** who need efficient workflows for client projects

**Small Hardware Teams** requiring collaboration and library management

**Educators** teaching PCB design with modern, safe tooling

**Anyone** frustrated with traditional EDA library management

## Contributing

We welcome contributions! CopperForge is built by the community, for the community.

**Ways to contribute:**
- 🐛 Report bugs and request features via GitHub Issues
- 💻 Submit pull requests (see [CONTRIBUTING.md](./CONTRIBUTING.md))
- 📚 Improve documentation
- 🎨 Design UI/UX improvements
- 📦 Add components to kiverse library
- ⭐ Star the repo and share with your network!

## Related Projects

- **[atlantix-eda](https://github.com/Atlantix-EDA/atlantix-eda)** - E96 resistor library
- **[kiverse](https://github.com/Atlantix-EDA/kiverse)** - IC component library
- **[KiCad](https://www.kicad.org/)** - The excellent open-source EDA suite we enhance

## License

MIT License - See LICENSE file for details.

---

<div align="center">

**Built with ❤️ using Rust and egui**

[⭐ Star on GitHub](https://github.com/Atlantix-EDA/CopperForge) • [📖 Documentation](./docs) • [🐛 Report Bug](https://github.com/Atlantix-EDA/CopperForge/issues)

</div>
