use crate::platform::{banner, details};
use egui_lens::ReactiveEventLogger;

/// Initialize and display application banner and system information
pub fn initialize_and_show_banner(
    logger: &ReactiveEventLogger,
) {
    // Create and format banner
    let mut app_banner = banner::Banner::new();
    app_banner.format();
    
    // Create and get system details
    let mut system_details = details::Details::new();
    system_details.get_os();
    
    // Log system details first (will appear below in reverse chronological view)
    let details_text = system_details.format_os();
    logger.log_info(&details_text);
    
    // Log banner message last (will appear on top in reverse chronological view)
    logger.log_info(&app_banner.message);
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