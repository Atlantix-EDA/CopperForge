use crate::{DemoLensApp, LayerInfo};
use crate::managers::ProjectState;
use egui_lens::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::Dynamic;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::io::BufReader;
use gerber_viewer::gerber_parser::parse;
use gerber_viewer::GerberLayer;

pub fn show_project_panel<'a>(
    ui: &mut egui::Ui,
    app: &'a mut DemoLensApp,
    logger_state: &'a Dynamic<ReactiveEventLoggerState>,
    log_colors: &'a Dynamic<LogColors>,
) {
    let logger = ReactiveEventLogger::with_colors(logger_state, log_colors);

    ui.heading("Project");
    ui.separator();

    // Show current state
    ui.group(|ui| {
        ui.label("Current State:");
        let state_text = match &app.project_manager.state {
            ProjectState::NoProject => "No project loaded",
            ProjectState::PcbSelected { .. } => "PCB file selected",
            ProjectState::GeneratingGerbers { .. } => "Generating gerbers...",
            ProjectState::GerbersGenerated { .. } => "Gerbers generated",
            ProjectState::LoadingGerbers { .. } => "Loading gerbers...",
            ProjectState::Ready { .. } => "Project ready",
        };
        ui.monospace(state_text);
    });

    ui.add_space(10.0);

    // Auto-generation settings
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.project_manager.auto_generate_on_startup, "Auto-generate on startup");
    });
    ui.horizontal(|ui| {
        ui.checkbox(&mut app.project_manager.auto_reload_on_change, "Auto-reload on file change");
    });

    ui.add_space(10.0);

    ui.label("KiCad PCB File:");
    ui.add_space(5.0);

    // Get current PCB path from state
    let current_pcb_path = match &app.project_manager.state {
        ProjectState::NoProject => None,
        ProjectState::PcbSelected { pcb_path } |
        ProjectState::GeneratingGerbers { pcb_path } |
        ProjectState::GerbersGenerated { pcb_path, .. } |
        ProjectState::LoadingGerbers { pcb_path, .. } |
        ProjectState::Ready { pcb_path, .. } => Some(pcb_path.clone()),
    };

    // Text input field for PCB file path
    ui.horizontal(|ui| {
        let mut path_str = current_pcb_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        
        let response = ui.add(
            egui::TextEdit::singleline(&mut path_str)
                .desired_width(ui.available_width() - 80.0)
                .hint_text("Enter path to .kicad_pcb file...")
        );
        
        // Update path if user edited the text
        if response.changed() {
            if path_str.is_empty() {
                app.project_manager.state = ProjectState::NoProject;
                app.selected_pcb_file = None;
            } else {
                let path = PathBuf::from(&path_str);
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_pcb") {
                    app.project_manager.state = ProjectState::PcbSelected { pcb_path: path.clone() };
                    app.selected_pcb_file = Some(path);
                }
            }
        }

        if ui.button("Browse...").clicked() {
            app.project_manager.open_file_dialog();
        }
    });

    // Update file dialog and handle selection
    if let Some(path_buf) = app.project_manager.update_file_dialog(ui.ctx()) {
        app.selected_pcb_file = Some(path_buf.clone());
        logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
    }

    ui.add_space(10.0);

    // Show appropriate controls based on current state
    match &app.project_manager.state.clone() {
        ProjectState::NoProject => {
            ui.label("No PCB file selected");
        },
        ProjectState::PcbSelected { pcb_path } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            if ui.button("Generate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                app.generating_gerbers = true;
                logger.log_info("Generating gerbers from PCB file...");
            }
        },
        ProjectState::GeneratingGerbers { pcb_path } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.add_enabled(false, egui::Button::new("Generating..."));
            
            // Handle generation
            if app.generating_gerbers {
                if let Some(output_dir) = generate_gerbers_from_pcb(pcb_path, &logger) {
                    app.project_manager.state = ProjectState::GerbersGenerated {
                        pcb_path: pcb_path.clone(),
                        gerber_dir: output_dir.clone(),
                    };
                    app.generated_gerber_dir = Some(output_dir);
                } else {
                    // Generation failed, go back to selected state
                    app.project_manager.state = ProjectState::PcbSelected { pcb_path: pcb_path.clone() };
                }
                app.generating_gerbers = false;
            }
        },
        ProjectState::GerbersGenerated { pcb_path, gerber_dir } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.label(format!("Gerbers in: {}", gerber_dir.display()));
            ui.add_space(5.0);
            
            if ui.button("Load Gerbers into Viewer").clicked() {
                app.project_manager.state = ProjectState::LoadingGerbers {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                };
                app.loading_gerbers = true;
                logger.log_info("Loading gerbers into viewer...");
            }
            
            if ui.button("Regenerate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                app.generating_gerbers = true;
            }
        },
        ProjectState::LoadingGerbers { pcb_path, gerber_dir } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.add_enabled(false, egui::Button::new("Loading..."));
            
            // Handle loading
            if app.loading_gerbers {
                load_gerbers_into_viewer(app, gerber_dir, &logger);
                let last_modified = std::fs::metadata(pcb_path)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::SystemTime::now());
                    
                app.project_manager.state = ProjectState::Ready {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                    last_modified,
                };
                app.loading_gerbers = false;
            }
        },
        ProjectState::Ready { pcb_path, gerber_dir, last_modified } => {
            show_pcb_info(ui, pcb_path);
            ui.add_space(10.0);
            
            ui.label("✓ Gerbers loaded and displayed");
            
            // Check if file has been modified
            if let Ok(metadata) = std::fs::metadata(pcb_path) {
                if let Ok(modified) = metadata.modified() {
                    if &modified != last_modified {
                        ui.colored_label(egui::Color32::YELLOW, "⚠ PCB file has been modified");
                    }
                }
            }
            
            ui.add_space(5.0);
            
            if ui.button("Reload Gerbers").clicked() {
                app.project_manager.state = ProjectState::LoadingGerbers {
                    pcb_path: pcb_path.clone(),
                    gerber_dir: gerber_dir.clone(),
                };
                app.loading_gerbers = true;
            }
            
            if ui.button("Regenerate Gerbers").clicked() {
                app.project_manager.state = ProjectState::GeneratingGerbers { pcb_path: pcb_path.clone() };
                app.generating_gerbers = true;
            }
        },
    }
}

