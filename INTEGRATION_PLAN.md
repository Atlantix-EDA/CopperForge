# KiForge WGPU Integration Plan

## Overview
Integrate wgpu renderer into KiForge to add hardware-accelerated rendering for PCB visualization.

## Current State
- Using egui for UI
- Have gerber parsing working
- Need to add wgpu for better performance

## Integration Steps

### Phase 1: Setup Dependencies
- [ ] Update Cargo.toml with wgpu dependencies
- [ ] Replace env_logger with tracing
- [ ] Create renderer module structure

### Phase 2: Basic WGPU Integration
- [ ] Create wgpu_renderer.rs
- [ ] Set up basic window and surface
- [ ] Integrate with existing egui setup
- [ ] Test basic rendering

### Phase 3: PCB Rendering
- [ ] Convert gerber data to vertices
- [ ] Create layer rendering system
- [ ] Add camera controls
- [ ] Implement layer visibility toggles

### Phase 4: Advanced Features
- [ ] Add selection highlighting
- [ ] Implement zoom/pan controls
- [ ] Add performance monitoring
- [ ] Create depth buffer for proper layering

## File Structure
```
src/
├── main.rs (update with new wgpu app)
├── renderer/
│   ├── mod.rs
│   ├── wgpu_renderer.rs
│   ├── pcb_renderer.rs
│   └── shaders/
│       └── pcb.wgsl
├── logging.rs (new tracing setup)
└── app_wgpu.rs (new wgpu application)
```

## Key Integration Points
1. Main app initialization in main.rs
2. Gerber data conversion in pcb_renderer.rs
3. UI integration in app_wgpu.rs
4. Performance logging throughout

## Testing Checklist
- [ ] Basic window opens
- [ ] egui UI renders
- [ ] Can load PCB file
- [ ] Layers render correctly
- [ ] Performance is improved