//! MO-aware unit normalization for parsed gerber command streams.
//!
//! `gerber_viewer` renders in raw file units and never applies the MO
//! (mode) parameter — an inch-mode file (`%MOIN*%`) draws 25.4× too small
//! next to metric layers, and its bounding box gets read as mm by every
//! downstream consumer (dimension readout, DRC, export). Rewriting the
//! command stream to mm once at load keeps the whole 2D pipeline mm-only,
//! matching the 3D extractors (which apply the unit scale during their own
//! walk of the raw file).
//!
//! Scaled: draw/flash coordinates and arc offsets, standard aperture
//! dimensions, macro-body dimension fields, macro AD parameters that feed
//! dimension fields, SR step distances, and deprecated OF image offsets.
//! Not scaled: angles, exposure flags, vertex counts, LS/SF scale factors.
//! The MO command itself is rewritten to mm so the stream stays
//! self-consistent if re-serialized.

use std::collections::{HashMap, HashSet};

use gerber_types::{
    Aperture, ApertureMacro, Command, CoordinateNumber, CoordinateOffset, Coordinates, DCode,
    ExtendedCode, FunctionCode, MacroBoolean, MacroContent, MacroDecimal, MacroInteger, Operation,
    StepAndRepeat, Unit,
};

const MM_PER_INCH: f64 = 25.4;

/// Take a parsed doc's declared units + command stream and return the
/// stream with all dimensional data expressed in mm. Metric docs pass
/// through untouched; docs with no MO at all are assumed mm (same policy
/// as the 3D extractors).
pub fn commands_in_mm(units: Option<Unit>, mut commands: Vec<Command>) -> Vec<Command> {
    if units != Some(Unit::Inches) {
        return commands;
    }
    log::info!("gerber declares inch units (MOIN) — normalizing to mm");

    // Pass 1: learn which `$n` variables each macro uses as dimensions vs.
    // angles/flags. AD parameters substitute into `$n` positionally and the
    // AD line itself carries no type info — a parameter feeding a diameter
    // must be scaled, one feeding a rotation must not.
    let mut roles: HashMap<String, VarRoles> = HashMap::new();
    for cmd in &mut commands {
        if let Command::ExtendedCode(ExtendedCode::ApertureMacro(am)) = cmd {
            let entry = roles.entry(am.name.clone()).or_default();
            visit_macro(am, |field| match field {
                MacroField::Dim(d) => match d {
                    MacroDecimal::Variable(n) => {
                        entry.dims.insert(*n);
                    }
                    MacroDecimal::Expression(_) => entry.expressions = true,
                    MacroDecimal::Value(_) => {}
                },
                MacroField::Other(var) => {
                    entry.other.insert(var);
                }
                MacroField::Unscalable => entry.expressions = true,
            });
        }
    }

    // Pass 2: scale everything in place.
    for cmd in &mut commands {
        scale_command(cmd, &roles);
    }
    commands
}

/// Which `$n` variables a macro uses as dimensions vs. anything else.
#[derive(Default)]
struct VarRoles {
    dims: HashSet<u32>,
    other: HashSet<u32>,
    /// Body contains arithmetic expressions or `$n=` variable definitions
    /// we can't statically scale — flagged so the load log shows it.
    expressions: bool,
}

/// One classified slot in a macro body, as seen by `visit_macro`.
enum MacroField<'a> {
    /// A length-typed field (coordinate, diameter, width…): scale it.
    Dim(&'a mut MacroDecimal),
    /// A non-length field (angle, exposure, vertex count) referencing `$n`.
    Other(u32),
    /// Something we can't scale statically (variable definition body).
    Unscalable,
}

