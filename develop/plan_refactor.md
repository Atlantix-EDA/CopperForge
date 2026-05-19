# CopperForge — `app.rs` Refactor Plan

`crates/copperforge-core/src/app.rs` is ~1,700 lines doing six distinct jobs.
This plan captures the extraction strategy without executing it — work lands
on its own dedicated branch to avoid merge conflicts with in-flight feature
branches (notably `feature/render-3d`).

## Current shape (as of 2026-04-21)

Rough breakdown of `app.rs`:

| Lines | Concern |
|---|---|
| 30–320 (~290) | `CopperForgeApp` struct + `new()` constructor — wiring |
| 320–700 (~380) | Method helpers — file dialog, PCB workflow helpers, dock state save/load |
| 717–957 (~240) | `eframe::App::update` body — top bar, about window, dock area, banner, hotkeys |
| inside `update` | Release modal rendering |
| inside `update` | Project edit modal rendering |
| inside `update` | Project import modal rendering |

Each of those is doing its own thing and has no reason to know about the
others. The file is a god-file, and the god-struct it defines isn't the
problem — the 1700 lines of surrounding code around the struct is.

## Target shape

```
crates/copperforge-core/src/app/
├── mod.rs            # struct + new() + eframe::App impl — ~400 lines
├── dock.rs           # load_dock_state, create_default_dock_state,
│                     #   save_dock_state, TabKind migration list
├── hotkeys.rs        # keyboard shortcut dispatch block
├── top_bar.rs        # menu bar, about modal, shortcuts panel
└── modals/
    ├── mod.rs
    ├── release.rs        # ReleaseModalState + rendering
    ├── project_edit.rs   # project edit modal
    └── project_import.rs # import modal + file dialog
```

Target line count for `app/mod.rs`: ~400 lines. The struct definition, the
constructor, and a `update()` that calls into the extracted modules.

## Priority-ordered extraction list

| # | Extract | Why this order | Rough savings |
|---|---|---|---|
| 1 | `app/dock.rs` | Zero coupling to `self`'s other state — pure persistence + construction | ~80 |
| 2 | `app/hotkeys.rs` | Self-contained `ctx.input_mut(...)` block, easy to move | ~50 |
| 3 | `app/top_bar.rs` | UI chrome; depends on `&mut self` but boundary is clean | ~200 |
| 4 | `app/modals/release.rs` | Self-contained modal, ~200 lines in a single block | ~250 |
| 5 | `app/modals/project_edit.rs` | Same pattern | ~150 |
| 6 | `app/modals/project_import.rs` | Same, slightly larger due to file dialog | ~200 |

Take them one at a time, verify `cargo check` after each, commit each
extraction as its own commit so a revert is local. Total expected reduction:
~930 lines → ~770 lines remaining → aim for ~400 by also moving the
helper-method chunk into a dedicated `app/helpers.rs` if it still feels heavy.

## What NOT to extract (yet)

- The struct definition itself. It's 80-odd fields, but splitting it
  introduces indirection without win. The real medicine is `SharedServices`
  expansion (Phase 3 already started this) — defer.
- `eframe::App::update` the trait impl. It stays on the struct. Its *body*
  becomes a dispatcher that calls `top_bar::show`, `hotkeys::handle`,
  `modals::release::show_if_open`, etc.

## Sequencing with `feature/render-3d`

**Do the refactor on a fresh branch off master.** Rationale:

- The refactor is mechanical (move code, fix imports, adjust visibility) and
  has nothing to do with 3D.
- If done on `feature/render-3d`, it creates a massive diff that makes the
  3D work hard to review and any other branch hard to rebase.
- Done first on master, `feature/render-3d` rebases cleanly onto the new
  layout — only `app.rs` changes need updating (new tab registration moves
  to `app/dock.rs`, the gl_context field and panel construction stay in
  `app/mod.rs`).

Suggested flow:
1. Finish Phase 1–7 of `render-3d` first (or pause at a good checkpoint).
2. Branch `refactor/app-extraction` off `master`.
3. Execute the 6 extractions, one commit each.
4. Open PR, merge to master.
5. `git rebase master` on `feature/render-3d` — conflicts should be scoped
   to tab registration and the panel field, both easy to fix.

## Branch cleanup candidates (audit 2026-04-21)

While auditing, these remote branches look like stale / superseded work:

| Branch | Age / status | Keep? |
|---|---|---|
| `origin/feature-wgpu-integration` | Earlier wgpu attempt | Candidate for deletion; we're on glow now |
| `origin/feature/ecs-3d-integration` | **Previous 3D attempt via ECS — failed** | Delete; `feature/render-3d` supersedes |
| `origin/feature/gerber-3d-simple` | Simpler 3D attempt | Delete if superseded by `feature/render-3d` |
| `origin/feature-extrusion` | 3D-adjacent experiment | Audit, likely delete |
| `origin/ecs-legacy`, `origin/ecs-migration` | Retired ECS migration | Delete after confirming nothing's needed |
| `origin/units-ecs` | Likely folded into master via Phase 3/4 | Verify, delete |
| `migrate-to-citizen` (local), `origin/migrate-to-citizen` | Merged / superseded | Delete |
| `origin/feature-banner`, `origin/feature-cleanup-data`, `origin/feature-layers-separate`, `origin/feature-new-project`, `origin/feature-png`, `origin/viewer-features`, `origin/gerver-viewer-updates`, `origin/kicad_pcb`, `origin/workspace-architecture`, `origin/drc-simple`, `origin/add-project-manager` | Likely all merged into current master | Audit one-by-one, delete if merged |

Recommended approach: use `git branch --merged master` locally and
`git branch -r --merged origin/master` remotely to find fully-merged
branches, then delete those first. Anything unmerged gets one-by-one review.

This is not blocking — do it when the mood strikes, or as a tidying pass
after the refactor lands.
