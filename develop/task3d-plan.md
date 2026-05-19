# CopperForge — 3D Viewer Task Plan

Incremental rollout of a 3D gerber/component viewer inside the existing egui
app. Target: render extruded copper layers plus component bodies (glTF first,
STEP later) in a new viewport panel, using the backend the app already ships
with.

## Context

- Reference: [`timschmidt/alumina-interface`](https://github.com/timschmidt/alumina-interface)
  — a Rust/eframe/glow CNC UI that embeds a hand-rolled OpenGL renderer via
  `egui_glow::CallbackFn`. `src/renderer.rs` (~120 lines) is the pattern we
  mirror.
- CopperForge already uses `eframe = "0.33"` with the `glow` feature
  (see root `Cargo.toml`), so the integration is drop-in.

## Decisions

- **glow, not wgpu.** Fewer concepts (VBO, VAO, program, uniform, draw call)
  maps 1:1 to the alumina reference and to every OpenGL tutorial. Revisit
  wgpu *after* the renderer is working and its responsibilities are
  well-understood; migration is then mechanical.
- **Module, not a crate (yet).** Lives at
  `crates/copperforge-core/src/render3d/`. Designed *as if* it were a separate
  crate: `render3d` knows nothing about gerbers, layers, or `layer_store`
  types. `copperforge-core` does gerber→vertices conversion and hands down
  plain float buffers. Extract to `crates/copper-3d/` when any of:
  1. A second workspace crate needs to render.
  2. The API is stable enough to publish.
  3. Compile churn on the renderer starts rebuilding core too often.
- **Two shader programs from day one** (once we reach Phase 5):
  `UnlitProgram` for extruded copper/outlines (flat colors), `LitProgram` for
  component bodies (normals + Lambert). Different vertex layouts ⇒ different
  VBOs, bind program-of-the-day before each draw.

## Semantics cheat sheet (Qt 2D → OpenGL 3D)

Kept here because it's easy to forget coming back after a month.

| Qt 2D | OpenGL 3D | What it actually is |
|---|---|---|
| `QGraphicsItem` | **mesh** | a `Vec<f32>` of triangle vertices |
| `QGraphicsScene` | — | own `Vec<Mesh>` — no framework type |
| `paintEvent` | `PaintCallback` closure | per-frame "now draw" hook |
| `QPainter::setPen` | **shader program** | tiny GPU routine, compiled once |
| `QPainter::setTransform` | **MVP matrix** | one 4×4 uploaded per draw |
| `setZValue` | **depth buffer** | per-pixel Z test |
| `QPainter::drawPolygon` | `gl.draw_arrays` | the actual draw call |

Three matrices: **M**odel (where the object sits) · **V**iew (inverse of
camera position) · **P**rojection (3D→2D collapse, perspective or ortho).
Upload `MVP = P·V·M` to the shader each frame.

**VBO** = GPU byte buffer holding vertex data. **VAO** = recipe for how to
read the VBO (stride + attribute offsets); set up once, bind before drawing.

## Module layout

```
crates/copperforge-core/src/
├── render3d/
│   ├── mod.rs          # re-exports
│   ├── renderer.rs     # UnlitProgram, LitProgram, meshes, compile helper
│   ├── camera.rs       # Camera { rotation, translation, zoom } + mvp()
│   └── mesh.rs         # ColoredMesh (6 floats/vert), LitMesh (9 floats/vert)
└── panels/
    └── gerber_view_3d.rs  # hosts the viewport: allocate rect, PaintCallback
```

`render3d` depends only on `glow`, `nalgebra`, `bytemuck`, `egui`,
`egui_glow`. No `layer_store`, no gerber types.

## Phased milestones

Each phase produces a visible diff in the viewport. If phase N looks wrong,
the bug is in what N added — earlier phases were already known-good.

### Phase 1 — Axes gizmo (smoke test)
- [ ] Create `crates/copperforge-core/src/render3d/` module skeleton
- [ ] Implement `UnlitProgram` (GLSL 330: position + color → MVP transform)
- [ ] Implement `ColoredMesh` (VBO+VAO, 6 floats/vertex, LINES or TRIANGLES)
- [ ] Implement `Camera` with `mvp(viewport: egui::Rect) -> Matrix4<f32>`
- [ ] Create `panels/gerber_view_3d.rs` hosting the viewport via
      `allocate_exact_size(..., Sense::drag())` + `egui_glow::CallbackFn`
- [ ] Draw 6-vertex axis gizmo: red X, green Y, blue Z
- [ ] Wire mouse drag → `camera.rotation`, wheel → `camera.zoom`
- **Test:** three perpendicular lines rotatable with the mouse. If this
  works, the pipeline works.

### Phase 2 — Ground grid
- [ ] Build an N-line XY gridlines `ColoredMesh` at Z=0
- [ ] Add it to the render list
- **Test:** spatial context — you can tell where "the board" will sit.

### Phase 3 — Flat board outline
- [ ] Pull M1 Board Outline polygon from `layer_store` (KiCad 10
      canonical name — see `feedback_kicad10_terminology.md`)
- [ ] Triangulate → `ColoredMesh` with one fill color
- [ ] Convert in `copperforge-core` (not in `render3d/`), hand down floats
- **Test:** FR-4-colored quad/polygon at Z=0, sized like the real board.

### Phase 4 — Extruded copper layer
- [ ] Take one top-copper polygon, extrude to small Z height
      (top cap + bottom cap + sidewalls = triangles)
- [ ] Verify depth buffer: copper should sit on top of board, not Z-fight
- [ ] Add `POLYGON_OFFSET_FILL` for outline crispness (alumina trick)
- **Test:** copper "pops out" of the board; no flickering where layers meet.

### Phase 5 — Lit program
- [ ] `LitProgram` (GLSL 330: position + normal, Lambert term, one
      directional light, uniform base color)
- [ ] `LitMesh` (9 floats/vertex: xyz + nx ny nz + rgb)
- [ ] Render a hardcoded cube through the lit program as a sanity check
- **Test:** each cube face shades differently under the light.

### Phase 6 — Component body loader
KiCad ships component models as **STEP** (`.step` / `.stp`) — the
authoritative format — and sometimes also **WRL** (VRML, legacy KiCad 3D
viewer format). Native glTF is rare in the PCB ecosystem. Decide the path
*before* coding Phase 6:

- **Option A — in-process STEP tessellation** via `truck-stepio`. Pure
  Rust, no external tool, slow on first load but results can be cached.
  Output goes straight into a `LitMesh`.
- **Option B — offline STEP → glTF conversion** via FreeCAD CLI or
  equivalent, run once per library, cached under the project tree. Loader
  then uses the `gltf` crate. Shifts complexity to an ingestion step but
  keeps runtime simple and fast.
- **Option C — WRL loader for models that ship it.** Trivial (triangle
  soup + material colors, already tessellated). Works for much of the
  KiCad standard library. Doesn't cover STEP-only models.

Proposed: **C first** (fast win — read the `.wrl` files KiCad already
tessellated), then **A** to cover STEP-only models, with tessellation
results cached in the project database (`redb`). Revisit B only if
`truck-stepio` turns out to be too slow on real libraries.

- [ ] Prototype WRL loader → `LitMesh`
- [ ] Draw one component body at origin
- [ ] Add `truck-stepio` for STEP fallback; cache tessellated result
- **Test:** a component (e.g. DIP-14) renders with recognizable geometry.

### Phase 7 — Per-instance placement
- [ ] Extend `LitMesh::draw(gl, model_matrix)` so shader MVP = `P·V·M_i`
- [ ] Pull footprint positions + rotations from the parsed `.kicad_pcb`
- [ ] Resolve each footprint's 3D model path (KiCad stores it in the
      footprint definition — `(model "path/to/foo.step" ...)`)
