use crate::platform::parameters::gui;

#[derive(Default, Debug)]
pub struct Banner {
    pub message: String,
}

impl Banner {
    pub fn new() -> Banner {
        Banner {
            message: String::new(),
        }
    }

    pub fn format(&mut self) {
        self.message = format!("\n**** Welcome to {}, Version {}", gui::APPLICATION_NAME, gui::VERSION);
        self.message += &format!("\n**** Today is {}", chrono::Utc::now().format("%m-%d-%Y %H:%M:%S"));
        
        // Add dependencies information
        self.message += "\n\nDEPENDENCIES";
        self.message += &format!("\nCopperForge      : {}", gui::VERSION);
        self.message += &format!("\ngerber_viewer    : {}", env!("GERBER_VIEWER_VERSION"));
        self.message += &format!("\ngerber_types     : {}", env!("GERBER_TYPES_VERSION"));
        self.message += &format!("\ngerber_parser    : {}\n", env!("GERBER_PARSER_VERSION"));
    }

    #[allow(dead_code)]
    pub fn print(&mut self) {
        println!("{}", self.message);
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_banner() {
        let mut banner = super::Banner::new();
        banner.format();
        banner.print();
    }
}
