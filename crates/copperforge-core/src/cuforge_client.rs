//! Client for the private `cuforge-services` backend.
//!
//! On app startup a background thread pings `/health` and updates
//! `Dynamic<CuforgeStatus>`; the ribbon renders a small indicator off
//! that reactive cell. The indicator is clickable — opens a details
//! modal with URL, version, last error (if any), and a Recheck-now
//! button.
//!
//! Service URL precedence:
//!   1. environment variable `CUFORGE_SERVICES_URL` (any scheme)
//!   2. default `http://127.0.0.1:8421`
//!
//! Everything here is intentionally tiny — no async runtime, no
//! framework, just `ureq` on a worker thread and a `Dynamic` for the
//! result.

use std::thread;
use std::time::Duration;

use egui_mobius_reactive::Dynamic;
use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8421";
const ENV_VAR: &str = "CUFORGE_SERVICES_URL";
/// Poll cadence when the service responded last time — just tracking
/// liveness, no rush.
const POLL_INTERVAL_CONNECTED: Duration = Duration::from_secs(30);
/// Poll cadence when the service is down — fast so the badge flips
/// green within a few seconds of the user starting the server.
const POLL_INTERVAL_DISCONNECTED: Duration = Duration::from_secs(3);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Latest known state of the cuforge-services backend.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CuforgeStatus {
    /// Initial state before the first ping completes.
    Unknown,
    /// A health check is in flight.
    Checking,
    /// Server responded; carries the service's reported version and the
    /// list of capabilities (features) the server advertised.
    Connected { version: String, features: Vec<Feature> },
    /// Server unreachable or returned an error; carries a short reason.
    Disconnected { reason: String },
}

/// A capability advertised by cuforge-services in its `/health` response.
/// `enabled` reflects the server-side entitlement state — `false` items
/// may be coming-soon, gated by tier, or both.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

/// `/health` response shape from cuforge-services.
#[derive(Deserialize)]
struct HealthResponse {
    #[allow(dead_code)]
    status: String,
    version: String,
    /// Server-advertised capabilities. `#[serde(default)]` so older
    /// server versions (without this field) still parse cleanly to an
    /// empty list.
    #[serde(default)]
    features: Vec<Feature>,
}

/// Resolve the cuforge-services base URL: env var if set, else default.
pub fn base_url() -> String {
    std::env::var(ENV_VAR).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string())
}

/// Spawn a background poller that updates `status` with the result of
/// `GET {base_url}/health` immediately, then every `POLL_INTERVAL`.
/// Calls `ctx.request_repaint()` on every status change so the UI
/// refreshes without waiting for the next user input.
pub fn spawn_health_poller(
    base_url: String,
    status: Dynamic<CuforgeStatus>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));

        // First ping only: signal Checking so the UI shows immediate
        // feedback on startup. Subsequent polls skip the Checking flash
        // so the badge doesn't twitch yellow every few seconds while
        // offline — explicit user rechecks (check_now) still show it.
        status.set(CuforgeStatus::Checking);
        ctx.request_repaint();

        loop {
            let new_status = do_health_check(&agent, &health_url);
            let is_connected = matches!(new_status, CuforgeStatus::Connected { .. });

            // Only push + repaint on actual state change. Steady-state
            // polling is invisible.
            if status.get() != new_status {
                status.set(new_status);
                ctx.request_repaint();
            }

            thread::sleep(if is_connected {
                POLL_INTERVAL_CONNECTED
            } else {
                POLL_INTERVAL_DISCONNECTED
            });
        }
    });
}

/// One-shot health check, used by the modal's "Recheck now" button.
/// Independent of (and races harmlessly with) the periodic poller.
pub fn check_now(base_url: String, status: Dynamic<CuforgeStatus>, ctx: egui::Context) {
    thread::spawn(move || {
        let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
        let health_url = format!("{}/health", base_url.trim_end_matches('/'));
        status.set(CuforgeStatus::Checking);
        ctx.request_repaint();
        let new_status = do_health_check(&agent, &health_url);
        status.set(new_status);
        ctx.request_repaint();
    });
}

/// Single check against the configured health endpoint. Shared by the
/// periodic poller and the on-demand `check_now`.
fn do_health_check(agent: &ureq::Agent, health_url: &str) -> CuforgeStatus {
    match agent.get(health_url).call() {
        Ok(resp) => match resp.into_json::<HealthResponse>() {
            Ok(h) => CuforgeStatus::Connected {
                version: h.version,
                features: h.features,
            },
            Err(e) => CuforgeStatus::Disconnected {
                reason: format!("bad response: {e}"),
            },
        },
        Err(e) => CuforgeStatus::Disconnected {
            reason: short_error(&e),
        },
    }
}

