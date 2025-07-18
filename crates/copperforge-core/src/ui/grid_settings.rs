use crate::{DemoLensApp, project::constants::LOG_TYPE_GRID, display::grid::{get_grid_status, GridStatus}};
use crate::ecs::{UnitsResource, mm_to_nm, nm_to_mm, mils_to_nm, nm_to_mils};
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;

pub fn show_grid_panel<'a>(
    ui: &mut egui::Ui, 
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);
    
    ui.add_space(4.0);
    if ui.checkbox(&mut app.grid_settings.enabled, "Enable Grid").changed() {
        logger.log_custom(
            LOG_TYPE_GRID,
            &format!("Grid display {}", if app.grid_settings.enabled { "enabled" } else { "disabled" })
        );
    }
    
    ui.horizontal(|ui| {
        // Get units from ECS
        let units_resource = app.ecs_world.get_resource::<UnitsResource>()
            .expect("UnitsResource should exist");
        
        let label = if units_resource.is_mils() {
            "Grid Spacing (mils):"
        } else {
            "Grid Spacing (mm):"
        };
        ui.label(label);
        
        let _prev_spacing_mm = app.grid_settings.spacing_mm;
        
        if units_resource.is_mils() {
            // Convert to mils for display using nanometer precision
            let spacing_nm = mm_to_nm(app.grid_settings.spacing_mm);
            let mut spacing_mils = nm_to_mils(spacing_nm);
            let prev_mils = spacing_mils;
            
            // Add slider
            let slider_response = ui.add(
                egui::Slider::new(&mut spacing_mils, 1.0..=1000.0)
                    .logarithmic(true)
            );
            
            // Add text input box next to slider
            let text_response = ui.add(
                egui::DragValue::new(&mut spacing_mils)
                    .speed(1.0)
                    .range(1.0..=1000.0)
                    .suffix(" mils")
            );
            
            if slider_response.changed() || text_response.changed() {
                // Convert back through nanometers for precision
                let spacing_nm = mils_to_nm(spacing_mils);
                app.grid_settings.spacing_mm = nm_to_mm(spacing_nm);
                logger.log_custom(
                    LOG_TYPE_GRID,
                    &format!("Grid spacing changed from {:.1} to {:.1} mils", prev_mils, spacing_mils)
                );
            }
        } else {
            // Work directly in mm
            let prev_mm = app.grid_settings.spacing_mm;
            
            // Add slider
            let slider_response = ui.add(
                egui::Slider::new(&mut app.grid_settings.spacing_mm, 0.025..=25.0)
                    .logarithmic(true)
            );
            
            // Add text input box next to slider
            let text_response = ui.add(
                egui::DragValue::new(&mut app.grid_settings.spacing_mm)
                    .speed(0.1)
                    .range(0.025..=25.0)
                    .suffix(" mm")
            );
            
            if slider_response.changed() || text_response.changed() {
                logger.log_custom(
                    LOG_TYPE_GRID,
                    &format!("Grid spacing changed from {:.2} to {:.2} mm", prev_mm, app.grid_settings.spacing_mm)
                );
            }
        }
    });
    
    ui.horizontal(|ui| {
        ui.label("Grid Dot Size:");
        let prev_dot_size = app.grid_settings.dot_size;
        if ui.add(egui::Slider::new(&mut app.grid_settings.dot_size, 0.5..=5.0)).changed() {
            logger.log_custom(
                LOG_TYPE_GRID,
                &format!("Grid dot size changed from {:.1} to {:.1}", prev_dot_size, app.grid_settings.dot_size)
            );
        }
    });
    
    // Enterprise features section
    ui.separator();
    ui.heading("Grid Features");
    
    // Snap to grid checkbox
    if ui.checkbox(&mut app.grid_settings.snap_enabled, "Snap to Grid").changed() {
        logger.log_custom(
            LOG_TYPE_GRID,
            &format!("Snap to grid {}", if app.grid_settings.snap_enabled { "enabled" } else { "disabled" })
        );
    }
    
    // Align to grid button
    ui.horizontal(|ui| {
        if ui.button("⌗ Align View to Grid (A)").clicked() {
            crate::display::align_to_grid(&mut app.view_state, &app.grid_settings);
            logger.log_custom(LOG_TYPE_GRID, "View aligned to grid");
        }
        
        ui.label("Aligns the view so content snaps to grid intersections");
    });
    
    // Show grid visibility status
    if app.grid_settings.enabled {
        ui.separator();
        let status = get_grid_status(&app.view_state, app.grid_settings.spacing_mm);
        
        match status {
            GridStatus::TooFine => {
                ui.colored_label(egui::Color32::from_rgb(255, 165, 0), 
                    egui::RichText::new("⚠ Grid too fine to display - zoom in or increase spacing").small());
            }
            GridStatus::TooCoarse => {
                ui.colored_label(egui::Color32::from_rgb(255, 165, 0), 
                    egui::RichText::new("⚠ Grid too coarse - zoom out or decrease spacing").small());
            }
            GridStatus::Visible(spacing_pixels) => {
                ui.colored_label(egui::Color32::from_rgb(0, 255, 0), 
                    egui::RichText::new(format!("✓ Grid visible (~{:.0} pixels)", spacing_pixels)).small());
            }
        }
    }
}