/// Walk every field of every primitive in a macro body exactly once,
/// classified by role. Single source of truth for which fields are
/// dimensions — used by both the role-collection and scaling passes.
fn visit_macro(am: &mut ApertureMacro, mut f: impl FnMut(MacroField)) {
    for content in &mut am.content {
        match content {
            MacroContent::Circle(c) => {
                f(MacroField::Dim(&mut c.diameter));
                f(MacroField::Dim(&mut c.center.0));
                f(MacroField::Dim(&mut c.center.1));
                if let Some(a) = &c.angle {
                    note_other_decimal(a, &mut f);
                }
            }
            MacroContent::VectorLine(v) => {
                f(MacroField::Dim(&mut v.width));
                f(MacroField::Dim(&mut v.start.0));
                f(MacroField::Dim(&mut v.start.1));
                f(MacroField::Dim(&mut v.end.0));
                f(MacroField::Dim(&mut v.end.1));
                note_other_decimal(&v.angle, &mut f);
            }
            MacroContent::CenterLine(c) => {
                f(MacroField::Dim(&mut c.dimensions.0));
                f(MacroField::Dim(&mut c.dimensions.1));
                f(MacroField::Dim(&mut c.center.0));
                f(MacroField::Dim(&mut c.center.1));
                note_other_decimal(&c.angle, &mut f);
            }
            MacroContent::Outline(o) => {
                for (x, y) in &mut o.points {
                    f(MacroField::Dim(x));
                    f(MacroField::Dim(y));
                }
                note_other_decimal(&o.angle, &mut f);
            }
            MacroContent::Polygon(p) => {
                f(MacroField::Dim(&mut p.center.0));
                f(MacroField::Dim(&mut p.center.1));
                f(MacroField::Dim(&mut p.diameter));
                if let MacroInteger::Variable(n) = &p.vertices {
                    f(MacroField::Other(*n));
                }
                note_other_decimal(&p.angle, &mut f);
            }
            MacroContent::Moire(m) => {
                f(MacroField::Dim(&mut m.center.0));
                f(MacroField::Dim(&mut m.center.1));
                f(MacroField::Dim(&mut m.diameter));
                f(MacroField::Dim(&mut m.ring_thickness));
                f(MacroField::Dim(&mut m.gap));
                f(MacroField::Dim(&mut m.cross_hair_thickness));
                f(MacroField::Dim(&mut m.cross_hair_length));
                note_other_decimal(&m.angle, &mut f);
            }
            MacroContent::Thermal(t) => {
                f(MacroField::Dim(&mut t.center.0));
                f(MacroField::Dim(&mut t.outer_diameter));
                f(MacroField::Dim(&mut t.inner_diameter));
                f(MacroField::Dim(&mut t.gap));
                note_other_decimal(&t.angle, &mut f);
            }
            MacroContent::VariableDefinition(_) => f(MacroField::Unscalable),
            MacroContent::Comment(_) => {}
        }
        // Exposure flags are booleans; a `$n` there must not be scaled.
        match content {
            MacroContent::Circle(c) => note_other_bool(&c.exposure, &mut f),
            MacroContent::VectorLine(v) => note_other_bool(&v.exposure, &mut f),
            MacroContent::CenterLine(c) => note_other_bool(&c.exposure, &mut f),
            MacroContent::Outline(o) => note_other_bool(&o.exposure, &mut f),
            MacroContent::Polygon(p) => note_other_bool(&p.exposure, &mut f),
            _ => {}
        }
    }
}

fn note_other_decimal(d: &MacroDecimal, f: &mut impl FnMut(MacroField)) {
    if let MacroDecimal::Variable(n) = d {
        f(MacroField::Other(*n));
    }
}

fn note_other_bool(b: &MacroBoolean, f: &mut impl FnMut(MacroField)) {
    if let MacroBoolean::Variable(n) = b {
        f(MacroField::Other(*n));
    }
}

fn scale_command(cmd: &mut Command, roles: &HashMap<String, VarRoles>) {
    match cmd {
        Command::FunctionCode(FunctionCode::DCode(DCode::Operation(op))) => match op {
            Operation::Interpolate(coords, offset) => {
                if let Some(c) = coords {
                    scale_coordinates(c);
                }
                if let Some(o) = offset {
                    scale_offset(o);
                }
            }
            Operation::Move(coords) | Operation::Flash(coords) => {
                if let Some(c) = coords {
                    scale_coordinates(c);
                }
            }
        },
        Command::ExtendedCode(ec) => match ec {
            ExtendedCode::Unit(u) => *u = Unit::Millimeters,
            ExtendedCode::ApertureDefinition(ad) => scale_aperture(&mut ad.aperture, roles),
            ExtendedCode::ApertureMacro(am) => {
                visit_macro(am, |field| {
                    if let MacroField::Dim(MacroDecimal::Value(v)) = field {
                        *v *= MM_PER_INCH;
                    }
                });
            }
            ExtendedCode::StepAndRepeat(StepAndRepeat::Open {
                distance_x,
                distance_y,
                ..
            }) => {
                *distance_x *= MM_PER_INCH;
                *distance_y *= MM_PER_INCH;
            }
            ExtendedCode::OffsetImage(off) => {
                off.a *= MM_PER_INCH;
                off.b *= MM_PER_INCH;
            }
            _ => {}
        },
        _ => {}
    }
}

