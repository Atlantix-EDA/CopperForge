<div align="center">
<img width=280 height=260 src="./assets/media/ForgeCopper.png"></img>

# CopperForge

Companion PCB Release & Manufacturing Tool for KiCad.

[![egui](https://img.shields.io/badge/egui-0.33-blue)](https://github.com/emilk/egui)
[![KiCad](https://img.shields.io/badge/KiCad-10-blue)](https://www.kicad.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

## What it does

CopperForge sits alongside KiCad and owns the manufacturing-backend
workflow: taking a finished PCB, generating fabrication outputs, tagging
a release, and tracking every revision you send to a fab.

![alt text](assets/media/KiForge_usage.gif)

- **Release packaging** — one click cuts `<project>/outputs/<rev>/<name>_<rev>[_<date>].zip`
  containing gerbers, drill files, and a Markdown `RELEASE_NOTES.md`
  (KiCad version, host OS, git commit, description, user changes).
  Right-click a rev in the Projects tree → Regenerate to overwrite in place.
- **Gerber processing** — generate via `kicad-cli` (auto-detected on
  PATH / Flatpak / Snap), load, inspect. Live stale-file warning when
  the PCB is modified on disk.
- **BOM extraction** — parse `.kicad_pcb` files directly via `kiparse`
  (no live IPC to KiCad needed).
- **Project database** — embedded [redb](https://github.com/cberner/redb)
  store tracks imported projects, BOM snapshots, and release history.
  Projects tab shows a tree: project → schematic (with sub-sheets) →
  pcb → `outputs/rev_NN`. Double-click loads; right-click opens a
  modal for edit/delete.
- **DRC overlay** — basic design-rule-check visualization on loaded
  gerber layers.
- **Shell / Terminal / Logger panels** — in-app command shell
  (`help`, `ver`, `info`, `status`, `env`, `new-project <name>`, OS passthrough
  via `sh <cmd>` or `!<cmd>`), a bash terminal, and a structured event log.

Built with Rust + [egui](https://github.com/emilk/egui). Uses the
[egui-citizen](https://github.com/saturn77/egui-citizen) framework for
panel lifecycle (an evolution of egui-mobius).

## Architecture

```
CopperForgeApp
  +-- Dispatcher          (citizen lifecycle, flip-flop activation)
  +-- SharedServices      (single source of truth for cross-panel state)
  |     +-- project_state: Dynamic<ProjectState>  (reactive driver)
  |     +-- layer_store, display_manager, drc_manager, ...
  |     +-- project_db: redb handle
  |     +-- kicad_cli_method  (cached at startup; no re-probing)
  +-- Persisted citizen panels (Bom, Terminal, Shell, Logger, ...)
```

Explicit init sequence in `new()`: **LoadConfig → DiscoverKiCad →
InitializeDb → wire SharedServices → register citizens**. Panics at any
stage print a stage-aware diagnostic with actionable hints.

### Dependencies

| Category | Crates |
|----------|--------|
| UI | egui 0.33, eframe 0.33 (glow-only, no accesskit), egui_dock 0.18 |
| Citizen pattern | egui_citizen, egui_mobius_reactive |
| Gerber handling | gerber_viewer 0.5, gerber_parser 0.4, gerber-types 0.7 |
| BOM parsing | kiparse (Atlantix-EDA/atlantix-eda) |
| Storage | redb (project database, single file) |
| Release archives | zip (deflate-only) |

## KiCad 10 support

CopperForge detects KiCad installed via PATH, Flatpak, or Snap. Discovery
runs once at startup; subsequent gerber/drill operations reuse the
cached method without re-probing (Flatpak cold-start is ~1–3 s, so this
matters). Gerber filename detection supports KiCad 10's `--no-protel-ext`
naming convention (`Top Layer.gbr`, `Bottom Solder.gbr`, etc.) alongside
traditional KiCad and Protel patterns.

## Building

Requires Rust 1.88+.

```bash
git clone https://github.com/Atlantix-EDA/CopperForge.git
cd CopperForge
cargo run
```

## Status

Active development. Shipped:

- Release workflow (zip + markdown notes + per-rev DB tracking + regenerate)
- Projects tab rework + Project Edit modal
- Shell / Terminal / Logger panels
- AppLifecycle with explicit init + cached kicad-cli discovery

In flight / planned:

- Vendor packaging (PCBWay, Sierra Proto Express, JLCPCB specifics)
- DRC algorithm enhancements
- Multi-rev diff view (outputs/rev_01 vs rev_02)

## License

MIT
