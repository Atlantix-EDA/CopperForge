// Test program to setup KiVerse global libraries
use std::path::PathBuf;

fn main() {
    let kiverse_path = dirs::home_dir()
        .map(|h| h.join(".kicad_libs/kiverse"))
        .filter(|p| p.exists());

    println!("Testing global library setup...");
    println!("KiVerse path: {:?}", kiverse_path);

    // This would normally be done by including the copperforge-core crate
    // For now, we'll just show what the command would be
    println!("\nTo test manually, run CopperForge and create a new project.");
    println!("The KiVerse libraries should be added to your global KiCad config automatically.");
    println!("\nYou can verify by checking:");
    println!("  ~/.config/kicad/9.99/sym-lib-table");
    println!("  ~/.config/kicad/9.99/fp-lib-table");
}
