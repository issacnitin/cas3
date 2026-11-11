/*
* Analyses built on the exact rule-table search.
*
* - complexity_profile: for each prefix length L the smallest number of table entries a rule has to
*   consult to reproduce the first L terms. A rule that really generates a sequence makes this curve
*   flatten; a rule that merely memorises the prefix needs more entries for every extra term.
* - arity_scan: every rule that depends on at most r of the neighbours, over all r-subsets of the
*   Moore neighbourhood up to lattice symmetry.
* - any_start_search: the same search from every close pair of cells (see bounds.rs), which by the
*   pair reduction covers every finite starting configuration for targets 2, 3, ...
* - legacy_expression_variants: size of the original expression-tree search space.
*/

use std::time::{Duration, Instant};

use crate::bounds::{interacting_pair_offsets, pair_representatives, pair_seed};
use crate::lattice::{moore_offsets, subset_orbit_representatives_in, Neighbourhood, SymmetryGroup};
use crate::progress::Progress;
use crate::rule_class::RuleClass;
use crate::table_search::{search, SearchParams, SearchReport, Solution};
use std::sync::Arc;

/// One close pair of an any-start run.
#[derive(Debug, Clone)]
pub struct PairRun {
    pub offset: Vec<i32>,
    pub seed: Vec<Vec<i32>>,
    pub report: SearchReport,
}

/// Close pairs to search for `base`, one per orbit of the symmetries that relate rules of the class
/// (all symmetries of the neighbourhood for symmetric classes; none for a table without symmetry).
pub fn any_start_pairs(base: &SearchParams) -> Vec<Vec<i32>> {
    let nb = &base.neighbourhood;
    let group = if base.symmetry || base.class != RuleClass::Table {
        SymmetryGroup::preserving(nb)
    } else {
        SymmetryGroup::trivial()
    };
    pair_representatives(&interacting_pair_offsets(nb), &group)
}

/// Runs `base` (whose target must start with 2) from every close pair. If every run is exhaustive,
/// the longest prefix over the pairs is exact for every finite starting configuration.
pub fn any_start_search(base: &SearchParams, mut on_pair: impl FnMut(&PairRun)) -> Result<Vec<PairRun>, String> {
    if base.target.first() != Some(&2) {
        return Err("an any-start run needs a target that starts with 2 (two seed cells)".to_string());
    }
    let mut runs = vec![];
    for offset in any_start_pairs(base) {
        let mut params = base.clone();
        params.seed = pair_seed(&offset);
        params.label = format!("{} · pair {:?}", base.label, offset);
        let report = search(&params)?;
        let run = PairRun { offset, seed: params.seed.clone(), report };
        on_pair(&run);
        runs.push(run);
    }
    Ok(runs)
}

/// Combines per-pair profiles: for each prefix length the minimum over the pairs and whether it is
/// exact (every other pair is known to need at least as many entries).
pub fn combine_profiles(per_pair: &[Vec<ProfileRow>]) -> Vec<(usize, Option<usize>, bool)> {
    let last = per_pair.iter().flat_map(|rows| rows.iter().map(|r| r.terms)).max().unwrap_or(0);
    let mut out = vec![];
    for terms in 2..=last {
        // Lower bound of each pair at this length (entries are non-decreasing in the length).
        let mut best: Option<usize> = None;
        let mut floor = usize::MAX;
        for rows in per_pair {
            match rows.iter().find(|r| r.terms == terms) {
                Some(row) => {
                    if let Some(m) = row.min_entries {
                        best = Some(best.map_or(m, |b: usize| b.min(m)));
                    } else {
                        floor = floor.min(row.ruled_out_below);
                    }
                }
                None => {
                    let below = rows.last().map(|r| r.ruled_out_below).unwrap_or(0);
                    floor = floor.min(below);
                }
            }
        }
        let exact = best.map_or(false, |b| b <= floor);
        out.push((terms, best, exact));
    }
    out
}

#[derive(Debug, Clone)]
pub struct ProfileRow {
    pub terms: usize,
    /// Smallest number of consulted entries for which a rule exists (exact), if found.
    pub min_entries: Option<usize>,
    /// Every rule consulting fewer entries than this was ruled out exhaustively.
    pub ruled_out_below: usize,
    pub nodes: u64,
    pub elapsed: Duration,
    pub solution: Option<Solution>,
}

