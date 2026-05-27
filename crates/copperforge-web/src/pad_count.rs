//! Count SMT pads on each side of the board.
//!
//! Method: parse the F.Paste / B.Paste gerber layers and count
//! D03 (`Operation::Flash`) commands. Each flash on a paste layer
//! corresponds to one paste-stencil opening, which is one SMT pad.
//! Same metric PCBWay / JLCPCB use when quoting SMT assembly.
//!
//! Thermal pads with cross-hatched paste (multiple openings for one
//! electrical pad) will inflate the count — acceptable here because
//! assembly cost scales with placements, not pads, and SMT-pad count
//! is the published quote-form metric.

use std::collections::BTreeMap;
use std::io::BufReader;

use gerber_viewer::gerber_parser::parse;
use gerber_viewer::gerber_types::{Command, DCode, FunctionCode, Operation};

#[derive(Debug, Clone, Copy, Default)]
pub struct SmtPadCount {
    pub top: usize,
    pub bottom: usize,
}

impl SmtPadCount {
    pub fn total(&self) -> usize {
        self.top + self.bottom
    }

    pub fn any(&self) -> bool {
        self.top > 0 || self.bottom > 0
    }
}

/// Scan the uploaded entries for F.Paste / B.Paste gerbers and count
/// flashes on each. Names matched loosely (`f_paste`, `f.paste`, etc.)
/// so projects with non-default suffixes still work. Returns zero
/// counts when no paste layer is present — `any()` lets the panel
/// decide whether to render the row.
pub fn count_from_entries(entries: &BTreeMap<String, Vec<u8>>) -> SmtPadCount {
    let mut counts = SmtPadCount::default();
    for (name, bytes) in entries {
        let lower = name.to_lowercase();
        if !lower.ends_with(".gbr") {
            continue;
        }
        let is_top = lower.contains("f_paste") || lower.contains("f.paste");
        let is_bot = lower.contains("b_paste") || lower.contains("b.paste");
        if !(is_top || is_bot) {
            continue;
        }

        let reader = BufReader::new(bytes.as_slice());
        let doc = match parse(reader) {
            Ok(d) => d,
            Err((_partial, e)) => {
                log::warn!("Paste gerber {} failed to parse: {}", name, e);
                continue;
            }
        };

        let n = doc
            .commands()
            .iter()
            .filter(|cmd| {
                matches!(
                    cmd,
                    Command::FunctionCode(FunctionCode::DCode(DCode::Operation(
                        Operation::Flash(_)
                    )))
                )
            })
            .count();

        if is_top {
            counts.top += n;
        } else {
            counts.bottom += n;
        }
        log::info!("SMT pads from {}: {}", name, n);
    }
    counts
}
