/*
* Analyses built on the exact rule-table search.
*
* - complexity_profile: for each prefix length L the smallest number of table entries a rule has to
*   consult to reproduce the first L terms. A rule that really generates a sequence makes this curve
*   flatten; a rule that merely memorises the prefix needs more entries for every extra term.
* - arity_scan: every rule that depends on at most r of the neighbours, over all r-subsets of the
*   Moore neighbourhood up to lattice symmetry.
* - legacy_expression_variants: size of the original expression-tree search space.
*/

use std::time::{Duration, Instant};

use crate::lattice::{subset_orbit_representatives, Neighbourhood};
use crate::progress::Progress;
use crate::table_search::{search, SearchParams, Solution};
use std::sync::Arc;

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

/// Runs the exact search on every `r`-element subset of the unit Moore neighbourhood (one per
/// symmetry orbit) for each arity in `arities`. A rule that depends on fewer neighbours is covered
/// by any superset, so the result for arity r holds for "at most r neighbours".
pub fn arity_scan(
    base: &SearchParams,
    arities: std::ops::RangeInclusive<usize>,
    progress: Option<Arc<Progress>>,
    mut on_class: impl FnMut(&ScanClass),
) -> Result<Vec<ScanSummary>, String> {
    let dim = base.neighbourhood.dim;
    let moore = Neighbourhood::moore(dim, 1)?;
    let mut summaries = vec![];
    for arity in arities {
        let representatives = subset_orbit_representatives(dim, arity)?;
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
            let nb = moore.select(representative)?;
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
            let report = search(&params)?;
            let class = ScanClass {
                arity,
                index,
                classes_in_arity: representatives.len(),
                offsets: nb.offsets.clone(),
                matched_terms: report.matched_terms(),
                solutions: report.solutions,
                nodes: report.nodes,
                complete: report.complete(),
                elapsed: report.elapsed,
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
    fn arity_scan_covers_the_neighbourhood_exactly() {
        let mut base = SearchParams::new(Neighbourhood::moore(2, 1).unwrap(), primes_target(6));
        base.parallel = false;
        base.survival = SurvivalMode::Free;
        base.store_limit = 0;
        let mut seen = 0;
        let summaries = arity_scan(&base, 1..=3, None, |class| {
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