/// Iterative deepening on the entry bound, for prefix lengths `first_terms..=last_terms` of
/// `base.target`. The minimum can only grow with the prefix, so each row starts where the last ended.
/// Stops at the first row that cannot be settled within `entry_cap` entries or `attempt_limit`.
pub fn complexity_profile(
    base: &SearchParams,
    first_terms: usize,
    last_terms: usize,
    entry_cap: usize,
    attempt_limit: Option<Duration>,
    mut on_row: impl FnMut(&ProfileRow),
) -> Result<Vec<ProfileRow>, String> {
    if first_terms < 2 || last_terms > base.target.len() || first_terms > last_terms {
        return Err("prefix range must satisfy 2 <= first <= last <= target length".to_string());
    }
    let mut rows: Vec<ProfileRow> = vec![];
    let mut bound = 0usize;
    for terms in first_terms..=last_terms {
        let started = Instant::now();
        let mut nodes = 0u64;
        let mut row = ProfileRow {
            terms,
            min_entries: None,
            ruled_out_below: bound,
            nodes: 0,
            elapsed: Duration::ZERO,
            solution: None,
        };
        loop {
            if bound > entry_cap {
                break;
            }
            let mut params = base.clone();
            params.target = base.target[..terms].to_vec();
            params.label = format!("{} — first {} terms, rules with ≤ {} entries", base.label, terms, bound);
            params.max_entries = Some(bound);
            params.find_first = true;
            params.store_limit = 1;
            params.time_limit = attempt_limit;
            let report = search(&params)?;
            nodes += report.nodes;
            if let Some(solution) = report.stored.first() {
                row.min_entries = Some(bound);
                row.ruled_out_below = bound;
                row.solution = Some(solution.clone());
                break;
            }
            if !report.complete() {
                break;
            }
            bound += 1;
            row.ruled_out_below = bound;
        }
        row.nodes = nodes;
        row.elapsed = started.elapsed();
        on_row(&row);
        let settled = row.min_entries.is_some();
        rows.push(row);
        if !settled {
            break;
        }
    }
    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ScanClass {
    pub arity: usize,
    pub index: usize,
    pub classes_in_arity: usize,
    pub offsets: Vec<Vec<i32>>,
    pub matched_terms: usize,
    pub solutions: u64,
    pub nodes: u64,
    pub complete: bool,
    pub elapsed: Duration,
}

#[derive(Debug, Clone)]
pub struct ScanSummary {
    pub arity: usize,
    pub classes: usize,
    pub with_solutions: usize,
    pub best_matched: usize,
    pub incomplete: usize,
    pub nodes: u64,
    pub elapsed: Duration,
}

/// Runs the exact search on every `r`-element subset of the Moore neighbourhood of radius
/// `universe_radius` (one per symmetry orbit) for each arity in `arities`. A rule that depends on
/// fewer neighbours is covered by any superset, so the result for arity r holds for "at most r
/// neighbours chosen from that universe".
#[cfg(test)]
pub fn arity_scan(
    base: &SearchParams,
    arities: std::ops::RangeInclusive<usize>,
    universe_radius: i32,
    progress: Option<Arc<Progress>>,
    on_class: impl FnMut(&ScanClass),
) -> Result<Vec<ScanSummary>, String> {
    arity_scan_with(base, arities, universe_radius, false, progress, on_class)
}

/// `arity_scan`; with `any_start` every class is searched from each of its close pairs (the target
/// must then start with 2), which covers every finite starting configuration.
pub fn arity_scan_with(
    base: &SearchParams,
    arities: std::ops::RangeInclusive<usize>,
    universe_radius: i32,
    any_start: bool,
    progress: Option<Arc<Progress>>,
    mut on_class: impl FnMut(&ScanClass),
) -> Result<Vec<ScanSummary>, String> {
    let dim = base.neighbourhood.dim;
    let universe = moore_offsets(dim, universe_radius, 1000)?;
    let mut summaries = vec![];
    for arity in arities {
        let representatives = subset_orbit_representatives_in(dim, &universe, arity)?;
        let started = Instant::now();
        let mut summary = ScanSummary {
            arity,
            classes: representatives.len(),
            with_solutions: 0,
            best_matched: 0,
            incomplete: 0,
            nodes: 0,
            elapsed: Duration::ZERO,
        };
        for (index, representative) in representatives.iter().enumerate() {
            let nb = Neighbourhood::new(dim, representative.iter().map(|&i| universe[i].clone()).collect())?;
            let mut params = base.clone();
            params.neighbourhood = nb.clone();
            params.label = format!("scan arity {} class {}/{}", arity, index + 1, representatives.len());
            params.progress = progress.clone();
            if let Some(p) = &progress {
                p.set_phase(
                    &format!("arity {} class {} of {}", arity, index + 1, representatives.len()),
                    index as u64,
                    representatives.len() as u64,
                );
            }
            let class = if any_start {
                let runs = any_start_search(&params, |_| {})?;
                ScanClass {
                    arity,
                    index,
                    classes_in_arity: representatives.len(),
                    offsets: nb.offsets.clone(),
                    matched_terms: runs.iter().map(|r| r.report.matched_terms()).max().unwrap_or(0),
                    solutions: runs.iter().map(|r| r.report.solutions).sum(),
                    nodes: runs.iter().map(|r| r.report.nodes).sum(),
                    complete: runs.iter().all(|r| r.report.complete()),
                    elapsed: runs.iter().map(|r| r.report.elapsed).sum(),
                }
            } else {
                let report = search(&params)?;
                ScanClass {
                    arity,
                    index,
                    classes_in_arity: representatives.len(),
                    offsets: nb.offsets.clone(),
                    matched_terms: report.matched_terms(),
                    solutions: report.solutions,
                    nodes: report.nodes,
                    complete: report.complete(),
                    elapsed: report.elapsed,
                }
            };
            summary.nodes += class.nodes;
            summary.best_matched = summary.best_matched.max(class.matched_terms);
            summary.with_solutions += (class.solutions > 0) as usize;
            summary.incomplete += (!class.complete) as usize;
            on_class(&class);
        }
        summary.elapsed = started.elapsed();
        summaries.push(summary);
    }
    Ok(summaries)
}

/// Number of expression structures the legacy generator enumerates for `leaves` input variables
/// (operators and leaf negations, not counting input permutations):
/// E(1) = 2 and E(m) = 2 * sum over splits s <= m/2 of E(s) * E(m - s).
pub fn legacy_expression_variants(leaves: usize) -> u128 {
    let mut variants = vec![0u128; leaves.max(1) + 1];
    variants[1] = 2;
    for m in 2..=leaves {
        variants[m] = 2 * (1..=m / 2).map(|s| variants[s] * variants[m - s]).sum::<u128>();
    }
    variants[leaves.max(1)]
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::graph::op_node::OpNode;
    use crate::sequences::{parse_target, primes_target};
    use crate::table_search::SurvivalMode;

    fn enumerate_variants(leaves: usize) -> u128 {
        let mut node = OpNode::new(0, leaves - 1);
        let mut count = 1u128;
        while node.has_next() {
            node.generate_next();
            count += 1;
        }
        count
    }

    #[test]
    fn legacy_variant_formula_matches_the_real_generator() {
        for leaves in 1..=7 {
            assert_eq!(legacy_expression_variants(leaves), enumerate_variants(leaves), "{} leaves", leaves);
        }
        assert_eq!(legacy_expression_variants(3), 32);
    }

    #[test]
    fn simple_sequences_have_a_flat_profile_primes_do_not() {
        let nb = Neighbourhood::moore(1, 1).unwrap();
        let mut growth = SearchParams::new(nb.clone(), parse_target("naturals", 12).unwrap());
        growth.parallel = false;
        let rows = complexity_profile(&growth, 2, 12, 40, None, |_| {}).unwrap();
        assert_eq!(rows.len(), 11);
        let entries: Vec<usize> = rows.iter().map(|r| r.min_entries.unwrap()).collect();
        // The count of consulted entries is non-decreasing and saturates once every pattern was seen.
        assert!(entries.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(entries[8], entries[10]);

        let mut primes = SearchParams::new(nb, primes_target(12));
        primes.parallel = false;
        let rows = complexity_profile(&primes, 2, 12, 40, None, |_| {}).unwrap();
        assert_eq!(rows.last().unwrap().min_entries, None);
        assert!(rows.len() < 11);
    }

    #[test]
    fn profile_rows_are_consistent_with_direct_searches() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let mut base = SearchParams::new(nb.clone(), primes_target(6));
        base.parallel = false;
        let rows = complexity_profile(&base, 2, 5, 60, None, |_| {}).unwrap();
        for row in &rows {
            let minimum = row.min_entries.unwrap();
            let mut params = base.clone();
            params.target = base.target[..row.terms].to_vec();
            params.store_limit = 0;
            params.max_entries = Some(minimum);
            assert!(search(&params).unwrap().solutions > 0, "{} terms", row.terms);
            params.max_entries = Some(minimum - 1);
            assert_eq!(search(&params).unwrap().solutions, 0, "{} terms", row.terms);
        }
        assert!(rows.windows(2).all(|w| w[0].min_entries <= w[1].min_entries));
    }

    #[test]
    fn arity_scan_over_a_larger_block_finds_more_classes_and_the_same_small_capacities() {
        let mut base = SearchParams::new(Neighbourhood::moore(2, 1).unwrap(), primes_target(30));
        base.parallel = false;
        base.store_limit = 0;
        let summaries = arity_scan(&base, 1..=2, 2, None, |class| {
            assert!(class.complete);
        })
        .unwrap();
        // Orbits of single offsets / pairs of offsets in the 5x5 block under the symmetries of the square.
        assert_eq!(summaries.iter().map(|s| s.classes).collect::<Vec<_>>(), vec![5, 44]);
        assert!(summaries.iter().all(|s| s.with_solutions == 0 && s.best_matched == 3));
    }

    /// End-to-end check of the pair reduction: simulate every rule of a small class from every small
    /// starting configuration, find the longest run 2, 3, 5, 7, ... at ANY time (after any transient),
    /// and compare with the longest prefix the any-start search finds from close pairs alone.
    fn brute_force_longest_prime_run(nb: &Neighbourhood, class: RuleClass, window: &[(i32, i32)], max_cells: usize, steps: usize) -> usize {
        use crate::rule_class::Classifier;
        use std::collections::{HashMap, HashSet};
        let classifier = Classifier::new(class, nb).unwrap();
        let k = nb.len();
        let mut birth_keys: Vec<u64> = (1..1u64 << k).map(|m| classifier.key(m)).collect();
        birth_keys.sort_unstable();
        birth_keys.dedup();
        let mut survive_keys: Vec<u64> = (0..1u64 << k).map(|m| classifier.key(m)).collect();
        survive_keys.sort_unstable();
        survive_keys.dedup();
        let birth_index: HashMap<u64, usize> = birth_keys.iter().enumerate().map(|(i, &key)| (key, i)).collect();
        let survive_index: HashMap<u64, usize> =
            survive_keys.iter().enumerate().map(|(i, &key)| (key, birth_keys.len() + i)).collect();
        let bits = birth_keys.len() + survive_keys.len();
        let offsets: Vec<(i32, i32)> = nb.offsets.iter().map(|o| (o[0], *o.get(1).unwrap_or(&0))).collect();
        let primes = primes_target(steps + 2)[1..].to_vec();
        let mut configs: Vec<Vec<(i32, i32)>> = vec![];
        for subset in 1u32..(1 << window.len()) {
            if subset.count_ones() as usize <= max_cells {
                configs.push(window.iter().enumerate().filter(|(i, _)| subset >> i & 1 == 1).map(|(_, &c)| c).collect());
            }
        }
        let mut best = 0;
        for rule in 0..(1u64 << bits) {
            for config in &configs {
                let mut live: HashSet<(i32, i32)> = config.iter().copied().collect();
                let mut counts = vec![live.len()];
                for _ in 0..steps {
                    let mut candidates: HashSet<(i32, i32)> = live.clone();
                    for &(x, y) in &live {
                        for &(dx, dy) in &offsets {
                            candidates.insert((x - dx, y - dy));
                        }
                    }
                    let mut next = HashSet::new();
                    for &(x, y) in &candidates {
                        let mut mask = 0u64;
                        for (j, &(dx, dy)) in offsets.iter().enumerate() {
                            if live.contains(&(x + dx, y + dy)) {
                                mask |= 1 << j;
                            }
                        }
                        let key = classifier.key(mask);
                        let on = if live.contains(&(x, y)) {
                            rule >> survive_index[&key] & 1 == 1
                        } else {
                            mask != 0 && rule >> birth_index[&key] & 1 == 1
                        };
                        if on {
                            next.insert((x, y));
                        }
                    }
                    live = next;
                    counts.push(live.len());
                    if live.is_empty() || live.len() > 200 {
                        break;
                    }
                }
                for start in 0..counts.len() {
                    let run = counts[start..].iter().zip(&primes).take_while(|(a, b)| a == b).count();
                    best = best.max(run);
                }
            }
        }
        best
    }

    #[test]
    fn any_start_search_equals_brute_force_over_starting_configurations() {
        let cases: Vec<(Neighbourhood, RuleClass, Vec<(i32, i32)>, usize, usize)> = vec![
            (Neighbourhood::moore(1, 1).unwrap(), RuleClass::Table, (-4..=4).map(|x| (x, 0)).collect(), 9, 14),
            (Neighbourhood::moore(1, 2).unwrap(), RuleClass::Totalistic, (-4..=4).map(|x| (x, 0)).collect(), 9, 14),
            (
                Neighbourhood::von_neumann(2).unwrap(),
                RuleClass::Totalistic,
                (-1..=1).flat_map(|x| (-1..=1).map(move |y| (x, y))).collect(),
                3,
                8,
            ),
        ];
        for (nb, class, window, max_cells, steps) in cases {
            let brute = brute_force_longest_prime_run(&nb, class, &window, max_cells, steps);
            let mut base = SearchParams::new(nb.clone(), primes_target(steps + 2)[1..].to_vec());
            base.class = class;
            base.parallel = false;
            base.store_limit = 0;
            let runs = any_start_search(&base, |_| {}).unwrap();
            assert!(runs.iter().all(|r| r.report.complete()));
            let best = runs.iter().map(|r| r.report.matched_terms()).max().unwrap();
            assert_eq!(best, brute, "dim {} {:?}", nb.dim, class);
            assert!(best >= 2);
        }
    }

    #[test]
    fn combined_profile_takes_the_minimum_and_knows_when_it_is_exact() {
        let row = |terms, min_entries: Option<usize>, ruled_out_below| ProfileRow {
            terms,
            min_entries,
            ruled_out_below,
            nodes: 0,
            elapsed: Duration::ZERO,
            solution: None,
        };
        let a = vec![row(2, Some(5), 5), row(3, Some(9), 9), row(4, None, 12)];
        let b = vec![row(2, Some(4), 4), row(3, None, 7)];
        assert_eq!(
            combine_profiles(&[a, b]),
            vec![(2, Some(4), true), (3, Some(9), false), (4, None, false)]
        );
    }

    #[test]
    fn arity_scan_covers_the_neighbourhood_exactly() {
        let mut base = SearchParams::new(Neighbourhood::moore(2, 1).unwrap(), primes_target(6));
        base.parallel = false;
        base.survival = SurvivalMode::Free;
        base.store_limit = 0;
        let mut seen = 0;
        let summaries = arity_scan(&base, 1..=3, 1, None, |class| {
            seen += 1;
            assert!(class.complete);
            assert_eq!(class.offsets.len(), class.arity);
        })
        .unwrap();
        assert_eq!(seen, 2 + 6 + 10);
        assert_eq!(summaries.iter().map(|s| s.classes).collect::<Vec<_>>(), vec![2, 6, 10]);
        // More neighbours can only let a rule reproduce a longer prefix.
        assert!(summaries.windows(2).all(|w| w[0].best_matched <= w[1].best_matched));

        // The scan's exhaustive result for the full neighbourhood agrees with a direct search.
        let mut full = base.clone();
        full.neighbourhood = Neighbourhood::moore(2, 1).unwrap();
        full.max_entries = Some(12);
        let direct = search(&full).unwrap();
        assert!(direct.matched_terms() >= summaries.last().unwrap().best_matched.min(3));
    }
}
