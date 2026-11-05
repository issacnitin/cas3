/*
* Reference simulator for table-defined rules.
*
* Deliberately naive "gather" formulation (look at the neighbours of every candidate cell using
* coordinate vectors and ordered sets) so that it shares no code or encoding tricks with the
* optimised "scatter" simulation inside the search engine. Tests compare the two, and every
* solution the CLI reports is re-simulated here before it is printed.
*/

use std::collections::{BTreeSet, HashMap};

use crate::lattice::Neighbourhood;
use crate::table_search::{Solution, SurvivalMode};

pub type Config = BTreeSet<Vec<i32>>;

/// A rule as two predicates over neighbour masks (bit j = neighbour `offsets[j]` is set).
pub trait RuleFn {
    fn birth(&self, mask: u64) -> bool;
    fn survive(&self, mask: u64) -> bool;
}

pub fn origin_config(dim: usize) -> Config {
    let mut config = Config::new();
    config.insert(vec![0; dim]);
    config
}

fn shifted(cell: &[i32], offset: &[i32], sign: i32) -> Vec<i32> {
    cell.iter().zip(offset).map(|(x, d)| x + sign * d).collect()
}

pub fn reference_step<R: RuleFn + ?Sized>(nb: &Neighbourhood, rule: &R, current: &Config) -> Config {
    // Cells that can change: the set cells and every cell that has a set neighbour.
    let mut candidates: Config = current.clone();
    for cell in current {
        for offset in &nb.offsets {
            candidates.insert(shifted(cell, offset, -1));
        }
    }
    let mut next = Config::new();
    for cell in candidates {
        let mut mask = 0u64;
        for (j, offset) in nb.offsets.iter().enumerate() {
            if current.contains(&shifted(&cell, offset, 1)) {
                mask |= 1 << j;
            }
        }
        let alive = if current.contains(&cell) {
            rule.survive(mask)
        } else {
            rule.birth(mask)
        };
        if alive {
            next.insert(cell);
        }
    }
    next
}

/// Configurations at t = 0..=steps starting from a single set cell at the origin.
pub fn reference_states<R: RuleFn + ?Sized>(nb: &Neighbourhood, rule: &R, steps: usize) -> Vec<Config> {
    let mut states = vec![origin_config(nb.dim)];
    for _ in 0..steps {
        let next = reference_step(nb, rule, states.last().unwrap());
        states.push(next);
    }
    states
}

pub fn reference_counts<R: RuleFn + ?Sized>(nb: &Neighbourhood, rule: &R, steps: usize) -> Vec<usize> {
    reference_states(nb, rule, steps).iter().map(|c| c.len()).collect()
}

/// True when the set-cell counts at t = 0, 1, ... equal `target`; stops at the first mismatch.
pub fn reference_matches<R: RuleFn + ?Sized>(nb: &Neighbourhood, rule: &R, target: &[usize]) -> bool {
    let mut current = origin_config(nb.dim);
    if target.first() != Some(&current.len()) {
        return false;
    }
    for &expected in &target[1..] {
        current = reference_step(nb, rule, &current);
        if current.len() != expected {
            return false;
        }
    }
    true
}

pub fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A solution's assigned entries; unassigned patterns are 0, or pseudo-random for a given seed.
pub struct SolutionRule {
    birth: HashMap<u64, bool>,
    survive: HashMap<u64, bool>,
    mode: SurvivalMode,
    seed: Option<u64>,
}

impl SolutionRule {
    pub fn new(solution: &Solution, mode: SurvivalMode, seed: Option<u64>) -> Self {
        SolutionRule {
            birth: solution.birth.iter().copied().collect(),
            survive: solution.survival.iter().copied().collect(),
            mode,
            seed,
        }
    }

    fn fill(&self, salt: u64, mask: u64) -> bool {
        match self.seed {
            None => false,
            Some(seed) => splitmix64(seed ^ salt.wrapping_mul(0xD6E8_FEB8_6659_FD93) ^ mask) & 1 == 1,
        }
    }
}

impl RuleFn for SolutionRule {
    fn birth(&self, mask: u64) -> bool {
        self.birth.get(&mask).copied().unwrap_or_else(|| self.fill(1, mask))
    }

    fn survive(&self, mask: u64) -> bool {
        match self.mode {
            SurvivalMode::Always => true,
            SurvivalMode::Never => false,
            SurvivalMode::Free => self.survive.get(&mask).copied().unwrap_or_else(|| self.fill(2, mask)),
        }
    }
}

