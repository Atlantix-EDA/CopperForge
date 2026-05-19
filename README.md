<div align="center">
<img width="420" src="./assets/media/copperforge-banner.png" alt="CopperForge"></img>

# CopperForge

Companion PCB Release & Manufacturing Tool for KiCad.

[![egui](https://img.shields.io/badge/egui-0.33-blue)](https://github.com/emilk/egui)
[![KiCad](https://img.shields.io/badge/KiCad-10-blue)](https://www.kicad.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

</div>

![CopperForge in action](assets/media/citizen-copper.gif)

## What it does

CopperForge sits alongside KiCad and owns the manufacturing-backend
workflow: taking a finished PCB, generating fabrication outputs, viewing
the board in 3D, tagging a release, and tracking every revision you send
to a fab.

- **Release packaging** — one click cuts `<project>/outputs/<rev>/<name>_<rev>[_<date>].zip`
  containing gerbers, drill files, fabrication data, and a Markdown
  `RELEASE_NOTES.md` (KiCad version, host OS, git commit, description,
  user changes). Right-click a rev in the Projects tree → Regenerate to
  overwrite in place.
- **Gerber processing** — generate via `kicad-cli` (auto-detected on
  PATH / Flatpak / Snap), load, inspect. Live stale-file warning when
  the PCB is modified on disk.
- **3D gerber viewer** — an embedded OpenGL view rendering the
  gerber-driven board outline, F.Cu / B.Cu copper, and a translucent
  F.Mask / B.Mask soldermask. Handles 2- and 4+ layer stacks with
  correct stack-position layer numbering.
- **BOM & fabrication data** — parse `.kicad_pcb` files directly via
  `kiparse` (no live IPC to KiCad needed); export a grouped BOM to CSV
  and XLSX — enriched with Manufacturer / MPN from your KiCad symbol
  libraries — plus a PCBWay / JLCPCB-style centroid (CPL) file.
- **Design rule checks** — a DRC panel that inspects copper geometry,
  including corner-rounding overlays, across every layer of the board.
- **Project database** — embedded [redb](https://github.com/cberner/redb)
  store tracks imported projects, BOM snapshots, and release history.
- **Shell / Terminal / Logger panels** — in-app command shell, a bash
  terminal, and a structured event log.

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
- 3D gerber viewer (board outline, copper, and soldermask; 2- and 4-layer stacks)
- BOM & centroid export (CSV / XLSX with symbol-library enrichment, CPL centroid)
- Projects tab rework + Project Edit modal
- Shell / Terminal / Logger panels
- AppLifecycle with explicit init + cached kicad-cli discovery

In flight / planned:

- 3D drill-hole and silkscreen rendering
- Vendor packaging (PCBWay, Sierra Proto Express, JLCPCB specifics)
- DRC algorithm enhancements
- Multi-rev diff view (outputs/rev_01 vs rev_02)

## Architecture

CopperForge is a native [egui](https://github.com/emilk/egui)
application built on the
[`egui_mobius`](https://github.com/saturn77/egui_mobius) framework — the
`egui_citizen` pattern for panel lifecycle, layered on
`egui_mobius_reactive`'s `Dynamic<T>` / `Derived<T>` reactive
primitives. It serves as a real-world reference implementation of the
citizen pattern.

| Category | Crates |
|----------|--------|
| UI | egui 0.33, eframe 0.33 (glow-only, no accesskit), egui_dock 0.18 |
| Citizen pattern | egui_citizen, egui_mobius_reactive |
| Gerber handling | gerber_viewer 0.5, gerber_parser 0.4, gerber-types 0.7 |
| BOM parsing | kiparse (Atlantix-EDA/atlantix-eda) |
| Storage | redb (project database, single file) |
| Release / export | zip (deflate-only), rust_xlsxwriter (BOM XLSX) |

## Credits

- The 3D viewer is adapted from
  [alumina-interface](https://github.com/timschmidt/alumina-interface) by
  Timothy Schmidt (MIT) — its OpenGL renderer is the foundation CopperForge's
  `render3d` module is built on.
- Gerber rendering builds on
  [gerber-viewer](https://github.com/MakerPnP/gerber-viewer) from the
  MakerPnP project.

## License

MIT