fn show_pcb_info(ui: &mut egui::Ui, pcb_path: &Path) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label("File:");
            ui.monospace(pcb_path.file_name()
                .unwrap_or_default()
                .to_string_lossy());
        });
        
        if pcb_path.exists() {
            ui.label("✓ File exists");
        } else {
            ui.colored_label(egui::Color32::RED, "✗ File not found");
        }
    });
}

fn generate_gerbers_from_pcb(pcb_path: &Path, logger: &ReactiveEventLogger) -> Option<PathBuf> {
    // Create output directory in the same location as the PCB file
    let output_dir = pcb_path.parent()
        .unwrap_or(Path::new("."))
        .join("gerber_output");
    
    // Create directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        logger.log_error(&format!("Failed to create output directory: {}", e));
        return None;
    }

    logger.log_info(&format!("Output directory: {}", output_dir.display()));

    // Run KiCad CLI to generate gerbers
    // Try to find kicad-cli in PATH first, then fall back to known locations
    let kicad_cli_path = if let Ok(output) = Command::new("which").arg("kicad-cli").output() {
        if output.status.success() {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        } else {
            // Check common installation paths
            let paths = [
                "/usr/lib/kicad-nightly/bin/kicad-cli",
                "/usr/lib/kicad/bin/kicad-cli",
                "/usr/local/bin/kicad-cli",
                "/opt/kicad/bin/kicad-cli",
            ];
            
            paths.iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "kicad-cli".to_string())
        }
    } else {
        "/usr/lib/kicad-nightly/bin/kicad-cli".to_string()
    };
    
    logger.log_info(&format!("Using KiCad CLI at: {}", kicad_cli_path));
    
    // Set up environment for KiCad libraries
    let mut cmd = Command::new(&kicad_cli_path);
    
    // Add library path for KiCad nightly if needed
    if kicad_cli_path.contains("kicad-nightly") {
        let lib_path = "/usr/lib/kicad-nightly/lib/x86_64-linux-gnu";
        let current_ld_path = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        let new_ld_path = if current_ld_path.is_empty() {
            lib_path.to_string()
        } else {
            format!("{}:{}", lib_path, current_ld_path)
        };
        cmd.env("LD_LIBRARY_PATH", new_ld_path);
        logger.log_info(&format!("Set LD_LIBRARY_PATH for KiCad nightly: {}", lib_path));
    }
    
    let output = cmd
        .arg("pcb")
        .arg("export")
        .arg("gerbers")
        .arg("--output")
        .arg(&output_dir)
        .arg("--layers")
        .arg("F.Cu,B.Cu,F.SilkS,B.SilkS,F.Mask,B.Mask,Edge.Cuts,F.Paste,B.Paste")
        .arg("--no-protel-ext")  // Use .gbr extension
        .arg(pcb_path)
        .output();

    match output {
        Ok(result) => {
            if result.status.success() {
                logger.log_info("Gerbers generated successfully!");
                logger.log_info(&format!("Check output directory: {}", output_dir.display()));
                
                // Log the generated files
                if let Ok(entries) = std::fs::read_dir(&output_dir) {
                    for entry in entries.flatten() {
                        if let Some(ext) = entry.path().extension() {
                            if ext == "gbr" {
                                logger.log_info(&format!("  Generated: {}", entry.file_name().to_string_lossy()));
                            }
                        }
                    }
                }
                return Some(output_dir);
            } else {
                logger.log_error("Failed to generate gerbers");
                if let Ok(stderr) = String::from_utf8(result.stderr) {
                    logger.log_error(&format!("Error: {}", stderr));
                }
            }
        }
        Err(e) => {
            logger.log_error(&format!("Failed to run kicad-cli: {}", e));
            logger.log_error("Make sure KiCad is installed and kicad-cli is in your PATH");
        }
    }
    None
}