fn scale_aperture(aperture: &mut Aperture, roles: &HashMap<String, VarRoles>) {
    match aperture {
        Aperture::Circle(c) => {
            c.diameter *= MM_PER_INCH;
            if let Some(h) = &mut c.hole_diameter {
                *h *= MM_PER_INCH;
            }
        }
        Aperture::Rectangle(r) | Aperture::Obround(r) => {
            r.x *= MM_PER_INCH;
            r.y *= MM_PER_INCH;
            if let Some(h) = &mut r.hole_diameter {
                *h *= MM_PER_INCH;
            }
        }
        Aperture::Polygon(p) => {
            p.diameter *= MM_PER_INCH;
            if let Some(h) = &mut p.hole_diameter {
                *h *= MM_PER_INCH;
            }
        }
        Aperture::Macro(name, args) => {
            let Some(roles) = roles.get(name) else {
                log::warn!("inch gerber: AD references undefined macro `{name}` — parameters left unscaled");
                return;
            };
            if roles.expressions {
                log::warn!(
                    "inch gerber: macro `{name}` uses arithmetic expressions — \
                     values derived from them stay in inches"
                );
            }
            let Some(args) = args else { return };
            // AD parameters are positional: the i-th parameter is `$(i+1)`.
            for (i, arg) in args.iter_mut().enumerate() {
                let var = i as u32 + 1;
                let is_dim = roles.dims.contains(&var);
                if is_dim && roles.other.contains(&var) {
                    log::warn!(
                        "inch gerber: macro `{name}` uses ${var} as both a dimension \
                         and an angle/flag — scaling it as a dimension"
                    );
                }
                if is_dim {
                    if let MacroDecimal::Value(v) = arg {
                        *v *= MM_PER_INCH;
                    }
                }
            }
        }
    }
}

fn scale_coordinates(c: &mut Coordinates) {
    scale_cn(&mut c.x);
    scale_cn(&mut c.y);
}

fn scale_offset(o: &mut CoordinateOffset) {
    scale_cn(&mut o.x);
    scale_cn(&mut o.y);
}

