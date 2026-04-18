//! BOM panel — extracts and displays bill of materials from .kicad_pcb files.
//!
//! Uses kiparse to parse the PCB file directly. No live KiCad connection needed.

use crate::CopperForgeApp;
use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use egui_extras::{TableBuilder, Column};

/// Cached BOM state — parsed once, rendered every frame without re-parsing.
pub struct BomPanelState {
    pub entries: Vec<crate::bom::BomEntry>,
    pub dimensions: Option<crate::bom::BoardDimensions>,
    pub summary: Vec<(String, usize)>,
    pub filter_text: String,
    pub pcb_path_hash: u64,
    pub selected_index: Option<usize>,
}

impl BomPanelState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            dimensions: None,
            summary: Vec::new(),
            filter_text: String::new(),
            pcb_path_hash: 0,
            selected_index: None,
        }
    }
}

/// Show the BOM panel.
pub fn show_bom_panel<'a>(
    ui: &mut egui::Ui,
    app: &mut CopperForgeApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    // Get PCB path from project state
    let pcb_path = app.project_manager.state.pcb_path().map(|p| p.to_path_buf());

    // Initialize BOM state if needed
    if app.bom_state.is_none() {
        app.bom_state = Some(BomPanelState::new());
    }

    let bom_state = app.bom_state.as_mut().unwrap();

    ui.vertical(|ui| {
        // Toolbar
        ui.horizontal(|ui| {
            if let Some(ref pcb) = pcb_path {
                let path_hash = {
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    pcb.hash(&mut h);
                    h.finish()
                };

                // Only parse on explicit button click — NOT on hash mismatch
                if ui.button("Extract BOM").clicked() {
                    match crate::bom::extract_bom(pcb) {
                        Ok(entries) => {
                            logger.log_info(&format!("Extracted {} components from {}",
                                entries.len(),
                                pcb.file_name().and_then(|n| n.to_str()).unwrap_or("?")));
                            bom_state.summary = crate::bom::component_summary(&entries);
                            bom_state.entries = entries;
                            bom_state.pcb_path_hash = path_hash;
                        }
                        Err(e) => {
                            logger.log_error(&format!("BOM extraction failed: {}", e));
                        }
                    }
                    if let Ok(Some(dims)) = crate::bom::extract_board_dimensions(pcb) {
                        logger.log_info(&format!("Board: {:.1} x {:.1} mm ({:.1} mm²)",
                            dims.width_mm, dims.height_mm, dims.area_mm2));
                        bom_state.dimensions = Some(dims);
                    }
                }

                // Auto-extract once when a new PCB is loaded (hash changed)
                if bom_state.pcb_path_hash != path_hash && bom_state.entries.is_empty() {
                    if let Ok(entries) = crate::bom::extract_bom(pcb) {
                        bom_state.summary = crate::bom::component_summary(&entries);
                        bom_state.entries = entries;
                        bom_state.pcb_path_hash = path_hash;
                    }
                    if let Ok(Some(dims)) = crate::bom::extract_board_dimensions(pcb) {
                        bom_state.dimensions = Some(dims);
                    }
                }

                ui.label(
                    egui::RichText::new(format!("{} components", bom_state.entries.len()))
                        .color(crate::theme::TokyoNight::CYAN)
                );
            } else {
                ui.label("No PCB file selected — open a project first");
                return;
            }

            ui.separator();

            // Filter
            ui.label("Filter:");
            ui.text_edit_singleline(&mut bom_state.filter_text);
        });

        if bom_state.entries.is_empty() {
            return;
        }

        ui.separator();

        // Summary bar (cached — no recalculation per frame)
        ui.horizontal_wrapped(|ui| {
            for (prefix, count) in &bom_state.summary {
                ui.label(
                    egui::RichText::new(format!("{}:{}", prefix, count))
                        .color(crate::theme::TokyoNight::FG_DIM)
                        .monospace()
                );
            }
            if let Some(ref dims) = bom_state.dimensions {
                ui.separator();
                ui.label(
                    egui::RichText::new(format!("{:.1}x{:.1}mm", dims.width_mm, dims.height_mm))
                        .color(crate::theme::TokyoNight::GREEN)
                        .monospace()
                );
            }
        });

        ui.separator();

        // BOM table
        let filtered: Vec<&crate::bom::BomEntry> = bom_state.entries.iter()
            .filter(|e| e.matches_filter(&bom_state.filter_text))
            .collect();

        let available = ui.available_size();
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .min_scrolled_height(available.y - 20.0)
            .column(Column::auto().at_least(30.0))        // #
            .column(Column::auto().at_least(55.0))        // Reference
            .column(Column::auto().at_least(70.0))        // Value
            .column(Column::auto().at_least(80.0))        // Description
            .column(Column::remainder().at_least(100.0))  // Footprint
            .column(Column::auto().at_least(55.0))        // X
            .column(Column::auto().at_least(55.0))        // Y
            .column(Column::auto().at_least(45.0))        // Layer
            .header(18.0, |mut header| {
                header.col(|ui| { ui.strong("#"); });
                header.col(|ui| { ui.strong("Ref"); });
                header.col(|ui| { ui.strong("Value"); });
                header.col(|ui| { ui.strong("Description"); });
                header.col(|ui| { ui.strong("Footprint"); });
                header.col(|ui| { ui.strong("X"); });
                header.col(|ui| { ui.strong("Y"); });
                header.col(|ui| { ui.strong("Layer"); });
            })
            .body(|body| {
                body.rows(16.0, filtered.len(), |mut row| {
                    let entry = filtered[row.index()];
                    row.col(|ui| { ui.label(entry.item.to_string()); });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.reference)
                                .color(crate::theme::TokyoNight::CYAN)
                                .monospace()
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.value)
                                .color(crate::theme::TokyoNight::GREEN)
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.description)
                                .color(crate::theme::TokyoNight::FG_DIM)
                        );
                    });
                    row.col(|ui| {
                        ui.label(
                            egui::RichText::new(&entry.footprint)
                                .color(crate::theme::TokyoNight::COMMENT)
                                .monospace()
                        );
                    });
                    row.col(|ui| { ui.label(format!("{:.2}", entry.x)); });
                    row.col(|ui| { ui.label(format!("{:.2}", entry.y)); });
                    row.col(|ui| { ui.label(&entry.layer); });
                });
            });
    });
}