fn load_gerbers_into_viewer(app: &mut DemoLensApp, gerber_dir: &Path, logger: &ReactiveEventLogger) {
    use crate::layer_detection::UnassignedGerber;
    
    // Clear all existing layers and unassigned gerbers first
    logger.log_info("Clearing existing gerber layers...");
    app.layer_manager.layers.clear();
    app.layer_manager.unassigned_gerbers.clear();
    app.layer_manager.layer_assignments.clear();
    
    let mut loaded_count = 0;
    let mut unassigned_count = 0;
    
    // Read directory and load all gerber files
    if let Ok(entries) = std::fs::read_dir(gerber_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("gbr") {
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                
                // Try to load and parse the gerber file
                match std::fs::read_to_string(&path) {
                    Ok(gerber_content) => {
                        let reader = BufReader::new(gerber_content.as_bytes());
                        match parse(reader) {
                            Ok(doc) => {
                                let commands = doc.into_commands();
                                let gerber_layer = GerberLayer::new(commands);
                                
                                // Try to detect layer type using regex patterns
                                if let Some(detected_type) = app.layer_manager.layer_detector.detect_layer_type(&filename) {
                                    // Check if we already have this layer type assigned
                                    if let Some(existing_assignment) = app.layer_manager.layer_assignments.iter()
                                        .find(|(_, layer_type)| **layer_type == detected_type)
                                        .map(|(fname, _)| fname.clone()) {
                                        // This layer type is already assigned to another file
                                        logger.log_warning(&format!(
                                            "Layer type {:?} already assigned to {}. Adding {} to unassigned list.",
                                            detected_type, existing_assignment, filename
                                        ));
                                        app.layer_manager.unassigned_gerbers.push(UnassignedGerber {
                                            filename: filename.clone(),
                                            content: gerber_content.clone(),
                                            parsed_layer: gerber_layer,
                                        });
                                        unassigned_count += 1;
                                    } else {
                                        // Create layer info
                                        let layer_info = LayerInfo::new(
                                            detected_type,
                                            Some(gerber_layer),
                                            Some(gerber_content.clone()),
                                            true, // All layers have their checkbox checked by default
                                        );
                                        
                                        // Insert into layers map
                                        app.layer_manager.layers.insert(detected_type, layer_info);
                                        app.layer_manager.layer_assignments.insert(filename.clone(), detected_type);
                                        loaded_count += 1;
                                        logger.log_info(&format!("Loaded {} as {:?}", filename, detected_type));
                                    }
                                } else {
                                    // Could not detect layer type, add to unassigned list
                                    logger.log_warning(&format!("Could not detect layer type for: {}", filename));
                                    app.layer_manager.unassigned_gerbers.push(UnassignedGerber {
                                        filename: filename.clone(),
                                        content: gerber_content,
                                        parsed_layer: gerber_layer,
                                    });
                                    unassigned_count += 1;
                                }
                            }
                            Err(e) => {
                                logger.log_error(&format!("Failed to parse {}: {:?}", filename, e));
                            }
                        }
                    }
                    Err(e) => {
                        logger.log_error(&format!("Failed to read {}: {}", filename, e));
                    }
                }
            }
        }
    }
    
    if loaded_count > 0 {
        logger.log_info(&format!("Successfully loaded {} gerber layers", loaded_count));
        if unassigned_count > 0 {
            logger.log_warning(&format!("{} gerber files could not be automatically assigned", unassigned_count));
        }
        app.needs_initial_view = true; // Trigger view reset
    } else if unassigned_count > 0 {
        logger.log_warning(&format!("No layers were automatically detected. {} gerber files need manual assignment.", unassigned_count));
    } else {
        logger.log_error("No gerber files were found");
    }
}