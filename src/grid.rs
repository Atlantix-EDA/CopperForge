use eframe::emath::Rect;
use eframe::epaint::Color32;
use gerber_viewer::ViewState;

pub struct GridSettings {
    pub enabled: bool,
    pub spacing_mils: f32,
    pub dot_size: f32,
}

impl Default for GridSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            spacing_mils: 10.0,
            dot_size: 1.0,
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
    
    // The CMOS S7 gerber files use millimeters (mm) as the unit
    // 1 mil = 0.0254 mm, so to convert mils to mm we multiply by 0.0254
    let grid_spacing_gerber = settings.spacing_mils as f64 * 0.0254;
    
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
            let grid_pos = gerber_viewer::position::Position::new(x, y);
            let screen_pos = view_state.gerber_to_screen_coords(grid_pos);
            
            // Only draw if within viewport
            if viewport.contains(screen_pos) {
                painter.circle_filled(screen_pos, settings.dot_size, grid_color);
            }
        }
    }
}

/// Get grid visibility status message
pub fn get_grid_status(view_state: &ViewState, grid_spacing_mils: f32) -> GridStatus {
    let grid_spacing_gerber = grid_spacing_mils as f64 * 0.0254;
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