/// Render the ribbon status indicator. Returns the response so the
/// caller can detect a click and open the details modal.
pub fn show_status_indicator(
    ui: &mut egui::Ui,
    status: &Dynamic<CuforgeStatus>,
) -> egui::Response {
    let s = status.get();
    let (color, label) = match &s {
        CuforgeStatus::Unknown => (
            egui::Color32::GRAY,
            "Services: —".to_string(),
        ),
        CuforgeStatus::Checking => (
            egui::Color32::from_rgb(220, 180, 80),
            "Services: checking…".to_string(),
        ),
        CuforgeStatus::Connected { version, .. } => (
            egui::Color32::from_rgb(120, 200, 120),
            format!("CuForge Services v{version}"),
        ),
        CuforgeStatus::Disconnected { .. } => (
            // Muted/translucent red — for most users "offline" is the
            // normal state (no subscription), not an error, so saturated
            // red mis-signals. Softer alpha reads as "neutral state".
            egui::Color32::from_rgba_unmultiplied(200, 130, 130, 170),
            "Services: offline".to_string(),
        ),
    };
    let tooltip = match &s {
        CuforgeStatus::Unknown => {
            "Health check not yet performed\n(click for details)".to_string()
        }
        CuforgeStatus::Checking => format!(
            "Pinging cuforge-services at {}/health\n(click for details)",
            base_url()
        ),
        CuforgeStatus::Connected { version, .. } => format!(
            "Connected to cuforge-services v{version}\nURL: {}\n(click for details)",
            base_url()
        ),
        CuforgeStatus::Disconnected { reason } => format!(
            "Cannot reach cuforge-services\nReason: {reason}\nURL: {}\n(click for details)",
            base_url()
        ),
    };
    // Render as a proper button so it matches the other ribbon modal
    // triggers (CopperForge version, KiCad version). The colored text
    // carries the status at a glance; the painted dot lives in the
    // modal where there's room for it.
    ui.button(egui::RichText::new(label).color(color))
        .on_hover_text(tooltip)
}

/// Details modal — opened by clicking the ribbon indicator. Shows the
/// resolved URL, current connection state, service version (or
/// disconnect reason), and a Recheck-now button.
pub fn show_modal_if_open(
    ctx: &egui::Context,
    open: &mut bool,
    status: &Dynamic<CuforgeStatus>,
) {
    if !*open {
        return;
    }
    let s = status.get();
    let mut should_recheck = false;

    egui::Window::new("CuForge Services")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(420.0)
        .default_pos(egui::pos2(
            ctx.content_rect().center().x - 210.0,
            ctx.content_rect().center().y - 140.0,
        ))
        .show(ctx, |ui| {
            ui.add_space(8.0);

            // Big status row (dot + heading) at the top.
            ui.horizontal(|ui| {
                let (color, status_text) = match &s {
                    CuforgeStatus::Unknown => (egui::Color32::GRAY, "Unknown"),
                    CuforgeStatus::Checking => {
                        (egui::Color32::from_rgb(220, 180, 80), "Checking…")
                    }
                    CuforgeStatus::Connected { .. } => {
                        (egui::Color32::from_rgb(120, 200, 120), "Connected")
                    }
                    CuforgeStatus::Disconnected { .. } => {
                        // Muted red — see show_status_indicator for the why.
                        (egui::Color32::from_rgba_unmultiplied(200, 130, 130, 170), "Offline")
                    }
                };
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(22.0, ui.spacing().interact_size.y),
                    egui::Sense::hover(),
                );
                ui.painter().circle_filled(rect.center(), 9.0, color);
                ui.heading(egui::RichText::new(status_text).color(color));
            });
            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            egui::Grid::new("cuforge_services_info_grid")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    ui.label("URL:");
                    ui.label(egui::RichText::new(base_url()).monospace());
                    ui.end_row();

                    if let CuforgeStatus::Connected { version, .. } = &s {
                        ui.label("Service version:");
                        ui.label(egui::RichText::new(version).monospace());
                        ui.end_row();
                    }

                    if let CuforgeStatus::Disconnected { reason } = &s {
                        ui.label("Reason:");
                        ui.label(reason);
                        ui.end_row();
                    }
                });

            // Server-advertised capability catalog. Read-only checkboxes
            // (rendered disabled so the user can't toggle them) — the
            // server is the source of truth for what's enabled.
            if let CuforgeStatus::Connected { features, .. } = &s {
                if !features.is_empty() {
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Services").strong());
                    ui.add_space(4.0);
                    for feature in features {
                        let mut enabled = feature.enabled;
                        ui.add_enabled(
                            false,
                            egui::Checkbox::new(&mut enabled, &feature.name),
                        );
                    }
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "Unchecked items are not enabled in your tier, or are coming soon.",
                        )
                        .small()
                        .weak(),
                    );
                }
            }

            if matches!(&s, CuforgeStatus::Disconnected { .. }) {
                ui.add_space(10.0);
                ui.label(egui::RichText::new("To start the server:").small().italics());
                ui.label(
                    egui::RichText::new("  cd cuforge-services && cargo run")
                        .monospace()
                        .small(),
                );
            }

            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Override URL via env var: CUFORGE_SERVICES_URL")
                    .small()
                    .weak(),
            );

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui.button("🔄 Recheck now").clicked() {
                    should_recheck = true;
                }
            });
        });

    if should_recheck {
        check_now(base_url(), status.clone(), ctx.clone());
    }
}

/// Short, human-friendly error string for the status tooltip.
fn short_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("HTTP {code}"),
        ureq::Error::Transport(t) => {
            let s = t.to_string();
            s.split_once(':').map(|(head, _)| head.to_string()).unwrap_or(s)
        }
    }
}