fn scale_cn(n: &mut Option<CoordinateNumber>) {
    if let Some(cn) = n {
        let raw: f64 = (*cn).into();
        match CoordinateNumber::try_from(raw * MM_PER_INCH) {
            Ok(scaled) => *cn = scaled,
            // Only reachable on overflow of the fixed-point range; keeping
            // the raw value beats poisoning the stream with a zero.
            Err(_) => log::warn!("inch gerber: coordinate {raw} overflows when scaled to mm"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gerber_parser::parse;
    use std::io::BufReader;

    fn parse_str(src: &str) -> (Option<Unit>, Vec<Command>) {
        let doc = match parse(BufReader::new(src.as_bytes())) {
            Ok(d) => d,
            Err((d, _)) => d,
        };
        (doc.units, doc.into_commands())
    }

    /// A 1.575 × 3.543 inch board outline in MOIN mode must come out as a
    /// 40.0 × 90.0 mm bbox (the slide-3 board that exposed the bug).
    #[test]
    fn inch_outline_normalizes_to_mm() {
        let src = "%FSLAX26Y26*%\n%MOIN*%\n%ADD10C,0.004*%\nD10*\nX0Y0D02*\nX1575000Y0D01*\nX1575000Y3543000D01*\nX0Y3543000D01*\nX0Y0D01*\nM02*\n";
        let (units, commands) = parse_str(src);
        assert_eq!(units, Some(Unit::Inches));
        let commands = commands_in_mm(units, commands);

        let layer = gerber_viewer::GerberLayer::new(commands);
        let bbox = layer.bounding_box();
        assert!(
            (bbox.width() - 40.005).abs() < 0.11,
            "width {} not ~40 mm",
            bbox.width()
        );
        assert!(
            (bbox.height() - 90.0).abs() < 0.2,
            "height {} not ~90 mm",
            bbox.height()
        );
    }

    #[test]
    fn metric_passes_through_untouched() {
        let src = "%FSLAX46Y46*%\n%MOMM*%\n%ADD10C,0.1*%\nD10*\nX0Y0D02*\nX40000000Y0D01*\nM02*\n";
        let (units, commands) = parse_str(src);
        let before = commands.clone();
        let after = commands_in_mm(units, commands);
        assert_eq!(before, after);
    }

    /// The MO command itself must be rewritten so the normalized stream is
    /// self-consistent if re-serialized.
    #[test]
    fn mo_rewritten_to_mm() {
        let src = "%FSLAX26Y26*%\n%MOIN*%\nM02*\n";
        let (units, commands) = parse_str(src);
        let commands = commands_in_mm(units, commands);
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::ExtendedCode(ExtendedCode::Unit(Unit::Millimeters))
        )));
        assert!(!commands.iter().any(|c| matches!(
            c,
            Command::ExtendedCode(ExtendedCode::Unit(Unit::Inches))
        )));
    }

    /// Macro AD parameters: `$1`/`$2` feed a rectangle's width/height
    /// (scaled), `$3` feeds the rotation (left alone).
    #[test]
    fn macro_args_scaled_by_role() {
        let src = "%FSLAX26Y26*%\n%MOIN*%\n%AMROTRECT*21,1,$1,$2,0,0,$3*%\n%ADD10ROTRECT,0.1X0.05X45*%\nD10*\nX0Y0D03*\nM02*\n";
        let (units, commands) = parse_str(src);
        let commands = commands_in_mm(units, commands);

        let ad = commands
            .iter()
            .find_map(|c| match c {
                Command::ExtendedCode(ExtendedCode::ApertureDefinition(ad)) => Some(ad),
                _ => None,
            })
            .expect("AD present");
        let Aperture::Macro(_, Some(args)) = &ad.aperture else {
            panic!("macro aperture with args expected, got {:?}", ad.aperture);
        };
        let vals: Vec<f64> = args
            .iter()
            .map(|a| match a {
                MacroDecimal::Value(v) => *v,
                other => panic!("literal args expected, got {other:?}"),
            })
            .collect();
        assert!((vals[0] - 2.54).abs() < 1e-9, "width {} not scaled", vals[0]);
        assert!((vals[1] - 1.27).abs() < 1e-9, "height {} not scaled", vals[1]);
        assert!((vals[2] - 45.0).abs() < 1e-9, "angle {} must not scale", vals[2]);
    }

    /// Literal dimensions inside a macro body are scaled; literal angles
    /// are not.
    #[test]
    fn macro_body_literals_scaled() {
        let src = "%FSLAX26Y26*%\n%MOIN*%\n%AMFIXED*21,1,0.1,0.05,0,0,30*%\n%ADD10FIXED*%\nD10*\nX0Y0D03*\nM02*\n";
        let (units, commands) = parse_str(src);
        let commands = commands_in_mm(units, commands);

        let am = commands
            .iter()
            .find_map(|c| match c {
                Command::ExtendedCode(ExtendedCode::ApertureMacro(am)) => Some(am),
                _ => None,
            })
            .expect("AM present");
        let MacroContent::CenterLine(cl) = &am.content[0] else {
            panic!("centerline expected, got {:?}", am.content[0]);
        };
        let MacroDecimal::Value(w) = cl.dimensions.0 else { panic!() };
        let MacroDecimal::Value(angle) = cl.angle else { panic!() };
        assert!((w - 2.54).abs() < 1e-9, "width {w} not scaled");
        assert!((angle - 30.0).abs() < 1e-9, "angle {angle} must not scale");
    }
}
