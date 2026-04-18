use std::{fs, path::PathBuf};

use eframe::emath::{Rect, Vec2};
use egui::Pos2;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex};

use crate::display;
use crate::display::DisplayManager;
use crate::drc_operations::DrcManager;

use crate::event_logger::{ReactiveEventLogger, ReactiveEventLoggerState, LogColors};
use egui_mobius_reactive::*;
use gerber_viewer::{
   BoundingBox,
   ViewState, UiState, GerberTransform
};
use crate::platform::parameters::gui::VERSION;
use crate::project;
use crate::ui;
use crate::project_manager;
use crate::services::SharedServices;

use crate::ui::{Tab, TabKind, TabViewer, initialize_and_show_banner, show_system_info};

use crate::project::{load_demo_gerber, ProjectState, manager::ProjectConfig};
use crate::display::GridSettings;

/// The main application struct. A thin outer layer around `SharedServices`;
/// panels operate on services, never directly on fields of this struct.
pub struct CopperForgeApp {
    /// All cross-panel state lives here.
    pub services: SharedServices,

    // ── Citizen infrastructure ────────────────────────────────
    pub dispatcher: egui_citizen::Dispatcher,
    pub app_messages: Vec<crate::messages::AppMessage>,

    // ── Dock state (eframe owns) ──────────────────────────────
    dock_state: DockState<Tab>,

    // ── File dialogs (I/O handles) ────────────────────────────
    pub pcb_file_dialog: egui_file_dialog::FileDialog,
    pub last_picked_pcb_file: Option<PathBuf>,
    pub projects_directory_dialog: egui_file_dialog::FileDialog,
    pub last_picked_projects_directory: Option<PathBuf>,

    // ── Panel-owned state (temporary — migrates into citizens) ─
    pub bom_state: Option<ui::BomPanelState>,
    pub project_manager_state: Option<project_manager::ProjectManagerState>,
    pub term_output: Vec<String>,
    pub term_cmd_buf: String,
    pub shell_log: Vec<String>,
    pub shell_cmd_buf: String,
}

impl Drop for CopperForgeApp {
    fn drop(&mut self) {
        self.save_dock_state();
        self.save_settings();
    }
}

/// Panic with a lengthy, stage-aware diagnostic when init fails.
fn panic_init(stage: &str, err: impl std::fmt::Display, hints: &[&str]) -> ! {
    eprintln!();
    eprintln!("============================================================");
    eprintln!("  CopperForge — initialization failure");
    eprintln!("============================================================");
    eprintln!();
    eprintln!("  Stage: {stage}");
    eprintln!("  Error: {err}");
    eprintln!();
    if !hints.is_empty() {
        eprintln!("  Hints:");
        for h in hints {
            eprintln!("    • {h}");
        }
        eprintln!();
    }
    eprintln!("  If this persists, capture the above and file an issue:");
    eprintln!("    https://github.com/Atlantix-EDA/CopperForge/issues");
    eprintln!("============================================================");
    eprintln!();
    panic!("CopperForge init failed at stage: {stage}");
}

impl CopperForgeApp {
    pub fn sync_units_to_ecs(&mut self) {
        if self.services.global_units_mils {
            self.services.layer_store.units.display_unit = crate::layer_store::DisplayUnit::Mils;
        } else {
            self.services.layer_store.units.display_unit = crate::layer_store::DisplayUnit::Millimeters;
        }
    }

    pub fn sync_units_from_ecs(&mut self) {
        self.services.global_units_mils = self.services.layer_store.units.is_mils();
    }

    pub fn sync_zoom_to_ecs(&mut self) {
        self.services.layer_store.zoom.set_scale(self.services.view_state.scale);
        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;
    }

    pub fn sync_zoom_from_ecs(&mut self) {
        self.services.view_state.scale = self.services.layer_store.zoom.scale;
        self.services.view_state.translation.x = self.services.layer_store.zoom.center_x;
        self.services.view_state.translation.y = self.services.layer_store.zoom.center_y;
    }

