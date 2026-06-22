# CopperForge Citizen Refactor — Implementation Plan

**Version 0.2 — collapse phase done; next phase = SharedServices untangle.**

## Status

**Phase 1 (delegation collapse) — essentially done.** Only the *delegator*
panels had the `panels/`↔`ui/` split, and most aren't clean collapses:

| Panel | Disposition |
|-------|-------------|
| `settings` | ✅ **collapsed** (`fe1b5d9`) — render moved into the citizen |
| `view_settings` | ✅ **collapsed** (`9e8a5f5`) — render moved in; **dead duplicate `ViewSettingsPanel<'a>` removed** |
| `drc` | 🅿️ **parked — full redesign** (also carries a misplaced `pub LayerInfo` consumed by `drc_operations`) |
| `bom` | 🅿️ **parked — redesign**: the in-app panel is the *ill-formatted* BOM; the well-formatted one is the **xlsx export engine**. End-state = rebuild the panel to render the export's structured rows (one BOM, shown in-app and shipped). |
| `projects` | **leave as-is** — not a delegator but a ~1,800-line PM subsystem (`projects_panel.rs` 712 + `projects_modals.rs` 1101) with sound internal structure (modals isolated on `ProjectsPanelState`). Collapsing → an 1,800-line monster. At most: tidy into a `projects/` module dir later. |
| `gerber_view`, `gerber_view_3d`, `logger`, `terminal`, `board3d_view` | already **one-file** (rendering inline) — no collapse needed. |

**Refined principle (learned doing it):** *collapse keepers (badly **organized**); park redesign candidates (badly **designed**); never force a real subsystem into one file.*

So the remaining readability work is **structural, not collapses** — the
`SharedServices` untangle (next section), readability passes on the inline giants
(`board3d_view` 1058, `terminal` 599), and macro/naming consistency.

## The goal — readability, nothing else

Reorganize CopperForge so the code is **logical and readable**: every panel is
a self-contained citizen (its declaration *and* its behavior in one file), every
piece of state has an obvious home, and the structure mirrors **CopperMine** —
the proven, readable sibling project.

This is an **in-place reorganization on the current egui 0.33**. It is *not* a
rewrite, *not* a version bump, *not* a framework project.

**The bar is readability.** "It works" earns nothing. A file you can open and
immediately understand stays; anything that fails that test gets rewritten to
pass it — engines included.

## Why this, why now

- `src/panels/` and `src/ui/` are split with no consistent rule: some panels use
  `citizen_panel!` and delegate to a `ui/X_panel.rs` render fn; some inline 1000+
  lines; names don't match (`view_settings` → `show_layers_panel`; `bom` →
  `bom_panel_v2`). Only `projects` actually `impl DockPanel`.
- `SharedServices` is a god-object: one struct of `Dynamic<T>` (and plain) fields
  passed `&mut` into everything. No declared ownership → e.g. `project_state` had
  16 scattered writers, and a release path silently adopting a deliverable as the
  project went unseen until reverse-engineered. Undeclared shared state is the
  root of a whole class of bugs.
- **CopperMine already solved this.** `src/citizens/<name>.rs`, one citizen per
  file, and (its own words) *"state lives in the citizen, not in one monolith."*
  It's the finished blueprint; this refactor is a port to it.

## The rubric

**1. One citizen = one file.** The `citizen_panel!` declaration and the citizen's
render/update methods live together. No `panels/`↔`ui/` delegation.

**2. State has exactly four homes — classify every piece:**

| Home | Primitive | When |
|------|-----------|------|
| **Private** | citizen struct field | only this citizen reads/writes it |
| **Shared** | app-owned `Dynamic<T>` | multiple citizens genuinely touch it |
| **Derived** | `Derived<T>` | computed from shared values |
| **Backend** | `Signal`/`Slot` (or the existing `mpsc` worker pattern) | a command dispatched to a worker thread; result returns as *shared* |

`SharedServices` collapses all four into "shared." The refactor pulls each field
into its correct home: private state moves *into* its citizen; genuinely-shared
state stays app-owned but with explicit, findable ownership; backend dispatch
becomes an explicit edge instead of a hand-rolled `mpsc` per panel.

