use eframe::emath::Rect;
use eframe::epaint::Color32;
use gerber_viewer::ViewState;
use nalgebra::Point2;

pub struct GridSettings {
    pub enabled: bool,
    pub spacing_mm: f32,  // Always store in mm internally
    pub dot_size: f32,
    pub snap_enabled: bool,  // Enterprise feature: snap to grid
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            spacing_mm: 2.54,  // 100 mils = 2.54 mm
            dot_size: 1.0,
            snap_enabled: false,  // Default off for existing users
        }
    }
}

/// Draw grid on the viewport
pub fn draw_grid(
    painter: &egui::Painter,
    viewport: &Rect,
    view_state: &ViewState,
    settings: &GridSettings,
) {
    if !settings.enabled {
        return;
    }
    
    // Grid spacing is stored in mm
    let grid_spacing_gerber = settings.spacing_mm as f64;
    
    // Convert to screen units
    let grid_spacing_screen = grid_spacing_gerber * view_state.scale as f64;
    
    // Skip if grid spacing is too small to be visible (less than 5 pixels)
    if grid_spacing_screen < 5.0 {
        return;
    }
    
    // Skip if grid spacing is too large (more than half viewport)
    if grid_spacing_screen > (viewport.width().min(viewport.height()) as f64 * 0.5) {
        return;
    }
    
    // Convert viewport bounds to gerber coordinates
    let top_left = view_state.screen_to_gerber_coords(viewport.min);
    let bottom_right = view_state.screen_to_gerber_coords(viewport.max);
    
    // Due to Y inversion, we need to get proper min/max
    let min_x = top_left.x.min(bottom_right.x);
    let max_x = top_left.x.max(bottom_right.x);
    let min_y = top_left.y.min(bottom_right.y);
    let max_y = top_left.y.max(bottom_right.y);
    
    // Calculate grid start/end indices
    let start_x = (min_x / grid_spacing_gerber).floor() as i32 - 1;
    let end_x = (max_x / grid_spacing_gerber).ceil() as i32 + 1;
    let start_y = (min_y / grid_spacing_gerber).floor() as i32 - 1;
    let end_y = (max_y / grid_spacing_gerber).ceil() as i32 + 1;
    
    // Limit the number of grid points to prevent performance issues
    let max_points = 10000;
    let total_points = ((end_x - start_x) * (end_y - start_y)).abs();
    if total_points > max_points {
        return;
    }
    
    // Grid color - adjust opacity based on grid density
    let opacity = if grid_spacing_screen > 50.0 { 120 } else { 60 };
    let grid_color = Color32::from_rgba_premultiplied(100, 100, 100, opacity);
    
    // Draw grid dots
    for grid_x in start_x..=end_x {
        for grid_y in start_y..=end_y {
            let x = grid_x as f64 * grid_spacing_gerber;
            let y = grid_y as f64 * grid_spacing_gerber;
            let grid_pos = crate::drc_operations::types::Position { x, y };
            let screen_pos = view_state.gerber_to_screen_coords(grid_pos.to_point2());
            
            // Only draw if within viewport
            if viewport.contains(screen_pos) {
                painter.circle_filled(screen_pos, settings.dot_size, grid_color);
            }
        }
    }
}

/// Get grid visibility status message
pub fn get_grid_status(view_state: &ViewState, grid_spacing_mm: f32) -> GridStatus {
    let grid_spacing_gerber = grid_spacing_mm as f64;
    let grid_spacing_screen = grid_spacing_gerber * view_state.scale as f64;
    
    if grid_spacing_screen < 5.0 {
        GridStatus::TooFine
    } else if grid_spacing_screen > 300.0 {
        GridStatus::TooCoarse
    } else {
        GridStatus::Visible(grid_spacing_screen)
    }
}

pub enum GridStatus {
    TooFine,
    TooCoarse,
    Visible(f64),
}

/// Enterprise feature: Snap a point to the nearest grid intersection
/// Returns the snapped position in gerber coordinates
pub fn snap_to_grid(point: Point2<f64>, grid_settings: &GridSettings) -> Point2<f64> {
    if !grid_settings.snap_enabled {
        return point;
    }
    
    let grid_spacing = grid_settings.spacing_mm as f64;
    
    // Snap X coordinate
    let snapped_x = (point.x / grid_spacing).round() * grid_spacing;
    
    // Snap Y coordinate  
    let snapped_y = (point.y / grid_spacing).round() * grid_spacing;
    
    Point2::new(snapped_x, snapped_y)
}

/// Enterprise feature: Align view to grid
/// Adjusts the view translation so the gerber content aligns with grid intersections
pub fn align_to_grid(view_state: &mut gerber_viewer::ViewState, grid_settings: &GridSettings) {
    if !grid_settings.enabled {
        return;
    }
    
    let grid_spacing = grid_settings.spacing_mm as f64;
    let grid_spacing_screen = grid_spacing * view_state.scale as f64;
    
    // Skip if grid spacing is too small or too large to be meaningful
    if grid_spacing_screen < 5.0 || grid_spacing_screen > 300.0 {
        return;
    }
    
    // Get current translation
    let current_translation = view_state.translation;
    
    // Calculate the offset to align to the nearest grid line
    let offset_x = current_translation.x % grid_spacing_screen as f32;
    let offset_y = current_translation.y % grid_spacing_screen as f32;
    
    // Snap to the nearest grid intersection
    let snap_x = if offset_x.abs() < grid_spacing_screen as f32 / 2.0 {
        -offset_x
    } else {
        grid_spacing_screen as f32 - offset_x
    };
    
    let snap_y = if offset_y.abs() < grid_spacing_screen as f32 / 2.0 {
        -offset_y
    } else {
        grid_spacing_screen as f32 - offset_y
    };
    
    // Apply the alignment adjustment
    view_state.translation.x += snap_x;
    view_state.translation.y += snap_y;
}

