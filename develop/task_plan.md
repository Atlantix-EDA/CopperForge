# CopperForge — Task Plan

## Completed

- [x] Migrate to egui-citizen architecture (Phases 0-7)
- [x] Replace bevy_ecs with LayerStore (plain Vec<PcbLayer>)
- [x] Replace kicad-ecs IPC with kiparse file parsing
- [x] Tokyo Night Storm theme
- [x] KiCad 10 detection (PATH, Flatpak, Snap)
- [x] Remove dead dependencies (notify, rfd, local-ip, egui_mobius, futures)
- [x] KiCad 10 gerber filename detection (--no-protel-ext naming)

## Next: BOM Panel Rewrite

Model on Stencil implementation (stencil-ide/backend/src/bom.rs).

### BOM Table Panel
- [ ] Group by part type (same value+footprint = one row with quantity)
- [ ] Proper headers: Item, Ref Des, Part, Description, Library, X, Y, Layer
- [ ] Column toggle checkboxes (show/hide each column)
- [ ] Natural sort on reference designators
- [ ] CSV export
- [ ] Filter by text, component type

### BOM Analysis Panel (separate citizen tab with egui_plot)
- [ ] Component type distribution (bar chart: R, C, U, J counts)
- [ ] Assembly side distribution (top vs bottom)
- [ ] SMD vs THT ratio
- [ ] Manufacturability scoring (unique parts, SMD ratio, single-sided, complexity)
- [ ] Color-coded grade (A+, A, B, C, D)

## Future

### Release Management
- [ ] Tag gerber releases per fabrication run (e.g. pcbway_01June2025_release)
- [ ] Archive gerber/drill packages with metadata
- [ ] Release history per project

### Vendor Integration
- [ ] PCBWay gerber packaging
- [ ] Sierra Proto Express gerber packaging
- [ ] JLCPCB gerber packaging
- [ ] OSH Park gerber packaging
- [ ] Vendor quote request via TCP JSON to server

### Project Panel Rework
- [ ] Merge Project tab and Project Database tab into a single panel
- [ ] Right-click context menu on project rows: Create / Open / Update / Delete
- [ ] Modal dialog for project details (name, description, PCB path, author, etc.)
- [ ] Description and metadata changes must persist on save (currently lost)
- [ ] Fix created_at / last_modified timestamps (created date shows later than revised date)
- [ ] Remove the separate Project tab — all project management in one place

### UI / Theme
- [ ] Font scale control (like saturn-grid-sim — theme::apply_font_scale with % slider)
- [ ] Separate font size control for top ribbon bar (independent of global scale)

### New Panels
- [ ] Shell panel
- [ ] Terminal panel
- [ ] Logger panel (replace event log)

### Incremental Migration
- [ ] Move panel rendering from ui/*.rs into panels/*.rs citizen structs
- [ ] Switch to SharedServices pattern (panels take &mut SharedServices)
- [ ] Remove old ui/ free functions once all panels are migrated