**3. Mirror CopperMine's layout:** `src/citizens/`, a central `src/citizens/ids.rs`
of `CitizenId` constants, `tabs.rs` binding each tab to its citizen.

## Non-goals (learned the hard way — keep them out)

- **No egui 0.34 bump.** Stay on 0.33. The version bump is a *separate*, later,
  isolated effort behind its own keystone (the gerber_viewer fork). Mixing it in
  is exactly what blew up.
- **No DSL.** Readability comes from *organization*, not a DSL — CopperMine proves
  it (no DSL, fully readable). The citizen-DSL is optional future ergonomics.
- **No external-citizen adoption** (`egui_lens`, `egui_3d_viewer`, `egui_quill`).
  That needs the 0.34 keystone; it's a later improvement, not part of "readable."
- **No engine rewrites** (`release/`, `export/`, `gerber_geom/`, `gerber_ops`,
  `kipanel`) *unless the engine itself is unreadable*. They move by
  reorganization, not rewrite.
- **The fab loop keeps working at every step.**

## Approach — incremental, build-green, never big-bang

One citizen at a time. `cargo build` green **and** the app runs after each step,
then commit, then the next. No sweeping change touches everything at once. State
untangling happens *per citizen*: when citizen X is colocated, classify and rehome
only the state X owns.

## Current-state inventory (the work-list)

Thin delegators (`citizen_panel!` + a call into `ui/`) — easiest, do first:

| `panels/` | delegates to | collapse target |
|-----------|--------------|-----------------|
| `bom.rs` | `ui::show_bom_panel` (`ui/bom_panel_v2.rs`) | `citizens/bom.rs` |
| `drc.rs` | `ui::show_drc_panel` | `citizens/drc.rs` |
| `settings.rs` | `ui::show_settings_panel` | `citizens/settings.rs` |
| `view_settings.rs` | `ui::show_layers_panel` | `citizens/view_settings.rs` |
| `projects.rs` (only `impl DockPanel`) | `ui::show_projects_panel` | `citizens/projects.rs` |

Inline-heavy — colocate + readability pass, do after the pattern is proven:

| `panels/` | lines | note |
|-----------|-------|------|
| `logger.rs` | 111 | (eventually → `egui_lens`, but only post-keystone) |
| `gerber_view.rs` | 27 | |
| `gerber_view_3d.rs` | 88 | |
| `terminal.rs` | 599 | the command shell |
| `board3d_view.rs` | **1058** | the glow 3D renderer; preserve the hand-tuned Z-stack verbatim |

Stays in `ui/` (genuine shared sub-components / orchestration, *not* panels):
`tabs.rs`, `projects_modals.rs`, `grid_settings.rs`, `layer_controls.rs`,
`selection.rs`, `orientation_panel.rs`, `about_panel.rs`.

## Phasing

- **Phase 0 — Pilot.** Stand up `src/citizens/` + `ids.rs`; collapse **one** thin
  delegator (`settings` or `drc`) into the clean one-file citizen shape. Build
  green, app runs, review the pattern. This sets the template for all the rest.
- **Phase 1 — Thin delegators.** Collapse the remaining macro+delegate panels
  (`bom`, `view_settings`, `projects`). Delete the now-empty `ui/X_panel.rs` files.
- **Phase 2 — Inline-heavy.** Colocate `gerber_view`, `gerber_view_3d`, `logger`,
  `terminal`, `board3d_view`, with a readability pass on each. Z-stack preserved
  verbatim in `board3d_view` (verify visually).
- **Phase 3 — `SharedServices` teardown.** By now most private state has migrated
  into citizens. Reclassify what remains: genuinely-shared values stay app-owned
  with explicit ownership; document the shared graph; make backend dispatch
  explicit. The god-object shrinks to only what is truly cross-citizen.
- **Phase 4 — Consistency sweep.** Naming; consider extending `citizen_panel!` to
  carry the render method so there's *zero* delegation; remove dead `ui/`
  helpers; align `tabs.rs` with the citizen set.