    pub fn render_layers_ecs(&mut self, painter: &egui::Painter) {
        let view_state = self.services.view_state;
        let rotation = self.services.rotation_degrees;
        self.services.layer_store.render(painter, view_state, &self.services.display_manager, rotation);
    }

    pub fn new() -> Self {
        let dummy_viewport = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 768.0));

        // ── Stage 1: LoadConfig ──────────────────────────────────
        let config_path: PathBuf = dirs::config_dir()
            .map(|d| d.join("copperforge"))
            .unwrap_or_else(|| {
                panic_init(
                    "LoadConfig",
                    "dirs::config_dir() returned None",
                    &["The OS didn't expose a config directory (XDG_CONFIG_HOME / %APPDATA%).",
                      "Set XDG_CONFIG_HOME to a writable path and retry."],
                )
            });
        let config = match ProjectConfig::load_from_file(&config_path) {
            Ok(cfg) => cfg,
            Err(e) => panic_init(
                "LoadConfig",
                e,
                &[
                    "project_config.json exists but failed to deserialize.",
                    "If the file is from an older CopperForge with incompatible schema,",
                    "delete it: rm ~/.config/copperforge/project_config.json",
                    "(CopperForge will recreate it with defaults on next launch.)",
                ],
            ),
        };

        // ── Stage 2: DiscoverKiCad (slow on first Flatpak launch) ─
        let (kicad_version, kicad_cli_method) = Self::probe_kicad_cli();

        // ── Stage 3: InitializeDb ────────────────────────────────
        let db_path = config_path.join("projects.db");
        let project_db = match crate::project_manager::database::ProjectDatabase::new(&db_path) {
            Ok(db) => db,
            Err(e) => panic_init(
                "InitializeDb",
                e,
                &[
                    &format!("Failed to open sled DB at {}", db_path.display()),
                    "Another CopperForge process may be holding a lock — close it.",
                    "Or the database is corrupted — delete the directory and restart:",
                    "  rm -rf ~/.config/copperforge/projects.db",
                ],
            ),
        };

        // ── Stage 4: Wire SharedServices ─────────────────────────
        let mut initial_logger_state = ReactiveEventLoggerState::new();
        initial_logger_state.show_timestamps = false;
        let logger_state = Dynamic::new(initial_logger_state);
        let log_colors = Dynamic::new(LogColors::default());
        let project_state = Dynamic::new(config.state.clone());

        let mut layer_store = crate::layer_store::LayerStore::default();
        if config.global_units_mils {
            layer_store.units.display_unit = crate::layer_store::DisplayUnit::Mils;
        } else {
            layer_store.units.display_unit = crate::layer_store::DisplayUnit::Millimeters;
        }

        let services = SharedServices {
            project_state,
            logger_state,
            log_colors,
            config_path: config_path.clone(),
            kicad_version,
            kicad_cli_method,
            project_db,
            layer_store,
            gerber_layer: load_demo_gerber(),
            view_state: ViewState::default(),
            ui_state: UiState::default(),
            needs_initial_view: true,
            rotation_degrees: 0.0,
            display_manager: DisplayManager::new(),
            drc_manager: DrcManager::new(),
            grid_settings: GridSettings::default(),
            global_units_mils: config.global_units_mils,
            user_timezone: config.user_timezone.clone(),
            use_24_hour_clock: config.use_24_hour_clock,
            zoom_window_start: None,
            zoom_window_dragging: false,
            setting_origin_mode: false,
            origin_has_been_set: false,
            ruler_active: false,
            ruler_start: None,
            ruler_end: None,
            ruler_dragging: false,
            ruler_drag_start: None,
            latched_measurement_start: None,
            latched_measurement_end: None,
            show_about_modal: false,
            show_kicad_version_modal: false,
            config,
        };

        // ── Stage 5: Register citizens ───────────────────────────
        let mut dispatcher = egui_citizen::Dispatcher::new();
        use egui_citizen::message::CitizenId;
        for id in [
            "gerber_view", "view_settings", "drc", "project", "projects",
            "settings", "bom",
            "shell", "terminal", "logger",
        ] {
            dispatcher.register(CitizenId::new(id));
        }
        dispatcher.activate(&CitizenId::new("gerber_view"));
        let _ = dispatcher.drain_messages();

        let dock_state = Self::create_default_dock_state();

        let mut app = Self {
            services,
            dispatcher,
            app_messages: Vec::new(),
            dock_state,
            pcb_file_dialog: egui_file_dialog::FileDialog::new(),
            last_picked_pcb_file: None,
            projects_directory_dialog: egui_file_dialog::FileDialog::new(),
            last_picked_projects_directory: None,
            bom_state: None,
            project_manager_state: None,
            term_output: Vec::new(),
            term_cmd_buf: String::new(),
            shell_log: Vec::new(),
            shell_cmd_buf: String::new(),
        };

        let logger = ReactiveEventLogger::with_colors(&app.services.logger_state, &app.services.log_colors);
        initialize_and_show_banner(&logger);
        app.prune_stale_project_state();

        app.reset_view(dummy_viewport);

        app
    }

    /// Prune project_state if its on-disk artifacts no longer exist.
    fn prune_stale_project_state(&mut self) {
        let current = self.services.project_state.get();
        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
        match &current {
            ProjectState::NoProject => {
                logger.log_info("No previous project found. Please select a PCB file.");
            }
            ProjectState::PcbSelected { pcb_path }
            | ProjectState::GeneratingGerbers { pcb_path } => {
                if !pcb_path.exists() {
                    self.services.project_state.set(ProjectState::NoProject);
                }
            }
            ProjectState::GerbersGenerated { pcb_path, gerber_dir }
            | ProjectState::Ready { pcb_path, gerber_dir, .. }
            | ProjectState::LoadingGerbers { pcb_path, gerber_dir } => {
                if !pcb_path.exists() || !gerber_dir.exists() {
                    self.services.project_state.set(ProjectState::NoProject);
                }
            }
        }
    }

    pub fn reset_view(&mut self, viewport: Rect) {
        let combined_bbox = self.services.layer_store.combined_bounding_box();
        let bbox = combined_bbox.unwrap_or_else(|| self.services.gerber_layer.bounding_box().clone());
        let content_width = bbox.width();
        let content_height = bbox.height();

        let scale = f32::min(
            viewport.width() / (content_width as f32),
            viewport.height() / (content_height as f32),
        );
        let scale = scale * 0.95;

        if self.services.display_manager.design_offset.x != 0.0 || self.services.display_manager.design_offset.y != 0.0 {
            let origin_screen_x = viewport.left() + viewport.width() * 0.2;
            let origin_screen_y = viewport.bottom() - viewport.height() * 0.2;

            let origin_gerber_x = self.services.display_manager.design_offset.x;
            let origin_gerber_y = self.services.display_manager.design_offset.y;

            self.services.view_state.translation = Vec2::new(
                origin_screen_x - (origin_gerber_x as f32 * scale),
                origin_screen_y + (origin_gerber_y as f32 * scale),
            );
        } else {
            let gerber_center = bbox.center();

            self.services.display_manager.center_offset = display::VectorOffset {
                x: -gerber_center.x,
                y: -gerber_center.y,
            };

            let origin: nalgebra::Vector2<f64> = self.services.display_manager.center_offset.clone().into();
            let offset: nalgebra::Vector2<f64> = self.services.display_manager.design_offset.clone().into();
            let transform = GerberTransform {
                rotation: self.services.rotation_degrees.to_radians(),
                mirroring: self.services.display_manager.mirroring.clone().into(),
                origin: origin - offset,
                offset,
                scale: 1.0,
            };

            let outline_vertices: Vec<_> = bbox
                .vertices()
                .into_iter()
                .map(|v| transform.apply_to_position(v))
                .collect();

            let transformed_bbox = BoundingBox::from_points(&outline_vertices);
            let transformed_center = transformed_bbox.center();

            self.services.view_state.translation = Vec2::new(
                viewport.center().x - (transformed_center.x as f32 * scale),
                viewport.center().y + (transformed_center.y as f32 * scale),
            );
        }

        self.services.view_state.scale = scale;

        self.services.layer_store.zoom.set_scale(scale);
        self.services.layer_store.zoom.set_fit_to_view_scale(scale);
        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;

        self.services.needs_initial_view = false;
    }

    /// Zoom to a specific BOM component location
    pub fn zoom_to_component(&mut self, component: &project_manager::bom::BomComponent, viewport: Rect) {
        if !self.services.origin_has_been_set {
            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
            logger.log_warning("Please set the origin before using cross-probing");
            return;
        }

        let comp_x = component.x_location;
        let comp_y = component.y_location;

        let viewport_center = viewport.center();
        self.services.view_state.translation = Vec2::new(
            viewport_center.x - (comp_x as f32 * self.services.view_state.scale),
            viewport_center.y + (comp_y as f32 * self.services.view_state.scale),
        );

        self.services.layer_store.zoom.center_x = self.services.view_state.translation.x;
        self.services.layer_store.zoom.center_y = self.services.view_state.translation.y;

        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
        logger.log_info(&format!("Cross-probed to component: {} at ({:.2}, {:.2})",
                                component.reference, comp_x, comp_y));
    }

    /// Open the PCB file dialog.
    pub fn open_pcb_file_dialog(&mut self) {
        self.pcb_file_dialog.pick_file();
    }

    /// Poll the PCB file dialog. On new pick of a .kicad_pcb, updates
    /// `project_state` to `PcbSelected` and returns the path.
    pub fn update_pcb_file_dialog(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if let Some(path) = self.pcb_file_dialog.update(ctx).picked() {
            let path_buf = path.to_path_buf();
            if self.last_picked_pcb_file.as_ref() != Some(&path_buf) {
                self.last_picked_pcb_file = Some(path_buf.clone());
                if path.extension().and_then(|s| s.to_str()) == Some("kicad_pcb") {
                    self.services.project_state.set(ProjectState::PcbSelected { pcb_path: path_buf.clone() });
                    return Some(path_buf);
                }
            }
        }
        None
    }

    fn show_clock_display(&mut self, ui: &mut egui::Ui) {
        use chrono::{Local, Utc};
        use chrono_tz::Tz;

        if ui.button(egui::RichText::new(format!("CopperForge v{}", VERSION))
            .color(egui::Color32::from_rgb(180, 200, 255))).clicked() {
            self.services.show_about_modal = true;
        }

        ui.separator();

        let kicad_text = if let Some(ref version) = self.services.kicad_version {
            format!("KiCad {}", version)
        } else {
            "KiCad (not found)".to_string()
        };

        if ui.button(egui::RichText::new(kicad_text)
            .color(egui::Color32::from_rgb(180, 255, 200))).clicked() {
            self.services.show_kicad_version_modal = true;
        }

        ui.separator();

        let time_format = if self.services.use_24_hour_clock { "%H:%M:%S" } else { "%I:%M:%S %p" };
        let date_format = "%Y-%m-%d";

        let clock_text = if let Some(tz_name) = &self.services.user_timezone {
            if let Ok(tz) = tz_name.parse::<Tz>() {
                let now = Utc::now().with_timezone(&tz);
                format!("{} 🕐 {} {}", now.format(date_format), now.format(time_format), tz.name())
            } else {
                let now = Local::now();
                format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
            }
        } else {
            let now = Local::now();
            format!("{} 🕐 {}", now.format(date_format), now.format(time_format))
        };

        ui.label(egui::RichText::new(clock_text).color(egui::Color32::from_rgb(220, 220, 220)));
    }

    /// One-shot KiCad discovery + version parse. Returns (version_string, method).
    fn probe_kicad_cli() -> (Option<String>, Option<String>) {
        let (method, mut cmd) = match Self::find_kicad_cli() {
            Some(f) => f,
            None => return (None, None),
        };
        let output = match cmd.arg("--version").output() {
            Ok(o) if o.status.success() => o,
            _ => return (None, Some(method)),
        };
        let version = Self::parse_kicad_version(&output.stdout, false).map(|mut v| {
            if method != "path" {
                v = format!("{} ({})", v, method);
            }
            v
        });
        (version, Some(method))
    }

    /// Build a `kicad-cli` Command using the cached discovery method — no probe.
    pub fn kicad_cli_command(&self) -> Option<std::process::Command> {
        self.services.kicad_cli_method.as_deref().map(Self::build_kicad_cli_command)
    }

    fn build_kicad_cli_command(method: &str) -> std::process::Command {
        use std::process::Command;
        match method {
            "flatpak" => {
                let mut cmd = Command::new("flatpak");
                cmd.args(["run", "--command=kicad-cli", "org.kicad.KiCad"]);
                cmd
            }
            "snap" => {
                let mut cmd = Command::new("snap");
                cmd.args(["run", "kicad.kicad-cli"]);
                cmd
            }
            _ => Command::new("kicad-cli"),
        }
    }

    pub fn find_kicad_cli() -> Option<(String, std::process::Command)> {
        use std::process::Command;

        for bin in ["kicad-cli", "kicad-cli-nightly"] {
            if let Ok(output) = Command::new(bin).arg("--version").output() {
                if output.status.success() {
                    return Some(("path".into(), Command::new(bin)));
                }
            }
        }

        if let Ok(output) = Command::new("flatpak")
            .args(["run", "--command=kicad-cli", "org.kicad.KiCad", "--version"])
            .output()
        {
            if output.status.success() {
                return Some(("flatpak".into(), Self::build_kicad_cli_command("flatpak")));
            }
        }

        if let Ok(output) = Command::new("snap")
            .args(["run", "kicad.kicad-cli", "--version"])
            .output()
        {
            if output.status.success() {
                return Some(("snap".into(), Self::build_kicad_cli_command("snap")));
            }
        }

        None
    }

    fn parse_kicad_version(stdout: &[u8], nightly: bool) -> Option<String> {
        let version_str = String::from_utf8_lossy(stdout);
        let line = version_str.lines().next()?;
        let mut version = if line.contains("kicad-cli") {
            line.split_whitespace().nth(1)?.to_string()
        } else {
            line.trim().to_string()
        };
        if nightly {
            version.push_str(" (nightly)");
        }
        Some(version)
    }

    fn render_kicad_info_modal(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.heading("KiCad PCB Design Software");
            ui.add_space(10.0);

            if let Some(ref version) = self.services.kicad_version {
                ui.label(egui::RichText::new(format!("Version: {}", version))
                    .size(16.0)
                    .strong());
            } else {
                ui.label(egui::RichText::new("KiCad not detected on system")
                    .size(14.0)
                    .color(egui::Color32::from_rgb(255, 200, 100)));
            }

            ui.add_space(15.0);
            ui.separator();
            ui.add_space(15.0);

            ui.label("KiCad is a free and open-source electronics design automation (EDA) suite.");
            ui.label("It features schematic capture, integrated circuit simulation,");
            ui.label("printed circuit board (PCB) layout, 3D viewing, and SPICE simulation.");

            ui.add_space(10.0);

            ui.hyperlink_to("🌐 Visit KiCad.org", "https://www.kicad.org/");
            ui.hyperlink_to("📖 Documentation", "https://docs.kicad.org/");
            ui.hyperlink_to("💬 Forums", "https://forum.kicad.info/");
        });
    }
}

