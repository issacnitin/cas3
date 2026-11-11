/*
* Dimension sweep for symmetric rule families, in any dimension.
*
* The main engine stores a cell's pattern as a 64-bit mask, so it stops at 62 neighbours (the Moore
* neighbourhood of radius 1 in 3D has 26, in 4D already 80) and its symmetry group at dimension 5.
* Symmetric families do not need masks: a totalistic rule only reads the number of set neighbours,
* and an isotropic von Neumann rule only reads, per axis, whether zero, one or both of the two axis
* neighbours are set, up to permuting the axes. So both keys are computed directly here, with cells
* packed into 128-bit integers, which reaches every dimension up to 21.
*
* By the pair reduction and the parity lemma (src/bounds.rs) a run 2, 3, 5, ... from any finite
* starting configuration passes through a pair {-u, u} with u in the neighbourhood (plus the cell)
* scaled by one half of an even offset, i.e. u with integer coordinates in [-r, r]. Up to the
* symmetries of the lattice only the sorted absolute values of u matter, so a family is settled in a
* dimension by one lazy search per such u. This module is independent of the main engine (no shared
* simulation code) and is cross-checked against it in low dimensions.
*/

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

const BITS: usize = 6;
const SHIFT: i128 = 1 << (BITS - 1);
pub const MAX_SWEEP_DIM: usize = 128 / BITS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    /// Only the number of set cells in the Moore ball of this radius counts (Life-like).
    MooreTotalistic(i32),
    /// Only the number of set axis neighbours counts.
    VonNeumannTotalistic,
    /// Per axis none, one or both neighbours set, up to permuting the axes (full isotropy).
    VonNeumannIsotropic,
}

impl Family {
    pub fn name(&self) -> String {
        match self {
            Family::MooreTotalistic(1) => "Moore totalistic (Life-like)".to_string(),
            Family::MooreTotalistic(r) => format!("Moore radius {} totalistic", r),
            Family::VonNeumannTotalistic => "von Neumann totalistic".to_string(),
            Family::VonNeumannIsotropic => "von Neumann isotropic".to_string(),
        }
    }

    fn radius(&self) -> i32 {
        match self {
            Family::MooreTotalistic(r) => *r,
            _ => 1,
        }
    }

    /// Neighbour offsets of the family in `dim` dimensions.
    pub fn offsets(&self, dim: usize) -> Vec<Vec<i32>> {
        match self {
            Family::MooreTotalistic(r) => {
                let side = (2 * r + 1) as usize;
                let total = side.pow(dim as u32);
                (0..total)
                    .map(|mut index| {
                        (0..dim)
                            .map(|_| {
                                let digit = (index % side) as i32 - r;
                                index /= side;
                                digit
                            })
                            .collect::<Vec<i32>>()
                    })
                    .filter(|o| o.iter().any(|&x| x != 0))
                    .collect()
            }
            _ => (0..dim)
                .flat_map(|axis| {
                    [1, -1].into_iter().map(move |sign| {
                        let mut o = vec![0; dim];
                        o[axis] = sign;
                        o
                    })
                })
                .collect(),
        }
    }

    /// Number of table entries of the family (birth without the empty pattern, plus survival).
    pub fn entries(&self, dim: usize) -> u64 {
        match self {
            Family::MooreTotalistic(r) => {
                let k = ((2 * r + 1) as u64).pow(dim as u32) - 1;
                2 * k + 1
            }
            Family::VonNeumannTotalistic => 4 * dim as u64 + 1,
            Family::VonNeumannIsotropic => {
                let classes = ((dim + 1) * (dim + 2) / 2) as u64;
                2 * classes - 1
            }
        }
    }

