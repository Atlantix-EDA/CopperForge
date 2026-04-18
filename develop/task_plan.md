# CopperForge — Task Plan

## Completed

- [x] Migrate to egui-citizen architecture (Phases 0-7)
- [x] Replace bevy_ecs with LayerStore (plain Vec<PcbLayer>)
- [x] Replace kicad-ecs IPC with kiparse file parsing
- [x] Remove orphan `crates/kicad-ecs/` directory (LayerStore + kiparse cover its role)
- [x] Remove orphan `src/ui/bom_panel.rs` (superseded by `bom_panel_v2.rs`)
- [x] Tokyo Night Storm theme
- [x] KiCad 10 detection (PATH, Flatpak, Snap)
- [x] Remove dead dependencies (notify, rfd, local-ip, egui_mobius, futures)
- [x] KiCad 10 gerber filename detection (--no-protel-ext naming)
- [x] Natural sort on reference designators (`bom/mod.rs::natural_sort_key`)

## Phase 3: App Lifecycle FSM + SharedServices activation

Infrastructure that should have been in place before panels started depending
on implicit initialization ordering. Do this *before* Project Panel Consolidation
so the new panel can assume a `Ready` state with fully-populated services.

- [ ] Define `AppLifecycle` enum: `Cold → LoadingConfig → DiscoveringKiCad → InitializingDb → Ready { services: SharedServices }`
- [ ] Run each phase explicitly in `CopperForgeApp::new()` (or spread across early frames if any step is slow) — no more lazy first-frame side effects
- [ ] Cached facts become first-class outputs of their phase:
      - `kicad_version` + `kicad_cli_method` from `DiscoveringKiCad`
      - `ProjectConfig` + `ProjectManager` from `LoadingConfig`
      - `ProjectDatabase` handle from `InitializingDb`
- [ ] Panels only render when `AppLifecycle::Ready { services }`; earlier phases show a splash / progress view
- [ ] Activate `SharedServices`: panels switch from `&mut CopperForgeApp` to `&mut SharedServices`. Collapse the god-struct over time.
- [ ] Retire ad-hoc first-frame init: `if self.kicad_version.is_none() { detect_and_cache_kicad() }` and equivalents go away — the fact is set before any frame renders.

## Next: Project Panel Consolidation

Merge the **Project**, **PCB File**, and **Project Database** tabs into a single
Projects panel. The Project Database tree becomes the primary surface; per-project
actions move onto a right-click context menu, with modal dialogs for create/edit.

### Context menu (right-click on a project row in the tree)
- [ ] `New Project` — opens empty create-modal
- [ ] `Open Project` — loads project, populates PCB path + current state
- [ ] `Update Project` — opens edit-modal prefilled with current metadata
- [ ] `Delete Project` — confirm dialog, then remove from sled db
- [ ] `Generate Gerbers` — invoke current PCB-file → gerbers flow from selected project
- [ ] `Show in File Manager` / `Reveal PCB path` (nice-to-have)

### Modal dialog
- [ ] Fields: Name, Description, Author, PCB path (file picker), Tags, Version
- [ ] `Save` persists through `ProjectDatabase::save_project` and closes
- [ ] `Cancel` closes with no write
- [ ] On create: generate UUID, set `created_at = last_modified = now()`
- [ ] On update: only bump `last_modified`, leave `created_at` untouched
- [ ] Validate: non-empty name, PCB path exists (warn, don't block)

### Bugs to fix as part of this rework
- [ ] Description and metadata changes must persist on save (currently lost)
- [ ] `created_at` / `last_modified` timestamps are inverted (created > revised)
- [ ] PCB file selection and gerber generation must be accessible from project context

### Cleanup after consolidation
- [ ] Remove the separate **Project** tab (`ui/project_panel.rs`)
- [ ] Remove the separate **PCB File** tab (`ui/pcb_file_panel.rs`)
- [ ] Collapse `ui/projects_panel.rs` + `ui/project_manager_panel.rs` into one
- [ ] Update `panels/project.rs`, `panels/pcb_file.rs`, `panels/projects.rs` citizen wiring accordingly
- [ ] Ref: consolidates GitHub issues #3, #4

## BOM Panel Rewrite

Model on Stencil implementation (stencil-ide/backend/src/bom.rs).

### BOM Table Panel (ref: GitHub issue #6)
- [ ] Group by part type (same value+footprint = one row with quantity)
- [ ] Reconcile column headers: decide on final set
      — proposed: Item, Ref Des, Value, Description, Footprint, Library, X, Y, Layer
      — current v2: #, Ref, Value, Description, Footprint, X, Y, Layer (no Library)
- [ ] Column toggle checkboxes (show/hide each column)
- [ ] CSV export
- [ ] Filter by text, component type
- [ ] Click-to-sort columns (reference, value, footprint, x, y)
- [ ] Evaluate: sort on `egui_extras::TableBuilder` vs `egui_deferred_table` dependency

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

### UI / Theme
- [ ] Font scale control (like saturn-grid-sim — theme::apply_font_scale with % slider)
- [ ] Separate font size control for top ribbon bar (independent of global scale)
- [ ] Disable gerber view cursor tracking when modal windows are open (About, KiCad info)

### New Panels
- [ ] Shell panel
- [ ] Terminal panel
- [ ] Logger panel (replace event log)

### Incremental Migration
- [ ] Move panel rendering from `ui/*.rs` into `panels/*.rs` citizen structs
      (today every `panels/<x>.rs::show()` just forwards to a `ui::show_*` free function)
- [ ] Switch to SharedServices pattern (panels take `&mut SharedServices`)
- [ ] Remove old `ui/` free functions once all panels are migrated
