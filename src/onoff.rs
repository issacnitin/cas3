/*
* The strict on/off goal: one rule on the 3x3 neighbourhood (512 entries, the all-off pattern stays
* off), two lit cells at the start, and exactly the next prime number of lit cells after every step.
*
* `OnOffRule` is a complete rule, written as 128 hex digits. `run` simulates it on a dense grid with
* its own code (it shares nothing with the search engine) and stops at the first wrong count, so a
* rule found by the search is judged as a whole rule: every entry the search left open is off.
*
* By the pair reduction (src/bounds.rs) every run 2, 3, 5, ... passes through a pair of lit cells at
* most two apart in each direction, so the starting pairs below cover every starting pattern.
*/

use crate::lattice::Neighbourhood;
use crate::reference::RuleFn;
use crate::table_search::Solution;

/// Bit index of the cell at offset (dx, dy) in a 3x3 pattern; the cell itself is bit 4.
pub fn bit(dx: i32, dy: i32) -> usize {
    ((dy + 1) * 3 + (dx + 1)) as usize
}

/// A complete on/off rule: entry p is the next state of a cell whose 3x3 pattern is p.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct OnOffRule {
    bits: [u64; 8],
}

impl OnOffRule {
    pub fn get(&self, pattern: usize) -> bool {
        (self.bits[pattern / 64] >> (pattern % 64)) & 1 == 1
    }

    pub fn set(&mut self, pattern: usize, on: bool) {
        let mask = 1u64 << (pattern % 64);
        if on {
            self.bits[pattern / 64] |= mask;
        } else {
            self.bits[pattern / 64] &= !mask;
        }
    }

    /// The all-off pattern stays off (required: otherwise empty space lights up).
    pub fn is_quiescent(&self) -> bool {
        !self.get(0)
    }

    /// Patterns whose entry is on.
    pub fn on_entries(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// 128 hex digits; digit i holds entries 4i .. 4i + 3 (lowest bit first).
    pub fn to_hex(&self) -> String {
        (0..128)
            .map(|i| {
                let nibble = (0..4).fold(0u32, |acc, j| acc | (self.get(4 * i + j) as u32) << j);
                char::from_digit(nibble, 16).unwrap()
            })
            .collect()
    }

    pub fn from_hex(text: &str) -> Result<Self, String> {
        let digits: Vec<char> = text.trim().chars().collect();
        if digits.len() != 128 {
            return Err(format!("a rule has 128 hex digits, got {}", digits.len()));
        }
        let mut rule = OnOffRule::default();
        for (i, c) in digits.iter().enumerate() {
            let nibble = c
                .to_digit(16)
                .ok_or_else(|| format!("'{}' is not a hex digit", c))?;
            for j in 0..4 {
                rule.set(4 * i + j, (nibble >> j) & 1 == 1);
            }
        }
        Ok(rule)
    }

    /// The rule of a search solution (unrestricted 2D Moore tables): the consulted entries as found,
    /// every other entry off.
    pub fn from_solution(nb: &Neighbourhood, solution: &Solution) -> Result<Self, String> {
        if nb.dim != 2 || nb.len() != 8 || nb.offsets.iter().any(|o| o.iter().any(|x| x.abs() > 1))
        {
            return Err(
                "only the 2D Moore neighbourhood of radius 1 maps to an on/off rule".into(),
            );
        }
        let place = |mask: u64, centre: bool| -> usize {
            let mut pattern = if centre { 1 << bit(0, 0) } else { 0 };
            for (j, o) in nb.offsets.iter().enumerate() {
                if (mask >> j) & 1 == 1 {
                    pattern |= 1 << bit(o[0], o[1]);
                }
            }
            pattern
        };
        let mut rule = OnOffRule::default();
        for &(mask, on) in &solution.birth {
            rule.set(place(mask, false), on);
        }
        for &(mask, on) in &solution.survival {
            rule.set(place(mask, true), on);
        }
        if !rule.is_quiescent() {
            return Err("the solution lights up empty space".into());
        }
        Ok(rule)
    }
}

/// The rule as birth/survival predicates over `nb`'s neighbour masks, for the reference simulator.
pub struct AsMasks<'a> {
    pub rule: &'a OnOffRule,
    pub nb: &'a Neighbourhood,
}

impl AsMasks<'_> {
    fn pattern(&self, mask: u64, centre: bool) -> usize {
        let mut pattern = if centre { 1 << bit(0, 0) } else { 0 };
        for (j, o) in self.nb.offsets.iter().enumerate() {
            if (mask >> j) & 1 == 1 {
                pattern |= 1 << bit(o[0], o[1]);
            }
        }
        pattern
    }
}

