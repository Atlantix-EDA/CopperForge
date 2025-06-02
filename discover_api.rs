use std::io::BufReader;
use gerber_viewer::gerber_parser::{parse, GerberDoc};

fn main() {
    let demo_str = include_str!("assets/demo.gbr").as_bytes();
    let reader = BufReader::new(demo_str);
    let doc = parse(reader).unwrap();
    
    // Try to call methods to see what's available
    println!("Doc type: {:?}", std::any::type_name::<GerberDoc>());
    
    // These will help us discover the API
    let commands = doc.into_commands();
    println!("Commands type: {:?}", std::any::type_name_of_val(&commands));
    
    // Try to see what's in a command
    if let Some(first_command) = commands.first() {
        println!("First command type: {:?}", std::any::type_name_of_val(first_command));
    }
}