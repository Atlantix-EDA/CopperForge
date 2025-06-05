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
    
    // Log banner message (welcome message)
    logger.log_info(&app_banner.message);
    
    // Log system details
    let details_text = system_details.format_os();
    logger.log_info(&details_text);
}

/// Show system information on demand
pub fn show_system_info(logger: &ReactiveEventLogger) {
    let mut system_details = details::Details::new();
    system_details.get_os();
    
    // Display system details first
    let details_text = system_details.format_os();
    logger.log_info(&details_text);
    
    // Then display banner (so it appears above the details in the log)
    let mut app_banner = banner::Banner::new();
    app_banner.format();
    logger.log_info(&app_banner.message);
}