impl CopperForgeApp {
    fn save_dock_state(&self) {
        if let Some(config_dir) = dirs::config_dir() {
            let copperforge_dir = config_dir.join("copperforge");
            if let Err(e) = fs::create_dir_all(&copperforge_dir) {
                eprintln!("Failed to create config directory: {}", e);
                return;
            }
            let config_path = copperforge_dir.join("dock_state.json");
            match serde_json::to_string_pretty(&self.dock_state) {
                Ok(json) => {
                    if let Err(e) = fs::write(&config_path, json) {
                        eprintln!("Failed to write dock state: {}", e);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to serialize dock state: {}", e);
                }
            }
        }
    }

    fn load_dock_state() -> Option<DockState<Tab>> {
        if let Some(config_dir) = dirs::config_dir() {
            let config_path = config_dir.join("copperforge").join("dock_state.json");
            if let Ok(json) = fs::read_to_string(&config_path) {
                match serde_json::from_str::<DockState<Tab>>(&json) {
                    Ok(dock_state) => {
                        return Some(dock_state);
                    }
                    Err(e) => {
                        eprintln!("Failed to deserialize dock state: {}", e);
                        fs::remove_file(config_path).ok();
                    }
                }
            }
        }
        None
    }

    pub fn save_settings(&self) {
        let mut config = self.services.config.clone();
        config.state = self.services.project_state.get();
        config.user_timezone = self.services.user_timezone.clone();
        config.use_24_hour_clock = self.services.use_24_hour_clock;
        config.global_units_mils = self.services.global_units_mils;

        if let Some(ref manager_state) = self.project_manager_state {
            config.default_author = manager_state.new_kicad_project_author.clone();
            config.default_company = manager_state.new_kicad_project_company.clone();
            config.include_kiverse = manager_state.include_kiverse;
            config.include_atlantix_resistors = manager_state.include_atlantix_resistors;
        }

        if let Err(e) = config.save_to_file(&self.services.config_path) {
            eprintln!("Failed to save settings: {}", e);
        }
    }

    fn create_default_dock_state() -> DockState<Tab> {
        if let Some(saved_dock_state) = Self::load_dock_state() {
            return saved_dock_state;
        }

        let gerber_tab = Tab::new(TabKind::GerberView, SurfaceIndex::main(), NodeIndex(0));
        let drc_tab = Tab::new(TabKind::DRC, SurfaceIndex::main(), NodeIndex(1));
        let view_settings_tab = Tab::new(TabKind::ViewSettings, SurfaceIndex::main(), NodeIndex(2));

        let project_tab = Tab::new(TabKind::Project, SurfaceIndex::main(), NodeIndex(3));
        let settings_tab = Tab::new(TabKind::Settings, SurfaceIndex::main(), NodeIndex(5));

        let projects_tab = Tab::new(TabKind::Projects, SurfaceIndex::main(), NodeIndex(6));

        let logger_tab = Tab::new(TabKind::Logger, SurfaceIndex::main(), NodeIndex(7));
        let terminal_tab = Tab::new(TabKind::Terminal, SurfaceIndex::main(), NodeIndex(8));
        let shell_tab = Tab::new(TabKind::Shell, SurfaceIndex::main(), NodeIndex(9));
        let bom_tab = Tab::new(TabKind::BOM, SurfaceIndex::main(), NodeIndex(10));

        let mut dock_state = DockState::new(vec![gerber_tab]);
        let surface = dock_state.main_surface_mut();

        let [left, right] = surface.split_left(
            NodeIndex::root(),
            0.3,
            vec![project_tab, settings_tab],
        );

        surface.split_below(left, 0.7, vec![projects_tab]);
        surface.split_below(right, 0.5, vec![logger_tab, terminal_tab, shell_tab, bom_tab]);
        surface.split_right(right, 0.5, vec![drc_tab, view_settings_tab]);
        dock_state
    }
}

impl eframe::App for CopperForgeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let show_system_info_clicked = ctx.memory(|mem| {
            mem.data.get_temp::<bool>(egui::Id::new("show_system_info")).unwrap_or(false)
        });

