//! One-off: parse a gerber and report command count + parse error, to see
//! where third-party exports lose geometry.
use std::io::BufReader;

fn main() {
    env_logger::init();
    let path = std::env::args().nth(1).expect("usage: gbr_debug <file>");
    let bytes = std::fs::read(&path).expect("read");
    let (doc, err) = match gerber_parser::parse(BufReader::new(bytes.as_slice())) {
        Ok(d) => (d, None),
        Err((d, e)) => (d, Some(e)),
    };
    println!("units: {:?}", doc.units);
    let cmds = doc.into_commands();
    println!("commands parsed: {}", cmds.len());
    for c in cmds.iter().take(12) {
        println!("  {:?}", c);
    }
    match copperforge_core::gerber_geom::extract_outline_from_bytes(&bytes) {
        Some((o, c)) => println!("outline OK: {} contours, bbox {:?}, counts {:?}", o.contours.len(), o.bbox, c),
        None => println!("outline: NONE"),
    }
    if let Some(e) = err {
        println!("PARSE ERROR: {e:?}");
    }
}

// quick outline probe appended for debugging
