use eframe::egui;
use egui_taffy::{taffy, tui};
use taffy::prelude::{length, percent, Style};
use egui_taffy::TuiBuilderLogic;

pub struct TaffyPanel;

impl TaffyPanel {
    pub fn render(ui: &mut egui::Ui) {
        tui(ui, "taffy_panel")
            .reserve_available_space()
            .style(taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Row,
                align_items: Some(taffy::AlignItems::Start),
                padding: length(8.),
                gap: length(16.),
                size: taffy::Size {
                    width: percent(1.0),
                    height: taffy::Dimension::Auto,
                },
                ..Default::default()
            })
            .show(|tui| {
                // Left section
                tui.style(Style {
                    display: taffy::Display::Flex,
                    flex_direction: taffy::FlexDirection::Column,
                    gap: length(8.0),
                    flex_grow: 0.0,
                    flex_shrink: 0.0,
                    flex_basis: length(160.0),
                    padding: length(8.0),
                    ..Default::default()
                })
                .add(|tui| {
                    tui.ui(|ui| {
                        ui.heading("Left Section");
                        if ui.button("Button 1").clicked() {
                            // Handle click
                        }
                    });
                });

                // Right section
                tui.style(Style {
                    display: taffy::Display::Flex,
                    flex_direction: taffy::FlexDirection::Column,
                    gap: length(8.0),
                    flex_grow: 1.0,
                    flex_shrink: 0.0,
                    padding: length(8.0),
                    size: taffy::Size {
                        width: length(200.0),
                        height: taffy::Dimension::Auto,
                    },
                    ..Default::default()
                })
                .add(|tui| {
                    tui.ui(|ui| {
                        ui.heading("Right Section");
                        ui.add(egui::TextEdit::singleline(&mut String::new()).hint_text("Input field"));
                    });
                });
            });
    }
}