        if show_system_info_clicked {
            ctx.memory_mut(|mem| {
                mem.data.remove::<bool>(egui::Id::new("show_system_info"));
            });
            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
            show_system_info(&logger);
        }

        if self.services.layer_store.is_dirty() {
            self.services.layer_store.mark_clean();
        }

        // Hotkeys (only when no text field has focus)
        let text_input_active = ctx.memory(|mem| mem.focused().is_some());

        if !text_input_active {
            ctx.input(|i| {
                if i.key_pressed(egui::Key::F) {
                    self.services.display_manager.showing_top = !self.services.display_manager.showing_top;

                    use crate::layer_store::{LayerType, Side};
                    for layer_type in LayerType::all() {
                        let visible = match layer_type {
                            LayerType::Copper(1)
                            | LayerType::Silkscreen(Side::Top)
                            | LayerType::Soldermask(Side::Top)
                            | LayerType::Paste(Side::Top) => {
                                self.services.display_manager.showing_top
                            }
                            LayerType::Copper(_) => !self.services.display_manager.showing_top,
                            LayerType::Silkscreen(Side::Bottom)
                            | LayerType::Soldermask(Side::Bottom)
                            | LayerType::Paste(Side::Bottom) => {
                                !self.services.display_manager.showing_top
                            }
                            LayerType::MechanicalOutline => {
                                self.services.layer_store.get_visibility(layer_type)
                            }
                        };
                        self.services.layer_store.set_visibility(layer_type, visible);
                    }

                    let view_name = if self.services.display_manager.showing_top { "top" } else { "bottom" };
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info(&format!("Flipped to {} view (F key)", view_name));
                    self.services.layer_store.mark_dirty();
                }

                if i.key_pressed(egui::Key::U) {
                    self.services.global_units_mils = !self.services.global_units_mils;
                    self.sync_units_to_ecs();
                    let units_name = if self.services.global_units_mils { "mils" } else { "mm" };
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info(&format!("Toggled units to {} (U key)", units_name));
                }

                if i.key_pressed(egui::Key::R) {
                    self.services.rotation_degrees = (self.services.rotation_degrees + 90.0) % 360.0;
                    self.services.layer_store.mark_dirty();

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_custom(
                        project::constants::LOG_TYPE_ROTATION,
                        &format!("Rotated board to {:.0}° (R key)", self.services.rotation_degrees)
                    );
                }

                if i.key_pressed(egui::Key::A) {
                    display::align_to_grid(&mut self.services.view_state, &self.services.grid_settings);
                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Aligned view to grid (A key)");
                }

                if i.key_pressed(egui::Key::M) {
                    if self.services.ruler_active {
                        if self.services.ruler_start.is_some() && self.services.ruler_end.is_some() {
                            self.services.latched_measurement_start = self.services.ruler_start;
                            self.services.latched_measurement_end = self.services.ruler_end;
                        }
                        self.services.ruler_active = false;
                        self.services.ruler_start = None;
                        self.services.ruler_end = None;
                        self.services.ruler_dragging = false;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("Ruler mode deactivated (M key) - measurement latched");
                    } else {
                        self.services.latched_measurement_start = None;
                        self.services.latched_measurement_end = None;
                        self.services.ruler_active = true;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        logger.log_info("Ruler mode activated (M key) - previous measurement cleared");
                    }
                }

                if i.key_pressed(egui::Key::Escape) && self.services.ruler_active {
                    if self.services.ruler_start.is_some() && self.services.ruler_end.is_some() {
                        self.services.latched_measurement_start = self.services.ruler_start;
                        self.services.latched_measurement_end = self.services.ruler_end;

                        let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                        if let (Some(start), Some(end)) = (self.services.ruler_start, self.services.ruler_end) {
                            logger.log_info(&format!("Latching measurement - Start: ({:.6}, {:.6}), End: ({:.6}, {:.6})",
                                                    start.x, start.y, end.x, end.y));
                            let dx = end.x - start.x;
                            let dy = end.y - start.y;
                            logger.log_info(&format!("Latching deltas - ΔX: {:.6}, ΔY: {:.6}", dx, dy));
                        }
                    }

                    self.services.ruler_active = false;
                    self.services.ruler_start = None;
                    self.services.ruler_end = None;
                    self.services.ruler_dragging = false;

                    let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                    logger.log_info("Ruler mode cancelled (ESC key) - measurement latched");
                }
            });
        }

