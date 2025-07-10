use bevy_ecs::prelude::*;
use egui::{Color32, Painter, Pos2, Stroke};
use nalgebra::Vector2;

/// Resource to track custom origin settings
#[derive(Resource, Clone, Debug)]
pub struct CustomOrigin {
    /// Whether custom origin is enabled
    pub enabled: bool,
    /// The custom origin position in PCB coordinates (mm)
    pub position: Vector2<f64>,
    /// Visual marker size for the origin
    pub marker_size: f32,
    /// Whether we're currently setting the origin
    pub setting_mode: bool,
    /// Temporary position while setting (for preview)
    pub preview_position: Option<Vector2<f64>>,
}

impl Default for CustomOrigin {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Vector2::new(0.0, 0.0),
            marker_size: 10.0,
            setting_mode: false,
            preview_position: None,
        }
    }
}

/// Component to mark entities that need coordinate transformation
#[derive(Component)]
pub struct RequiresOriginTransform;

/// System to render the custom origin marker
pub fn render_origin_marker_system(
    origin: &CustomOrigin,
    painter: &Painter,
    view_state: &gerber_viewer::ViewState,
) {
    if !origin.enabled && !origin.setting_mode {
        return;
    }

    // Determine which position to render
    let render_pos = if origin.setting_mode && origin.preview_position.is_some() {
        origin.preview_position.unwrap()
    } else {
        origin.position
    };

    // Convert PCB coordinates to screen coordinates
    let screen_pos = Pos2::new(
        view_state.translation.x + (render_pos.x as f32 * view_state.scale),
        view_state.translation.y - (render_pos.y as f32 * view_state.scale), // Flip Y
    );

    // Draw origin marker (crosshair)
    let color = if origin.setting_mode {
        Color32::from_rgb(255, 128, 0) // Orange when setting
    } else {
        Color32::from_rgb(255, 0, 0) // Red when set
    };

    let stroke = Stroke::new(2.0, color);
    let size = origin.marker_size;

    // Draw crosshair
    painter.line_segment(
        [
            Pos2::new(screen_pos.x - size, screen_pos.y),
            Pos2::new(screen_pos.x + size, screen_pos.y),
        ],
        stroke,
    );

    painter.line_segment(
        [
            Pos2::new(screen_pos.x, screen_pos.y - size),
            Pos2::new(screen_pos.x, screen_pos.y + size),
        ],
        stroke,
    );

    // Draw circle at center
    painter.circle_stroke(screen_pos, size * 0.3, stroke);

    // Draw "0,0" label
    painter.text(
        Pos2::new(screen_pos.x + size + 5.0, screen_pos.y - 10.0),
        egui::Align2::LEFT_CENTER,
        "Origin (0,0)",
        egui::FontId::default(),
        color,
    );
}

/// Transform coordinates relative to custom origin
pub fn transform_to_custom_origin(
    world_coord: Vector2<f64>,
    origin: &CustomOrigin,
) -> Vector2<f64> {
    if origin.enabled {
        world_coord - origin.position
    } else {
        world_coord
    }
}

/// Transform coordinates from custom origin back to world
pub fn transform_from_custom_origin(
    custom_coord: Vector2<f64>,
    origin: &CustomOrigin,
) -> Vector2<f64> {
    if origin.enabled {
        custom_coord + origin.position
    } else {
        custom_coord
    }
}

/// System to update BOM component coordinates based on custom origin
pub fn update_bom_coordinates_system(
    origin: Res<CustomOrigin>,
    mut bom_components: Query<(&crate::ui::bom_panel_v2::PcbPosition, &mut crate::ui::bom_panel_v2::PcbComponent)>,
) {
    if !origin.is_changed() {
        return;
    }

    for (pos, mut _component) in bom_components.iter_mut() {
        // Transform the position based on custom origin
        let world_pos = Vector2::new(pos.x, pos.y);
        let _custom_pos = transform_to_custom_origin(world_pos, &origin);
        
        // Note: We don't modify the component directly here
        // The BOM display should read the origin and transform on display
    }
}

/// Helper to convert screen position to PCB coordinates
pub fn screen_to_pcb_coordinates(
    screen_pos: Pos2,
    view_state: &gerber_viewer::ViewState,
    _display_manager: &crate::display::DisplayManager,
) -> Vector2<f64> {
    // Convert to gerber coordinates first
    let gerber_pos = view_state.screen_to_gerber_coords(screen_pos);
    
    // The gerber coordinates are already in PCB space
    // We just need to convert from nalgebra Point to Vector
    Vector2::new(gerber_pos.x, gerber_pos.y)
}