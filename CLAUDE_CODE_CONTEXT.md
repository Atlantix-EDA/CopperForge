# Claude Code Context for KiForge WGPU Integration

## Project Overview
KiForge is a KiCad PCB CAM tool written in Rust using egui. We need to integrate wgpu for hardware-accelerated rendering.

## Current Code Structure
- Main entry point: src/main.rs
- Uses egui with eframe
- Gerber parsing is in src/gerber/
- Current renderer is CPU-based

## Integration Goals
1. Add wgpu rendering while keeping egui UI
2. Improve performance for large PCBs (400+ components)
3. Add proper layer rendering with transparency
4. Maintain existing functionality

## Key Constraints
- Must work with existing gerber_types, gerber_parser
- Keep egui for UI elements
- Maintain cross-platform compatibility
- Use tracing instead of env_logger

## Code Style
- Use short variable names in loops (i, j, etc.)
- Prefer functional style where appropriate
- Add comprehensive error handling
- Include tracing/logging for debugging

## Dependencies to Add
```toml
wgpu = "0.19"
egui-wgpu = "0.27"
egui-winit = "0.27"
pollster = "0.3"
bytemuck = "1.14"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

## Please Help With
1. Incrementally integrate wgpu without breaking existing code
2. Create proper module structure
3. Handle the transition from eframe to winit+wgpu
4. Ensure all existing features still work
5. Add proper error handling and logging