        // Project Ribbon at the top
        egui::TopBottomPanel::top("project_ribbon").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;

                // Prominent current-project indicator on the far left — so a
                // cold start with a remembered PCB is obvious at a glance.
                let state = self.services.project_state.get();
                let (project_label, project_color) = match state.pcb_path() {
                    Some(p) => (
                        format!("📄 {}", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())),
                        egui::Color32::from_rgb(180, 220, 180),
                    ),
                    None => (
                        "📄 (no project loaded)".to_string(),
                        egui::Color32::from_rgb(140, 140, 140),
                    ),
                };
                // Wrap in a group so vertical centering matches the other ribbon widgets.
                ui.group(|ui| {
                    ui.label(egui::RichText::new(project_label).color(project_color).strong());
                });

                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label("📁 KiCad PCB File:");

                        if ui.button("Browse...").clicked() {
                            self.open_pcb_file_dialog();
                        }

                        if let Some(path_buf) = self.update_pcb_file_dialog(ui.ctx()) {
                            let logger = ReactiveEventLogger::with_colors(&self.services.logger_state, &self.services.log_colors);
                            logger.log_info(&format!("Selected PCB file: {}", path_buf.display()));
                        }
                    });
                });

                // Right-aligned section: clock/version first (rightmost), then
                // Hotkeys added LAST so it ends up just to the left of the clock.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.show_clock_display(ui);
                    ui.separator();
                    ui.menu_button("📋 Hotkeys", |ui| {
                        ui.heading("Keyboard Shortcuts");
                        ui.separator();

                        for (key, desc) in [
                            ("F", "Flip Top/Bottom view"),
                            ("R", "Rotate 90° clockwise"),
                            ("U", "Toggle units (mm/mils)"),
                            ("A", "Align view to grid"),
                            ("M", "Toggle ruler/measurement mode"),
                            ("ESC", "Cancel measurement mode"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(key);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(desc);
                                });
                            });
                        }

                        ui.separator();
                        ui.heading("Mouse Controls");

                        for (key, desc) in [
                            ("Double-click", "Center view"),
                            ("Right-click + drag", "Zoom to selection"),
                            ("Scroll wheel", "Zoom in/out"),
                            ("Left-click + drag", "Pan view"),
                            ("Escape", "Cancel zoom selection / measurement mode"),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(key);
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(desc);
                                });
                            });
                        }
                    });
                });
            });
        });

        // Main dock area below the ribbon
        let mut dock_state = self.dock_state.clone();
        let mut dispatcher = std::mem::take(&mut self.dispatcher);
        {
            let mut tab_viewer = TabViewer {
                app: self,
                dispatcher: &mut dispatcher,
            };
            let mut style = Style::from_egui(ctx.style().as_ref());
            style.dock_area_padding = None;
            style.tab_bar.fill_tab_bar = true;

            DockArea::new(&mut dock_state)
                .style(style)
                .show_add_buttons(true)
                .show_close_buttons(true)
                .show(ctx, &mut tab_viewer);
        }

        for msg in dispatcher.drain_messages() {
            self.app_messages.push(crate::messages::AppMessage::Citizen(msg));
        }
        self.dispatcher = dispatcher;
        self.dock_state = dock_state;

        if self.services.show_about_modal {
            egui::Window::new("About CopperForge")
                .collapsible(false)
                .resizable(true)
                .default_size(egui::vec2(400.0, 550.0))
                .default_pos(egui::pos2(
                    ctx.content_rect().center().x - 200.0,
                    ctx.content_rect().center().y - 275.0
                ))
                .show(ctx, |ui| {
                    ui::AboutPanel::render(ui);

                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.services.show_about_modal = false;
                            }
                        });
                    });
                });
        }

        if self.services.show_kicad_version_modal {
            egui::Window::new("KiCad Information")
                .collapsible(false)
                .resizable(false)
                .default_pos(egui::pos2(
                    ctx.content_rect().center().x - 200.0,
                    ctx.content_rect().center().y - 150.0
                ))
                .show(ctx, |ui| {
                    self.render_kicad_info_modal(ui);

                    ui.add_space(20.0);
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Close").clicked() {
                                self.services.show_kicad_version_modal = false;
                            }
                        });
                    });
                });
        }

        if ctx.input(|i| i.time) % 30.0 < 0.1 {
            self.save_dock_state();
        }
    }
}
