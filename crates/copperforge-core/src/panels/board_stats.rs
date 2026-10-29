//! Board-statistics Citizen panel — board dimensions, component breakdown,
//! part commonality, and the manufacturability score, shown as a dock tab.
//!
//! A proper Citizen (see `citizen_panel!`), cross-registered into the dock
//! `TabKind` like every other panel — not a plugin or overlay. Compute runs
//! in-process via `services::manuf`; the BOM + dimensions are parsed from the
//! loaded `.kicad_pcb`, cached and only recomputed when the loaded board's
//! geometry generation changes.

use super::citizen_panel;
use eframe::egui;
use services::manuf::{self, Stats};

use crate::bom::{extract_board_dimensions, extract_bom};
use crate::services::SharedServices;

citizen_panel!(
    BoardStatsPanel,
    "board_stats",
    key: Option<u64> = None,
    stats: Option<Stats> = None,
    error: Option<String> = None,
);

impl BoardStatsPanel {
    /// Recompute when a board is loaded and its geometry generation changes;
    /// clear when no board is loaded. Tied to `board_geometry_gen`, NOT the
    /// project path — selecting a project without loading gerbers shows
    /// nothing, and a reload/clear invalidates the cache.
    fn refresh(&mut self, services: &SharedServices) {
        if services.geometry.board_outline.is_none() {
            self.key = None;
            self.stats = None;
            self.error = None;
            return;
        }
        let generation = services.board_geometry_gen;
        if self.key == Some(generation) {
            return;
        }
        self.key = Some(generation);
        self.stats = None;
        self.error = None;

        let Some(pcb_path) = services
            .project_state
            .get()
            .pcb_path()
            .map(|p| p.to_path_buf())
        else {
            return;
        };
        match extract_bom(&pcb_path) {
            Ok(entries) => {
                let dims = extract_board_dimensions(&pcb_path).ok().flatten();
                let (w, h) = dims.map(|d| (d.width_mm, d.height_mm)).unwrap_or((0.0, 0.0));
                // Outer outline contour (mm) for centroid + weight, if loaded.
                let outline = services
                    .geometry
                    .board_outline
                    .as_ref()
                    .and_then(|o| o.contours.first())
                    .map(|c| c.iter().map(|p| [p.x as f64, p.y as f64]).collect())
                    .unwrap_or_default();
                let req = manuf::Request {
                    entries: entries
                        .iter()
                        .map(|e| manuf::entry(&e.value, &e.footprint, &e.layer))
                        .collect(),
                    width_mm: w,
                    height_mm: h,
                    outline,
                };
                self.stats = Some(manuf::board_stats(&req));
            }
            Err(e) => self.error = Some(e),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut SharedServices) {
        self.refresh(services);

        let dim = egui::Color32::from_rgb(140, 150, 170);
        if let Some(err) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(240, 115, 100), err);
            return;
        }
        let Some(a) = self.stats.clone() else {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Load a KiCad project (.kicad_pcb) to see board analysis.")
                    .italics()
                    .color(dim),
            );
            return;
        };

        let row = |ui: &mut egui::Ui, label: &str, value: String| {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.monospace(value);
                });
            });
        };

        ui.label(egui::RichText::new("Board").strong());
        row(ui, "Width", format!("{:.2} mm", a.width_mm));
        row(ui, "Height", format!("{:.2} mm", a.height_mm));
        row(ui, "Area", format!("{:.1} mm²", a.area_mm2));
        row(
            ui,
            "Centroid",
            format!("({:.1}, {:.1}) mm", a.centroid[0], a.centroid[1]),
        );
        row(ui, "Est. weight", format!("{:.1} g", a.weight_g));
        ui.add_space(8.0);
        ui.separator();

        ui.label(egui::RichText::new("Components").strong());
        row(ui, "Total", a.total.to_string());
        row(ui, "Top", a.top.to_string());
        row(ui, "Bottom", a.bottom.to_string());
        row(ui, "SMT", a.smt.to_string());
        row(ui, "Through-hole", a.tht.to_string());
        row(ui, "Unclassified", a.unclassified.to_string());
        ui.add_space(8.0);
        ui.separator();

        ui.label(egui::RichText::new("Part commonality").strong());
        row(ui, "Unique parts", a.unique.to_string());
        row(ui, "Unique / total", format!("{:.0} %", a.unique_ratio * 100.0));
        row(ui, "Reuse", format!("{:.1}× /part", a.reuse));
        ui.add_space(8.0);
        ui.separator();

        ui.label(egui::RichText::new("Manufacturability").strong());
        let [r, g, b, _] = a.grade_rgba();
        let grade_col = egui::Color32::from_rgb(r, g, b);
        ui.horizontal(|ui| {
            ui.label("Score");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.monospace(
                    egui::RichText::new(format!("{} / 100  ({})", a.score, a.grade))
                        .color(grade_col),
                );
            });
        });
    }
}