- [ ] Draw N copies at their placement transforms
- **Test:** components sit on correct footprint locations with correct
  rotation.

## Deferred / out of scope for v1

- **Picking / hit-testing.** Alumina's TODO calls this out too. Useful for
  "click a footprint, select it in the BOM," but not a v1 blocker.
- **PBR / shadows / reflections.** Lambert is fine for PCB component bodies.
- **Textures.** No silkscreen decals or soldermask maps in v1.
- **Instanced rendering** (real `glDrawArraysInstanced`). Phase 7 does
  instancing via per-draw MVP upload; good enough until N ≫ 100.
- **wgpu migration.** Revisit once renderer is stable.

## Answered decisions

- **Viewport placement.** New tab alongside the existing 2D gerber viewer
  (not a mode toggle inside it). Two independent views, shared layer-store
  data, separate camera state.
- **Stackup Z heights.** Primary: extract from the KiCad `.kicad_pcb`
  `(stackup ...)` block via `kiparse`. Fallback: parse the M12 Stackup
  mechanical layer if the stackup block is absent or incomplete. Last
  resort: hardcoded defaults (35µm copper, 1.6mm FR-4). Graceful
  degradation; never block rendering on missing stackup.
- **Component body sourcing.** KiCad ships STEP (always) and WRL (often,
  for standard libraries). Each footprint in the `.kicad_pcb` has a
  `(model "…" (offset …) (scale …) (rotate …))` block pointing to its 3D
  file on disk. The resolver lives in `copperforge-core`, reads footprint
  model paths, hands `render3d` a `LitMesh` + per-instance transform.
  Conversion strategy for STEP: see Phase 6.
