use crate::platform::{banner, details};
use crate::event_logger::ReactiveEventLogger;

/// Emit just the app banner to the logger — the FSM's Banner phase.
pub fn show_banner(logger: &ReactiveEventLogger) {
    let mut app_banner = banner::Banner::new();
    app_banner.format();
    logger.log_info(&app_banner.message);
}

/// Emit just the OS / system details (from `platform::details`) to the
/// logger — the FSM's SystemInfo phase.
pub fn show_system_details(logger: &ReactiveEventLogger) {
    let mut system_details = details::Details::new();
    system_details.get_os();
    logger.log_info(&system_details.format_os());
}

/// Show system information on demand
pub fn show_system_info(logger: &ReactiveEventLogger) {
    // Log system details FIRST (will appear BELOW in circular buffer)
    let mut system_details = details::Details::new();
    system_details.get_os();
    let details_text = system_details.format_os();
    logger.log_info(&details_text);
    
    // Log banner with dependencies LAST (will appear ON TOP in circular buffer)
    let mut app_banner = banner::Banner::new();
    app_banner.format();
    logger.log_info(&app_banner.message);
}