impl RuleFn for AsMasks<'_> {
    fn birth(&self, mask: u64) -> bool {
        self.rule.get(self.pattern(mask, false))
    }

    fn survive(&self, mask: u64) -> bool {
        self.rule.get(self.pattern(mask, true))
    }
}

/// A finite set of lit cells on a dense grid with a margin of empty cells.
#[derive(Clone, Debug)]
pub struct Grid {
    x0: i64,
    y0: i64,
    width: usize,
    height: usize,
    cells: Vec<bool>,
}

impl Grid {
    pub fn new(lit: &[(i64, i64)]) -> Self {
        let mut grid = Grid {
            x0: 0,
            y0: 0,
            width: 0,
            height: 0,
            cells: vec![],
        };
        grid.reset(lit);
        grid
    }

    fn reset(&mut self, lit: &[(i64, i64)]) {
        let (min_x, max_x) = lit
            .iter()
            .fold((i64::MAX, i64::MIN), |(a, b), &(x, _)| (a.min(x), b.max(x)));
        let (min_y, max_y) = lit
            .iter()
            .fold((i64::MAX, i64::MIN), |(a, b), &(_, y)| (a.min(y), b.max(y)));
        if lit.is_empty() {
            *self = Grid {
                x0: 0,
                y0: 0,
                width: 0,
                height: 0,
                cells: vec![],
            };
            return;
        }
        self.x0 = min_x - 1;
        self.y0 = min_y - 1;
        self.width = (max_x - min_x + 3) as usize;
        self.height = (max_y - min_y + 3) as usize;
        self.cells = vec![false; self.width * self.height];
        for &(x, y) in lit {
            let i = (y - self.y0) as usize * self.width + (x - self.x0) as usize;
            self.cells[i] = true;
        }
    }

    pub fn lit(&self) -> Vec<(i64, i64)> {
        let mut out = vec![];
        for row in 0..self.height {
            for col in 0..self.width {
                if self.cells[row * self.width + col] {
                    out.push((self.x0 + col as i64, self.y0 + row as i64));
                }
            }
        }
        out
    }

    pub fn count(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    fn at(&self, x: i64, y: i64) -> bool {
        let (col, row) = (x - self.x0, y - self.y0);
        col >= 0
            && row >= 0
            && (col as usize) < self.width
            && (row as usize) < self.height
            && self.cells[row as usize * self.width + col as usize]
    }

    /// One step: every cell within one of a lit cell reads its 3x3 pattern (all other cells are
    /// all-off and stay off, since the rule is quiescent).
    pub fn step(&mut self, rule: &OnOffRule) {
        let mut next = vec![];
        for y in self.y0..self.y0 + self.height as i64 {
            for x in self.x0..self.x0 + self.width as i64 {
                let mut pattern = 0usize;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if self.at(x + dx as i64, y + dy as i64) {
                            pattern |= 1 << bit(dx, dy);
                        }
                    }
                }
                if pattern != 0 && rule.get(pattern) {
                    next.push((x, y));
                }
            }
        }
        self.reset(&next);
    }
}

/// The longest run found for the strict goal (search, 2D Moore tables, from the diagonal pair two
/// apart, 20 minutes): 76 primes (2 .. 383). It consults 485 of the 512 entries and has 398 lit
/// cells at step 76 instead of 389: memorised, not generated.
pub const BEST_RULE: &str = "04448002a115402485a44d643cd1d12e905f002597e9c3e4546ecca1b0d6b008545009b2ac5eea87f1981ee2e55045d554e3cec13d6db490750498a2bb085798";
pub const BEST_SEED: [(i64, i64); 2] = [(-1, -1), (1, 1)];

/// The primes from 2 up to the `n`-th.
pub fn first_primes(n: usize) -> Vec<usize> {
    let mut limit = 16usize;
    loop {
        let mut composite = vec![false; limit + 1];
        let mut primes = vec![];
        for i in 2..=limit {
            if !composite[i] {
                primes.push(i);
                let mut j = i * i;
                while j <= limit {
                    composite[j] = true;
                    j += i;
                }
            }
        }
        if primes.len() >= n {
            primes.truncate(n);
            return primes;
        }
        limit *= 2;
    }
}

/// How a rule did from a starting pair.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// Consecutive primes matched from step 0 (the starting pair counts as 2).
    pub matched: usize,
    /// The count after the last step simulated (the first wrong one, if any).
    pub counts: Vec<usize>,
}

