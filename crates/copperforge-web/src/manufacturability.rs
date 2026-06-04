//! v1 manufacturability index — a transparent, weighted 0..=100 score
//! from part-commonality + assembly factors you already parse from the
//! BOM. Higher = easier to assemble.
//!
//! Deliberately a plain weighted blend (not ML, not a GA): a DFM number
//! people make fab decisions on should be explainable. The weights and
//! knees below are **v1 placeholders to calibrate against real boards.**
//!
//! This whole `score` body is the planned swap point for a v2 Mamdani
//! **fuzzy inference** system (membership functions + an IF-THEN rule
//! base + centroid defuzzification) — same inputs, same output type, so
//! the call sites don't change. Build that engine generic and the same
//! machinery scores *reliability* (derating / thermal / stress) too.

/// Result of the manufacturability heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Manufacturability {
    /// 0..=100, higher = easier to assemble.
    pub score: u32,
    /// Letter grade derived from `score` (A best … F worst).
    pub grade: char,
}

/// Score a board from commonality + assembly inputs.
///
/// - `unique_ratio`  — unique parts / total placements (`0..=1`, lower better)
/// - `tht_fraction`  — through-hole fraction of placements (`0..=1`, lower better)
/// - `total_parts`   — placed-component count (run length / complexity)
pub fn score(unique_ratio: f32, tht_fraction: f32, total_parts: usize) -> Manufacturability {
    // Each sub-factor is a 0 (easy) .. 1 (hard) difficulty contribution.
    let f_common = unique_ratio.clamp(0.0, 1.0); // feeder count / low reuse
    let f_tht = tht_fraction.clamp(0.0, 1.0); // hand / selective solder
    let f_size = (total_parts as f32 / 1000.0).min(1.0); // run length, soft-capped at 1k

    // Commonality dominates; THT is a meaningful penalty; size is minor.
    let difficulty = (0.55 * f_common + 0.30 * f_tht + 0.15 * f_size).clamp(0.0, 1.0);
    let score = (100.0 * (1.0 - difficulty)).round() as u32;

    let grade = match score {
        85..=100 => 'A',
        70..=84 => 'B',
        55..=69 => 'C',
        40..=54 => 'D',
        _ => 'F',
    };
    Manufacturability { score, grade }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn high_reuse_all_smt_scores_well() {
        // Lots of reuse (low ratio), no THT, modest size → easy.
        let m = score(0.1, 0.0, 200);
        assert!(m.score >= 85, "got {}", m.score);
        assert_eq!(m.grade, 'A');
    }

    #[test]
    fn every_part_unique_and_through_hole_scores_poorly() {
        // Ratio 1.0 (no reuse) + all THT → hard.
        let m = score(1.0, 1.0, 200);
        assert!(m.score <= 20, "got {}", m.score);
        assert_eq!(m.grade, 'F');
    }

    #[test]
    fn monotonic_in_commonality() {
        // More reuse never scores worse.
        assert!(score(0.2, 0.1, 300).score >= score(0.8, 0.1, 300).score);
    }
}