/// Re-simulates a solution with the reference engine: once with unassigned patterns off and
/// `completions` times with them random. Every completion must reproduce `target`.
pub fn verify_solution(
    nb: &Neighbourhood,
    target: &[usize],
    solution: &Solution,
    mode: SurvivalMode,
    completions: u64,
) -> bool {
    if !reference_matches(nb, &SolutionRule::new(solution, mode, None), target) {
        return false;
    }
    (0..completions).all(|seed| reference_matches(nb, &SolutionRule::new(solution, mode, Some(seed + 1)), target))
}

pub fn pattern_to_string(nb: &Neighbourhood, mask: u64) -> String {
    let parts: Vec<String> = nb
        .offsets
        .iter()
        .enumerate()
        .filter(|(j, _)| (mask >> j) & 1 == 1)
        .map(|(_, o)| {
            let coords: Vec<String> = o.iter().map(|x| format!("{:+}", x)).collect();
            format!("({})", coords.join(","))
        })
        .collect();
    format!("{{{}}}", parts.join(" "))
}

/// Space-time diagram for one dimension, grid snapshots for two, counts only beyond that.
pub fn render_evolution(states: &[Config]) -> String {
    let dim = states.first().and_then(|c| c.iter().next()).map(|c| c.len()).unwrap_or(0);
    let mut out = String::new();
    let all: Vec<&Vec<i32>> = states.iter().flat_map(|c| c.iter()).collect();
    if all.is_empty() {
        return out;
    }
    let lo = |axis: usize| all.iter().map(|c| c[axis]).min().unwrap();
    let hi = |axis: usize| all.iter().map(|c| c[axis]).max().unwrap();
    match dim {
        1 => {
            for (t, config) in states.iter().enumerate() {
                let row: String = (lo(0)..=hi(0))
                    .map(|x| if config.contains(&vec![x]) { '#' } else { '.' })
                    .collect();
                out.push_str(&format!("t={:<3} {}  ({})\n", t, row, config.len()));
            }
        }
        2 => {
            for (t, config) in states.iter().enumerate() {
                out.push_str(&format!("t={} count={}\n", t, config.len()));
                for y in (lo(1)..=hi(1)).rev() {
                    let row: String = (lo(0)..=hi(0))
                        .map(|x| if config.contains(&vec![x, y]) { '#' } else { '.' })
                        .collect();
                    out.push_str(&format!("  {}\n", row));
                }
            }
        }
        _ => {
            let counts: Vec<String> = states.iter().map(|c| c.len().to_string()).collect();
            out.push_str(&format!("counts: {}\n", counts.join(" ")));
        }
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;

    struct Fixed {
        birth: Vec<u64>,
        survive: Vec<u64>,
    }

    impl RuleFn for Fixed {
        fn birth(&self, mask: u64) -> bool {
            self.birth.contains(&mask)
        }
        fn survive(&self, mask: u64) -> bool {
            self.survive.contains(&mask)
        }
    }

    #[test]
    fn one_dimensional_growth_to_the_right() {
        // Neighbour bit 0 is +1, bit 1 is -1. A cell is born when its left neighbour (-1, bit 1) is set.
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let rule = Fixed { birth: vec![0b10], survive: vec![0b00, 0b01, 0b10, 0b11] };
        assert_eq!(reference_counts(&nb, &rule, 5), vec![1, 2, 3, 4, 5, 6]);
        assert!(reference_matches(&nb, &rule, &[1, 2, 3, 4]));
        assert!(!reference_matches(&nb, &rule, &[1, 2, 4]));
        assert!(!reference_matches(&nb, &rule, &[2, 3]));
    }

    #[test]
    fn rule_90_makes_sierpinski_counts() {
        // XOR of the two neighbours, no self dependence: counts are 2^popcount(t).
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let rule = Fixed { birth: vec![0b01, 0b10], survive: vec![0b01, 0b10] };
        assert_eq!(reference_counts(&nb, &rule, 8), vec![1, 2, 2, 4, 2, 4, 4, 8, 2]);
    }

    #[test]
    fn evolution_renders_for_one_and_two_dimensions() {
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let rule = Fixed { birth: vec![0b10], survive: vec![0, 1, 2, 3] };
        let text = render_evolution(&reference_states(&nb, &rule, 2));
        assert!(text.contains("t=2   ###  (3)"));

        let nb2 = Neighbourhood::moore(2, 1).unwrap();
        let stay = Fixed { birth: vec![], survive: vec![0] };
        assert!(render_evolution(&reference_states(&nb2, &stay, 1)).contains("t=1 count=1"));
    }

    #[test]
    fn pattern_strings_list_offsets() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        assert_eq!(pattern_to_string(&nb, 0), "{}");
        assert_eq!(pattern_to_string(&nb, 0b101), "{(+1,+0) (+0,+1)}");
    }
}