/// Runs the rule from `seed` until the count is not the next prime or `terms` primes are matched.
pub fn run(rule: &OnOffRule, seed: &[(i64, i64)], terms: usize) -> Verdict {
    let primes = first_primes(terms.max(1));
    let mut grid = Grid::new(seed);
    let mut counts = vec![grid.count()];
    let mut matched = 0;
    while matched < terms && counts[matched] == primes[matched] {
        matched += 1;
        if matched == terms {
            break;
        }
        grid.step(rule);
        counts.push(grid.count());
    }
    Verdict { matched, counts }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::reference::{reference_states_from, splitmix64, Config};
    use crate::table_search::{search, SearchParams};

    fn life() -> OnOffRule {
        let mut rule = OnOffRule::default();
        for pattern in 0..512usize {
            let centre = (pattern >> bit(0, 0)) & 1 == 1;
            let around = (pattern & !(1 << bit(0, 0))).count_ones();
            rule.set(pattern, around == 3 || (centre && around == 2));
        }
        rule
    }

    #[test]
    fn hex_round_trips_and_life_behaves() {
        let rule = life();
        assert_eq!(OnOffRule::from_hex(&rule.to_hex()).unwrap(), rule);
        assert!(OnOffRule::from_hex("12").is_err());
        assert!(rule.is_quiescent());
        // A glider keeps 5 cells and moves one cell diagonally every 4 steps.
        let glider = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
        let mut grid = Grid::new(&glider);
        for _ in 0..4 {
            grid.step(&rule);
            assert_eq!(grid.count(), 5);
        }
        let mut moved: Vec<(i64, i64)> = glider.iter().map(|&(x, y)| (x + 1, y + 1)).collect();
        moved.sort();
        let mut now = grid.lit();
        now.sort();
        assert_eq!(now, moved);
    }

    #[test]
    fn the_grid_agrees_with_the_reference_simulator_on_random_rules() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        for seed in 0..40u64 {
            let mut rule = OnOffRule::default();
            for pattern in 1..512 {
                rule.set(pattern, splitmix64(seed * 1000 + pattern as u64) % 3 == 0);
            }
            let start: Vec<(i64, i64)> = (0..4)
                .map(|k| {
                    let r = splitmix64(seed ^ (k + 77));
                    ((r % 5) as i64 - 2, ((r >> 8) % 5) as i64 - 2)
                })
                .collect();
            let cells: Config = start
                .iter()
                .map(|&(x, y)| vec![x as i32, y as i32])
                .collect();
            let expected: Vec<usize> = reference_states_from(
                &nb,
                &AsMasks {
                    rule: &rule,
                    nb: &nb,
                },
                &cells,
                6,
            )
            .iter()
            .map(|c| c.len())
            .collect();
            let mut grid = Grid::new(&start);
            let mut counts = vec![grid.count()];
            for _ in 0..6 {
                grid.step(&rule);
                counts.push(grid.count());
            }
            assert_eq!(counts, expected, "seed {}", seed);
        }
    }

    #[test]
    fn search_solutions_hold_up_as_complete_rules() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let mut params = SearchParams::new(nb.clone(), first_primes(14));
        params.seed = vec![vec![0, 0], vec![1, 0]];
        params.find_first = true;
        let report = search(&params).unwrap();
        let solution = report
            .stored
            .first()
            .expect("a 14-prime rule from the pair");
        let rule = OnOffRule::from_solution(&nb, solution).unwrap();
        assert!(run(&rule, &[(0, 0), (1, 0)], 14).matched == 14);
    }

    #[test]
    fn the_best_rule_found_matches_76_primes_then_fails() {
        let rule = OnOffRule::from_hex(BEST_RULE).unwrap();
        assert!(rule.is_quiescent());
        let verdict = run(&rule, &BEST_SEED, 10_000);
        assert_eq!(verdict.matched, 76);
        assert_eq!(verdict.counts[76], 398);
        assert_eq!(first_primes(77)[76], 389);
    }

    #[test]
    fn the_deepest_partial_rule_reproduces_the_longest_run() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let mut params = SearchParams::new(nb.clone(), first_primes(60));
        params.seed = vec![vec![0, 0], vec![1, 0]];
        params.node_limit = Some(200_000);
        let report = search(&params).unwrap();
        assert!(report.stored.is_empty() && report.matched_terms() > 14);
        let partial = report.deepest_rule.as_ref().unwrap();
        let rule = OnOffRule::from_solution(&nb, partial).unwrap();
        assert!(run(&rule, &[(0, 0), (1, 0)], 60).matched >= report.matched_terms());
    }
}