    /// Half offsets u of the centred pairs {-u, u} up to symmetry: sorted absolute values.
    pub fn centred_pairs(&self, dim: usize) -> Vec<Vec<i32>> {
        match self {
            Family::MooreTotalistic(r) => {
                // Non-increasing sequences of values in 0..=r, not all zero.
                let mut out = vec![];
                let mut current = vec![];
                fn rec(dim: usize, max: i32, current: &mut Vec<i32>, out: &mut Vec<Vec<i32>>) {
                    if current.len() == dim {
                        if current.iter().any(|&x| x != 0) {
                            out.push(current.clone());
                        }
                        return;
                    }
                    for v in (0..=max).rev() {
                        current.push(v);
                        rec(dim, v, current, out);
                        current.pop();
                    }
                }
                rec(dim, *r, &mut current, &mut out);
                out
            }
            _ => {
                let mut u = vec![0; dim];
                u[0] = 1;
                vec![u]
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct SweepResult {
    pub dim: usize,
    pub family: Family,
    pub entries: u64,
    pub pairs: usize,
    /// Terms of the target checked (shortened when the radius would leave the coordinate range).
    pub terms_checked: usize,
    /// Longest prefix of the target reproduced from any centred pair (the pair itself counts).
    pub best_run: usize,
    pub best_pair: Vec<i32>,
    pub nodes: u64,
    pub complete: bool,
    pub elapsed: Duration,
}

struct Searcher<'a> {
    family: Family,
    dim: usize,
    deltas: Vec<i128>,
    axis_deltas: Vec<(usize, bool, i128)>,
    target: &'a [usize],
    rule: HashMap<(bool, u64), bool>,
    best: usize,
    nodes: u64,
    node_limit: u64,
}

fn encode(coordinates: &[i32]) -> i128 {
    coordinates
        .iter()
        .enumerate()
        .map(|(i, &x)| (x as i128 + SHIFT) << (BITS * i))
        .sum()
}

fn delta(offset: &[i32]) -> i128 {
    offset
        .iter()
        .enumerate()
        .map(|(i, &x)| (x as i128) << (BITS * i))
        .sum()
}

impl<'a> Searcher<'a> {
    /// The cells that may change, grouped by (is set, key).
    fn classify(&self, live: &HashSet<i128>) -> HashMap<(bool, u64), Vec<i128>> {
        let mut keys: HashMap<i128, u64> = HashMap::new();
        match self.family {
            Family::VonNeumannIsotropic => {
                // Bit 2 * axis + side: the neighbour on that side of that axis is set.
                let mut seen: HashMap<i128, u64> = HashMap::new();
                for &cell in live {
                    seen.entry(cell).or_insert(0);
                    for &(axis, positive, d) in &self.axis_deltas {
                        *seen.entry(cell - d).or_insert(0) |=
                            1u64 << (2 * axis + positive as usize);
                    }
                }
                for (cell, bits) in seen {
                    let mut one = 0u64;
                    let mut both = 0u64;
                    for axis in 0..self.dim {
                        match (bits >> (2 * axis)) & 3 {
                            3 => both += 1,
                            0 => {}
                            _ => one += 1,
                        }
                    }
                    keys.insert(cell, one * 64 + both);
                }
            }
            _ => {
                for &cell in live {
                    keys.entry(cell).or_insert(0);
                    for &d in &self.deltas {
                        *keys.entry(cell - d).or_insert(0) += 1;
                    }
                }
            }
        }
        let mut groups: HashMap<(bool, u64), Vec<i128>> = HashMap::new();
        for (cell, key) in keys {
            groups
                .entry((live.contains(&cell), key))
                .or_default()
                .push(cell);
        }
        groups
    }

    fn visit(&mut self, t: usize, live: &HashSet<i128>) {
        self.nodes += 1;
        self.best = self.best.max(t + 1);
        if t + 1 == self.target.len() || self.nodes >= self.node_limit {
            return;
        }
        let groups = self.classify(live);
        let mut known = 0usize;
        let mut open: Vec<((bool, u64), usize)> = vec![];
        for (key, cells) in &groups {
            match self.rule.get(key) {
                Some(true) => known += cells.len(),
                Some(false) => {}
                None => open.push((*key, cells.len())),
            }
        }
        let target = self.target[t + 1];
        if known > target {
            return;
        }
        open.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut suffix = vec![0usize; open.len() + 1];
        for i in (0..open.len()).rev() {
            suffix[i] = suffix[i + 1] + open[i].1;
        }
        let mut chosen = vec![false; open.len()];
        self.choose(t, &groups, &open, &suffix, 0, target - known, &mut chosen);
    }

    #[allow(clippy::too_many_arguments)]
    fn choose(
        &mut self,
        t: usize,
        groups: &HashMap<(bool, u64), Vec<i128>>,
        open: &[((bool, u64), usize)],
        suffix: &[usize],
        i: usize,
        remaining: usize,
        chosen: &mut Vec<bool>,
    ) {
        if self.nodes >= self.node_limit {
            return;
        }
        if remaining == 0 {
            for (j, (key, _)) in open.iter().enumerate() {
                self.rule.insert(*key, j < i && chosen[j]);
            }
            let next: HashSet<i128> = groups
                .iter()
                .filter(|(key, _)| self.rule[*key])
                .flat_map(|(_, cells)| cells.iter().copied())
                .collect();
            self.visit(t + 1, &next);
            for (key, _) in open {
                self.rule.remove(key);
            }
            return;
        }
        if i == open.len() || suffix[i] < remaining {
            return;
        }
        if open[i].1 <= remaining {
            chosen[i] = true;
            self.choose(
                t,
                groups,
                open,
                suffix,
                i + 1,
                remaining - open[i].1,
                chosen,
            );
            chosen[i] = false;
        }
        self.choose(t, groups, open, suffix, i + 1, remaining, chosen);
    }
}

/// Exhaustive any-start search of one family in one dimension (up to `node_limit` nodes).
/// `target` must start with 2 (the pair).
pub fn sweep(
    family: Family,
    dim: usize,
    target: &[usize],
    node_limit: u64,
) -> Result<SweepResult, String> {
    if dim == 0 || dim > MAX_SWEEP_DIM {
        return Err(format!(
            "the sweep supports dimensions 1 to {}",
            MAX_SWEEP_DIM
        ));
    }
    if target.first() != Some(&2) {
        return Err("the sweep target starts with the pair: 2, 3, ...".to_string());
    }
    // Cells stay within r (seed) + r per step of the origin; keep that inside the coordinate range.
    let r = family.radius() as i128;
    let fitting = ((SHIFT - 1 - r) / r - 1).max(0) as usize;
    if fitting < 2 {
        return Err(format!(
            "radius {} does not fit in {} bits per coordinate",
            r, BITS
        ));
    }
    let target = &target[..target.len().min(fitting)];
    let started = Instant::now();
    let offsets = family.offsets(dim);
    let deltas: Vec<i128> = offsets.iter().map(|o| delta(o)).collect();
    let axis_deltas: Vec<(usize, bool, i128)> = (0..dim)
        .flat_map(|axis| {
            [true, false].into_iter().map(move |positive| {
                let mut o = vec![0; dim];
                o[axis] = if positive { 1 } else { -1 };
                (axis, positive, delta(&o))
            })
        })
        .collect();
    let pairs = family.centred_pairs(dim);
    let mut result = SweepResult {
        dim,
        family,
        entries: family.entries(dim),
        pairs: pairs.len(),
        terms_checked: target.len(),
        best_run: 0,
        best_pair: vec![],
        nodes: 0,
        complete: true,
        elapsed: Duration::ZERO,
    };
    for u in pairs {
        let negated: Vec<i32> = u.iter().map(|x| -x).collect();
        let live: HashSet<i128> = [encode(&u), encode(&negated)].into_iter().collect();
        let mut searcher = Searcher {
            family,
            dim,
            deltas: deltas.clone(),
            axis_deltas: axis_deltas.clone(),
            target,
            rule: HashMap::new(),
            best: 0,
            nodes: 0,
            node_limit: node_limit.saturating_sub(result.nodes),
        };
        searcher.visit(0, &live);
        result.nodes += searcher.nodes;
        if searcher.nodes >= searcher.node_limit {
            result.complete = false;
        }
        if searcher.best > result.best_run {
            result.best_run = searcher.best;
            result.best_pair = u.clone();
        }
    }
    result.elapsed = started.elapsed();
    Ok(result)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::analysis::any_start_search;
    use crate::lattice::Neighbourhood;
    use crate::rule_class::RuleClass;
    use crate::sequences::parse_target_from;
    use crate::table_search::SearchParams;

    fn engine_best(nb: Neighbourhood, class: RuleClass, terms: usize) -> usize {
        let mut params = SearchParams::new(nb, parse_target_from("primes", terms, 2).unwrap());
        params.class = class;
        params.parallel = false;
        params.store_limit = 0;
        let runs = any_start_search(&params, |_| {}).unwrap();
        assert!(runs.iter().all(|r| r.report.complete()));
        runs.iter().map(|r| r.report.matched_terms()).max().unwrap()
    }

    #[test]
    fn family_sizes_and_pairs() {
        assert_eq!(Family::MooreTotalistic(1).offsets(4).len(), 80);
        assert_eq!(Family::MooreTotalistic(1).entries(2), 17);
        assert_eq!(Family::VonNeumannIsotropic.entries(3), 19);
        assert_eq!(Family::VonNeumannIsotropic.entries(5), 41);
        assert_eq!(
            Family::MooreTotalistic(1).centred_pairs(3),
            vec![vec![1, 1, 1], vec![1, 1, 0], vec![1, 0, 0]]
        );
        assert_eq!(Family::MooreTotalistic(2).centred_pairs(2).len(), 5);
        assert_eq!(
            Family::VonNeumannIsotropic.centred_pairs(7),
            vec![vec![1, 0, 0, 0, 0, 0, 0]]
        );
    }

    #[test]
    fn sweep_agrees_with_the_main_engine_in_low_dimensions() {
        let terms = 12;
        let target = parse_target_from("primes", terms, 2).unwrap();
        let cases: Vec<(Family, Neighbourhood, RuleClass)> = vec![
            (
                Family::MooreTotalistic(1),
                Neighbourhood::moore(2, 1).unwrap(),
                RuleClass::Totalistic,
            ),
            (
                Family::MooreTotalistic(1),
                Neighbourhood::moore(3, 1).unwrap(),
                RuleClass::Totalistic,
            ),
            (
                Family::MooreTotalistic(2),
                Neighbourhood::moore(2, 2).unwrap(),
                RuleClass::Totalistic,
            ),
            (
                Family::VonNeumannTotalistic,
                Neighbourhood::von_neumann(2).unwrap(),
                RuleClass::Totalistic,
            ),
            (
                Family::VonNeumannTotalistic,
                Neighbourhood::von_neumann(4).unwrap(),
                RuleClass::Totalistic,
            ),
            (
                Family::VonNeumannIsotropic,
                Neighbourhood::von_neumann(2).unwrap(),
                RuleClass::Isotropic,
            ),
            (
                Family::VonNeumannIsotropic,
                Neighbourhood::von_neumann(3).unwrap(),
                RuleClass::Isotropic,
            ),
            (
                Family::VonNeumannIsotropic,
                Neighbourhood::von_neumann(5).unwrap(),
                RuleClass::Isotropic,
            ),
        ];
        for (family, nb, class) in cases {
            let dim = nb.dim;
            let swept = sweep(family, dim, &target, u64::MAX).unwrap();
            assert!(swept.complete);
            assert_eq!(
                swept.best_run,
                engine_best(nb, class, terms),
                "{} in {}D",
                family.name(),
                dim
            );
        }
    }

    #[test]
    fn symmetric_families_die_at_two_three_in_every_higher_dimension() {
        // The theorems in bounds.rs, checked as far as the sweep goes quickly.
        let target = parse_target_from("primes", 10, 2).unwrap();
        for dim in 2..=12 {
            let iso = sweep(Family::VonNeumannIsotropic, dim, &target, u64::MAX).unwrap();
            assert!(iso.complete);
            assert_eq!(iso.best_run, 2, "von Neumann isotropic in {}D", dim);
        }
        for dim in 3..=6 {
            let life = sweep(Family::MooreTotalistic(1), dim, &target, u64::MAX).unwrap();
            assert!(life.complete);
            assert_eq!(life.best_run, 2, "Moore totalistic in {}D", dim);
        }
        let plane = sweep(Family::MooreTotalistic(1), 2, &target, u64::MAX).unwrap();
        assert_eq!(plane.best_run, 3);
    }
}
