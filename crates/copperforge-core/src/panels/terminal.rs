//! Terminal panel — OS shell augmented with CopperForge commands.
//!
//! The first token of each line decides the dispatch:
//!
//! * Known forge sub-command (e.g. `new-project`, `list-projects`, `status`,
//!   `ver`, `info`, `env`, `sh`) → parsed by `clap` and run against the
//!   app's state.
//! * Anything else (e.g. `ls`, `git status`, `kicad-cli …`) → handed to
//!   `bash -c` verbatim.
//!
//! That keeps the user's muscle-memory ("terminal = bash") intact while
//! letting project management happen in the same line. If a forge command
//! shadows a bash binary you actually want, `sh <cmd>` is the explicit
//! escape (e.g. `sh status` → bash's `status`, not forge's).
//!
//! `clear` / `cls` clears the panel-local log and is handled before
//! dispatch — it's a UI action, not a command.

use clap::{CommandFactory, Parser, Subcommand};
use egui::RichText;
use egui_citizen::{Citizen, CitizenId, CitizenState};

use super::citizen_panel;
use crate::services::SharedServices;
use crate::theme::TokyoNight;

const PROMPT: &str = "$ ";
const INPUT_ID: &str = "terminal_input";

citizen_panel!(TerminalPanel, "terminal",
    output: Vec<String> = Vec::new(),
    cmd_buf: String = String::new()
);

impl TerminalPanel {
    pub fn show(&mut self, ui: &mut egui::Ui, services: &mut SharedServices) {
        let frame = egui::Frame::new()
            .fill(TokyoNight::BG_DARK)
            .inner_margin(8.0);

        frame.show(ui, |ui| {
            ui.style_mut().visuals.extreme_bg_color = TokyoNight::BG_DARK;

            let text_id = ui.make_persistent_id(INPUT_ID);

            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in self.output.iter() {
                        render_line(ui, line);
                    }

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(
                            RichText::new(PROMPT)
                                .color(TokyoNight::GREEN)
                                .monospace()
                                .strong(),
                        );
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.cmd_buf)
                                .id(text_id)
                                .desired_width(ui.available_width())
                                .font(egui::TextStyle::Monospace)
                                .frame(egui::Frame::NONE)
                                .text_color(TokyoNight::FG),
                        );

                        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let input = self.cmd_buf.trim().to_string();
                            if !input.is_empty() {
                                if input == "clear" || input == "cls" {
                                    self.output.clear();
                                } else {
                                    self.output.push(format!("{PROMPT}{input}"));
                                    for line in execute_command(&input, services) {
                                        self.output.push(line);
                                    }
                                }
                            }
                            self.cmd_buf.clear();
                            ui.memory_mut(|m| m.request_focus(text_id));
                        }

                        if !response.has_focus() && !ui.ctx().wants_keyboard_input() {
                            ui.memory_mut(|m| m.request_focus(text_id));
                        }
                    });
                });
        });
    }
}

fn render_line(ui: &mut egui::Ui, line: &str) {
    let color = line_color(line);
    ui.label(RichText::new(line).color(color).monospace());
}

// ────────────────────────────────────────────────────────────────────────
// clap grammar
// ────────────────────────────────────────────────────────────────────────

/// `no_binary_name = true` tells clap the first token is the subcommand.
/// `disable_help_subcommand` keeps clap from injecting its own `help` —
/// we route `help` / `?` to our own handler so the overview is curated.
#[derive(Parser)]
#[command(
    name = "forge",
    no_binary_name = true,
    disable_help_subcommand = true,
    disable_version_flag = true,
    help_template = "{about-section}{usage-heading} {usage}\n\n{all-args}",
    about = "CopperForge terminal: forge subcommands + bash passthrough."
)]
struct ForgeCli {
    #[command(subcommand)]
    cmd: ForgeCommand,
}

#[derive(Subcommand)]
enum ForgeCommand {
    /// Show CopperForge version
    #[command(alias = "version")]
    Ver,

    /// Show host OS / CPU / memory / network
    #[command(aliases = ["system", "sysinfo"])]
    Info,

    /// Show current project + PCB + layers
    #[command(alias = "state")]
    Status,

    /// Show discovered KiCad + relevant env vars
    Env,

    /// Scaffold a new KiCad project under the config-supplied dir
    #[command(aliases = ["newproject", "new"])]
    NewProject {
        /// Project name. Letters, digits, '.', '_', '-' (not leading).
        name: String,
        /// Parent directory for the new project. `~` expands to $HOME.
        #[arg(short = 'p', long = "path", value_name = "DIR")]
        path: Option<String>,
    },