## First step

Phase 0 pilot on **`settings`** (or `drc`) — the 10-line delegators are the
lowest-risk way to establish `src/citizens/` and prove the shape before touching
anything heavy.

## Risk notes (today's lessons, written down so we don't repeat them)

- Never combine this with a version bump or any dependency change.
- Never big-bang. One citizen → build → run → commit → repeat.
- Engines move by reorganization, not rewrite — unless they fail the read test.

## The SharedServices untangle (the next phase)

`SharedServices` is ~43 fields passed `&mut` everywhere — the god-object. The
untangle classifies every field into a home and rehomes it. The key realization:
a large cluster is **gerber-view-private state that leaked into the global struct.**

### The four buckets, applied to the real fields

**Bucket 0 — Private to one citizen** (move INTO that citizen as fields):
- *Gerber-view interaction cluster (the big win):* `view_state`, `ui_state`,
  `rotation_degrees`, `needs_initial_view`, `zoom_window_start`,
  `zoom_window_dragging`, `setting_origin_mode`, `origin_has_been_set`,
  `ruler_active`, `ruler_start/end/dragging/drag_start`,
  `latched_measurement_start/end` — ~15 fields, gerber-view-only.
- `drc_manager` → DrcPanel (drc parked → defer). `bom_component_count` → BomPanel (parked).

**Bucket 1 — Genuinely shared** (stays app-owned — the real graph, leave it):
- `logger_state` + `log_colors` (9 files — everyone logs); `project_state` (6);
  `layer_store` (10 — gerber view + 3D + drc, the viewer data plane);
  `bom_state` (BomPanel↔ProjectsPanel); `display_manager`, `global_units_mils`;
  `user_timezone`, `use_24_hour_clock` (settings panel ↔ ribbon clock).

**Bucket 2 — Derived** (compute from shared; future `Derived<T>`):
- `board_outline`, `top_copper`, `bottom_copper`, `top_mask`, `bottom_mask`,
  `drill` — all recomputed from `layer_store` on load; `board_geometry_gen` is
  the manual recompute signal.

**Bucket 3 — Backend dispatch** (`Signal`/`Slot`):
- `cuforge_status` — written by the background health-poller; its result.
  (Gerber generation's hand-rolled `mpsc` worker is the prototype of this bucket.)

**Not reactive state — init config/handles** (split into a `Config`/`Platform`
grouping, not mixed with UI state): `config`, `config_path`, `kicad_version`,
`kicad_cli_method`, `project_db`.

**App-shell, not a citizen** — modal flags: `show_about_modal`,
`show_kicad_version_modal`, `show_cuforge_services_modal`.

### Highest-value first target

The **gerber-view interaction cluster** (Bucket 0, ~15 fields): one real owner,
the biggest chunk of the monolith, and moving it makes the gerber view
self-contained. It's bound up with collapsing the gerber-view rendering
(`render_gerber_view` in `tabs.rs`) into a stateful `GerberViewPanel` citizen —
do them together: the citizen owns the interaction state + the render body.

### Incremental method (non-negotiable)

Per field or tight cluster, never the whole struct at once:
1. `grep` every reader/writer of `services.<field>` — confirm the real owner.
2. Single-owner → move into that citizen as a private field; fix the (contained) sites.
3. Genuinely shared → leave it.
4. `cargo build` green + app runs + **commit**. Then the next field/cluster.

Start with the safest single-owner sub-cluster (ruler / zoom-window / origin —
gerber-view-only), prove it, then the rest. Leave Bucket 1 alone (legitimate
shared core). Defer Buckets 2–3 until private-state migration is done.

## Open questions

- Adopt CopperMine's `citizens/` + `ids.rs` convention verbatim? (Default: yes.)
- Extend `citizen_panel!` to carry the render method up front, or in Phase 4?
- Exact per-field four-bucket classification of `SharedServices` — do it
  incrementally (per citizen) or as one Phase-3 pass? (Default: incremental.)