    /// List projects registered in CopperForge's DB
    #[command(aliases = ["ls-projects", "projects"])]
    ListProjects,

    /// Remove a project from the DB. Matches id-prefix then exact name.
    #[command(aliases = ["rm-project", "del-project"])]
    DeleteProject {
        /// Project id (prefix match ok) or exact project name.
        query: String,
    },

    /// Force bash interpretation (useful when a forge name shadows a bin).
    Sh {
        /// Command and its arguments, passed verbatim to `bash -c`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

/// All tokens we treat as forge sub-commands. Anything else falls through
/// to `bash -c`. Mirrors the clap derive above; keep the two lists in sync.
fn is_known_forge_command(tok: &str) -> bool {
    matches!(
        tok,
        "ver" | "version"
            | "info" | "system" | "sysinfo"
            | "status" | "state"
            | "env"
            | "new-project" | "newproject" | "new"
            | "list-projects" | "ls-projects" | "projects"
            | "delete-project" | "rm-project" | "del-project"
            | "sh"
    )
}

// ────────────────────────────────────────────────────────────────────────
// Dispatch
// ────────────────────────────────────────────────────────────────────────

fn execute_command(input: &str, services: &SharedServices) -> Vec<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let first = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_lowercase();

    // Help shortcuts. clap also supports `<cmd> --help` per sub-command, but
    // this first-word shortcut gives an overview of everything.
    if matches!(first.as_str(), "help" | "?" | "-h" | "--help") {
        return help_lines();
    }

    // Unknown first token → plain bash. Keeps `ls`, `git status`,
    // `kicad-cli pcb export …` working without any prefix.
    if !is_known_forge_command(&first) {
        return run_os_command(trimmed);
    }

    // Known forge command: tokenize quote-aware, then let clap parse.
    let tokens: Vec<String> = match shlex::split(trimmed) {
        Some(t) => t,
        None => return vec!["error: unbalanced quotes".into()],
    };

    match ForgeCli::try_parse_from(&tokens) {
        Ok(cli) => dispatch(cli.cmd, services),
        Err(e) => render_clap_error(e),
    }
}

fn dispatch(cmd: ForgeCommand, services: &SharedServices) -> Vec<String> {
    match cmd {
        ForgeCommand::Ver => version_lines(),
        ForgeCommand::Info => info_lines(),
        ForgeCommand::Status => status_lines(services),
        ForgeCommand::Env => env_lines(services),
        ForgeCommand::NewProject { name, path } => {
            if !is_safe_project_name(&name) {
                return vec![format!(
                    "error: invalid project name '{}'. Use letters, digits, '.', '_', '-' (not leading); no '/', '\\', or '..'.",
                    name
                )];
            }
            let override_path = path.map(expand_path);
            new_project(&name, override_path.as_deref(), services)
        }
        ForgeCommand::ListProjects => list_projects(services),
        ForgeCommand::DeleteProject { query } => delete_project(&query, services),
        ForgeCommand::Sh { args } => {
            if args.is_empty() {
                vec!["Usage: sh <command> [args...]".into()]
            } else {
                run_os_command(&args.join(" "))
            }
        }
    }
}

/// Convert a clap error into plain-text lines. Help / version output is
/// relayed as-is; real errors are prefixed `error:` if clap didn't.
fn render_clap_error(err: clap::Error) -> Vec<String> {
    use clap::error::ErrorKind;
    let is_help_or_version = matches!(
        err.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand,
    );
    let rendered = err.render().to_string();
    let stripped = strip_ansi(&rendered);
    let mut lines: Vec<String> = stripped.lines().map(|s| s.to_string()).collect();
    if !is_help_or_version {
        if let Some(first) = lines.iter().position(|l| !l.trim().is_empty()) {
            if !lines[first].to_lowercase().contains("error") {
                lines[first] = format!("error: {}", lines[first]);
            }
        }
    }
    lines
}

/// Drop CSI escape sequences (`ESC [ … letter`) from clap's colored output.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.next() == Some('[') {
                for inner in chars.by_ref() {
                    if inner.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Hand-rolled help
// ────────────────────────────────────────────────────────────────────────

fn help_lines() -> Vec<String> {
    let mut out = vec![
        "CopperForge terminal.".into(),
        "  Any command not listed below is forwarded to `bash -c` —".into(),
        "  type `ls`, `git status`, `kicad-cli …` just like a normal shell.".into(),
        String::new(),
    ];
    let mut app = ForgeCli::command();
    let rendered = app.render_long_help().to_string();
    for line in strip_ansi(&rendered).lines() {
        out.push(line.to_string());
    }
    out.push(String::new());
    out.push("Built-in UI actions:".into());
    out.push("  clear, cls            Clear the terminal log".into());
    out.push("  help, ?               Show this overview".into());
    out
}

// ────────────────────────────────────────────────────────────────────────
// Subcommand handlers
// ────────────────────────────────────────────────────────────────────────

fn version_lines() -> Vec<String> {
    let mut banner = crate::platform::banner::Banner::new();
    banner.format();
    banner
        .message
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn info_lines() -> Vec<String> {
    let mut details = crate::platform::details::Details::new();
    details.format_os().lines().map(|s| s.to_string()).collect()
}

fn env_lines(services: &SharedServices) -> Vec<String> {
    let mut lines = vec!["KiCad:".into()];
    lines.push(format!(
        "  version          = {}",
        services.kicad_version.as_deref().unwrap_or("(not detected)")
    ));

    let mut kicad_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(k, _)| k.starts_with("KICAD"))
        .collect();
    kicad_vars.sort_by(|a, b| a.0.cmp(&b.0));
    if kicad_vars.is_empty() {
        lines.push("  env vars         = (none set — typical when KiCad runs via Flatpak sandbox)".into());
    } else {
        for (k, v) in kicad_vars {
            lines.push(format!("  {k} = {v}"));
        }
    }

    lines.push(String::new());
    lines.push("Host:".into());
    for var in ["USER", "HOME", "SHELL", "PWD"] {
        let val = std::env::var(var).unwrap_or_else(|_| "(not set)".into());
        lines.push(format!("  {var:<16} = {val}"));
    }
    lines
}

fn status_lines(services: &SharedServices) -> Vec<String> {
    let mut lines = vec!["CopperForge Status:".into()];
    lines.push(format!(
        "  PCB file        = {}",
        services
            .project_state
            .get()
            .pcb_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(none)".into())
    ));
    lines.push(format!(
        "  Gerber layers   = {}",
        services.layer_store.layers.len()
    ));
    lines.push(format!(
        "  BOM components  = {}",
        services.bom_component_count
    ));
    lines.push(format!(
        "  Units           = {}",
        if services.global_units_mils { "mils" } else { "mm" }
    ));
    lines.push(format!(
        "  KiCad           = {}",
        services.kicad_version.as_deref().unwrap_or("(not detected)")
    ));
    lines
}

fn list_projects(services: &SharedServices) -> Vec<String> {
    match services.project_db.list_projects() {
        Ok(projects) if projects.is_empty() => vec!["(no projects registered)".into()],
        Ok(projects) => {
            let id_width = projects.iter().map(|p| p.id.len()).max().unwrap_or(0);
            let mut lines = vec![format!("Projects ({}):", projects.len())];
            for p in &projects {
                lines.push(format!(
                    "  {:<id_width$}  {:<24}  {}",
                    p.id,
                    p.name,
                    p.pcb_file_path.display(),
                    id_width = id_width,
                ));
            }
            lines
        }
        Err(e) => vec![format!("error: failed to list projects: {}", e)],
    }
}

fn delete_project(query: &str, services: &SharedServices) -> Vec<String> {
    let projects = match services.project_db.list_projects() {
        Ok(p) => p,
        Err(e) => return vec![format!("error: could not enumerate projects: {}", e)],
    };
    let id_matches: Vec<_> = projects.iter().filter(|p| p.id.starts_with(query)).collect();
    let name_matches: Vec<_> = projects.iter().filter(|p| p.name == query).collect();
    let matches: Vec<_> = if !id_matches.is_empty() { id_matches } else { name_matches };
    match matches.len() {
        0 => vec![format!("error: no project matches '{}'. Try `list-projects`.", query)],
        1 => {
            let p = matches[0];
            match services.project_db.delete_project(&p.id) {
                Ok(()) => vec![
                    format!("Deleted from DB: {} ({})", p.name, p.id),
                    format!("  PCB path was: {}", p.pcb_file_path.display()),
                    "Files on disk were NOT removed. Clean up with `rm -rf -- <dir>` if wanted.".into(),
                ],
                Err(e) => vec![format!("error: delete failed: {}", e)],
            }
        }
        n => {
            let mut lines = vec![format!(
                "error: '{}' is ambiguous — matched {} projects. Use a longer id prefix:",
                query, n
            )];
            for p in matches {
                lines.push(format!("  {}  {}", p.id, p.name));
            }
            lines
        }
    }
}

fn new_project(
    name: &str,
    path_override: Option<&std::path::Path>,
    services: &SharedServices,
) -> Vec<String> {
    use crate::project_manager::kicad_project::{create_kicad_project, NewKicadProjectInfo};
    use crate::project_manager::kicad_global_libs::setup_kiverse_globally;
    use crate::project_manager::database::{generate_project_id, ProjectData, ProjectMetadata};

    let name = name.trim().to_string();
    if name.is_empty() {
        return vec!["error: project name cannot be empty".into()];
    }

    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let location = match path_override {
        Some(p) => p.to_path_buf(),
        None => default_projects_dir(services),
    };
    let kiverse_path = home.join("kiverse");

    if services.config.include_kiverse || services.config.include_atlantix_resistors {
        let kiverse_opt = if kiverse_path.exists() { Some(kiverse_path.clone()) } else { None };
        if let Err(e) = setup_kiverse_globally(kiverse_opt) {
            eprintln!("Warning: kiverse global setup failed: {}", e);
        }
    }

    let mut info = NewKicadProjectInfo::new(name.clone(), location.clone());
    info.author = services.config.default_author.clone();
    info.company = services.config.default_company.clone();
    info.include_kiverse = false;
    info.include_atlantix_resistors = false;
    info.kiverse_path = Some(kiverse_path);

    let project_dir = match create_kicad_project(&info) {
        Ok(dir) => dir,
        Err(e) => return vec![format!("error: failed to create KiCad project: {}", e)],
    };

    let now = chrono::Utc::now();
    let metadata = ProjectMetadata {
        id: generate_project_id(),
        name: name.clone(),
        description: String::new(),
        pcb_file_path: info.pcb_file_path(),
        created_at: now,
        last_modified: now,
        version: env!("CARGO_PKG_VERSION").to_string(),
        tags: Vec::new(),
        parent_id: None,
    };
    let data = ProjectData {
        metadata,
        bom_components: Vec::new(),
        notes: String::new(),
        releases: Vec::new(),
        hierarchy: None,
    };
    if let Err(e) = services.project_db.save_project(&data) {
        return vec![
            format!("warning: KiCad files created at {} but DB save failed: {}", project_dir.display(), e),
        ];
    }

    vec![
        format!("Created: {}", project_dir.display()),
        format!("  .kicad_pro = {}", info.project_file_path().display()),
        format!("  .kicad_sch = {}", info.schematic_file_path().display()),
        format!("  .kicad_pcb = {}", info.pcb_file_path().display()),
        "Registered in CopperForge project DB.".into(),
    ]
}

// ────────────────────────────────────────────────────────────────────────
// Helpers
// ────────────────────────────────────────────────────────────────────────

fn default_projects_dir(services: &SharedServices) -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    services
        .config
        .preferred_projects_directory
        .clone()
        .unwrap_or_else(|| home.join("projects"))
}

fn expand_path(s: String) -> std::path::PathBuf {
    if s == "~" {
        return dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    }
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    std::path::PathBuf::from(s)
}

fn is_safe_project_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.starts_with('-') || name.starts_with('.') {
        return false;
    }
    !name.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
}

fn run_os_command(cmd: &str) -> Vec<String> {
    match std::process::Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .output()
    {
        Ok(output) => {
            let mut lines = Vec::new();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stdout.lines() {
                lines.push(line.to_string());
            }
            for line in stderr.lines() {
                lines.push(format!("error: {line}"));
            }
            if lines.is_empty() && !output.status.success() {
                lines.push(format!("error: exit code {}", output.status));
            }
            lines
        }
        Err(e) => vec![format!("error: {e}")],
    }
}

fn line_color(line: &str) -> egui::Color32 {
    if line.starts_with("error:") || line.starts_with("ERROR") {
        TokyoNight::RED
    } else if line.starts_with(PROMPT) {
        TokyoNight::GREEN
    } else if line.starts_with("  ") && line.contains('=') {
        TokyoNight::FG_DIM
    } else if line.starts_with("  ") {
        TokyoNight::FG_DIM
    } else if line.ends_with(':') {
        TokyoNight::GREEN
    } else {
        TokyoNight::FG
    }
}
