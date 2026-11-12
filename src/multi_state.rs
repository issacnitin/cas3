/*
* Rules with three or more states per cell, searched for a count that follows the primes.
*
* The other searches (table_search.rs, rule_class.rs, dimension_sweep.rs) cover two-state rules: a
* cell is set or not. Here a cell is in one of k states, 0 (empty) to k - 1, and the rule maps a
* cell's own state and its neighbours' states to its next state; an empty cell with empty neighbours
* stays empty. Which cells count:
*  - Lit::NonZero: every cell that is not empty (the states are different colours of a lit cell);
*  - Lit::One: only state 1; states 2 .. k-1 are hidden helpers that do not count, like the machinery
*    of the constructed automaton in prime_display.rs.
*
* Classes, by how much of the neighbourhood a rule reads:
*  - Totalistic: the own state and how many neighbours are in each state (multi-state Life-like);
*  - Isotropic: the own state and the neighbourhood pattern up to the rotations and reflections of
*    the grid;
*  - Table: the own state and the full pattern.
* Totalistic and isotropic rules treat all directions alike, like the laws of physics.
*
* The search is the lazy prefix search of the binary engines: the configuration is simulated from
* the seed, a table entry is decided the first time a cell needs it, and an assignment survives only
* if the step produces exactly the next target count. Each node is a distinct trajectory, so a
* complete search is exhaustive over the class. In addition:
*  - states with the same role are interchangeable (relabelling them changes no count), so a state
*    may be used for the first time only if every lower one already is;
*  - for the symmetric classes, seeds are taken up to the symmetries of the grid;
*  - every node records how many table entries its trajectory has consulted, so a complete search
*    also gives the exact smallest rule for each prefix of the target (its description length).
*
* Symmetric rules and one cell (any dimension, any number of states, hidden helpers or not). A
* symmetric rule commutes with the rotations and reflections that fix the seed, so everything grown
* from one cell is symmetric about it: the cells in each state form a union of orbits, namely the seed
* cell and orbits of other cells, which have at least 4 cells on the square grid and at least 6 on
* the hexagonal grid and the cube. The count is 0 or 1 plus a sum of orbit sizes of at least 4, so it
* is never 2 or 3: no symmetric rule takes one cell to 2.
*
* Every starting pattern (Lit::NonZero). If the counts at two consecutive steps are 2 and 3, the first
* configuration is two cells a, b in some states. If b - a is not in N0 - N0 (N0: the neighbourhood
* and the cell), no cell reads both, each evolves as it would alone, and the next count is c_a + c_b,
* where c_s is the count one step after a lone cell in state s. For a symmetric rule each c_s is 0 or
* 1 plus a sum of orbit sizes of at least 4, so c_a + c_b is never 3. So a run 2, 3, 5, ... of a
* symmetric rule from any finite starting pattern passes through a close pair, and the close pairs,
* with both state combinations up to relabelling (1, 1 and 1, 2) and up to symmetry, settle every
* starting pattern. With hidden helpers (Lit::One) a configuration with two lit cells may hold any
* number of hidden ones, so there the search starts from one or two lit cells only.
*/

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use rayon::prelude::*;

use crate::bounds::{interacting_pair_offsets, pair_representatives_linear, pair_seed};
use crate::lattice::{apply_matrix, hexagonal_symmetries, hyperoctahedral_group, Neighbourhood};
use crate::sequences::parse_target_from;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    Line,
    Square,
    SquareVonNeumann,
    Hexagonal,
    Cube,
    CubeVonNeumann,
}

impl Grid {
    pub fn name(&self) -> &'static str {
        match self {
            Grid::Line => "line (1D, 2 neighbours)",
            Grid::Square => "square (2D, 8 neighbours)",
            Grid::SquareVonNeumann => "square, axis neighbours only (2D, 4)",
            Grid::Hexagonal => "hexagonal (2D, 6 neighbours)",
            Grid::Cube => "cube (3D, 26 neighbours)",
            Grid::CubeVonNeumann => "cube, axis neighbours only (3D, 6)",
        }
    }

    pub fn parse(text: &str) -> Result<Grid, String> {
        Ok(match text {
            "line" => Grid::Line,
            "square" => Grid::Square,
            "vn" | "square-vn" => Grid::SquareVonNeumann,
            "hex" => Grid::Hexagonal,
            "cube" => Grid::Cube,
            "cube-vn" => Grid::CubeVonNeumann,
            other => {
                return Err(format!(
                    "unknown grid '{}' (line | square | vn | hex | cube | cube-vn)",
                    other
                ))
            }
        })
    }

    pub fn neighbourhood(&self) -> Neighbourhood {
        match self {
            Grid::Line => Neighbourhood::new(1, vec![vec![1], vec![-1]]),
            Grid::Square => Neighbourhood::moore(2, 1),
            Grid::SquareVonNeumann => Neighbourhood::von_neumann(2),
            Grid::Hexagonal => Ok(Neighbourhood::hexagonal()),
            Grid::Cube => Neighbourhood::moore(3, 1),
            Grid::CubeVonNeumann => Neighbourhood::von_neumann(3),
        }
        .expect("valid neighbourhood")
    }

    /// The rotations and reflections that fix a cell, as integer matrices (identity included).
    pub fn symmetries(&self) -> Vec<Vec<Vec<i32>>> {
        match self {
            Grid::Hexagonal => hexagonal_symmetries(),
            _ => {
                let dim = self.neighbourhood().dim;
                hyperoctahedral_group(dim)
                    .iter()
                    .map(|g| {
                        let mut matrix = vec![vec![0; dim]; dim];
                        for i in 0..dim {
                            matrix[i][g.perm[i]] = g.sign[i];
                        }
                        matrix
                    })
                    .collect()
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Totalistic,
    Isotropic,
    Table,
}

impl Class {
    pub fn name(&self) -> &'static str {
        match self {
            Class::Totalistic => "totalistic",
            Class::Isotropic => "isotropic",
            Class::Table => "table",
        }
    }

    pub fn parse(text: &str) -> Result<Class, String> {
        Ok(match text {
            "totalistic" => Class::Totalistic,
            "isotropic" => Class::Isotropic,
            "table" => Class::Table,
            other => {
                return Err(format!(
                    "unknown class '{}' (totalistic | isotropic | table)",
                    other
                ))
            }
        })
    }

    pub fn is_symmetric(&self) -> bool {
        *self != Class::Table
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lit {
    /// Every non-empty cell counts.
    NonZero,
    /// Only state 1 counts; the other states are hidden helpers.
    One,
}

impl Lit {
    pub fn name(&self) -> &'static str {
        match self {
            Lit::NonZero => "every non-empty cell counts",
            Lit::One => "only state 1 counts (other states are hidden helpers)",
        }
    }

    pub fn parse(text: &str) -> Result<Lit, String> {
        Ok(match text {
            "any" => Lit::NonZero,
            "one" => Lit::One,
            other => return Err(format!("unknown lit mode '{}' (any | one)", other)),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Start {
    /// One lit cell; target 1, 2, 3, 5, ...
    One,
    /// Two lit cells, every close pair up to symmetry; target 2, 3, 5, ...
    Pair,
}

const COORDINATE_BITS: u32 = 16;
const COORDINATE_OFFSET: i64 = 1 << (COORDINATE_BITS - 1);
/// Totalistic keys pack the number of neighbours in each state into fields of this many bits.
const COUNT_BITS: u32 = 5;

fn encode(coordinates: &[i32]) -> i64 {
    coordinates
        .iter()
        .enumerate()
        .map(|(i, &x)| (x as i64 + COORDINATE_OFFSET) << (COORDINATE_BITS * i as u32))
        .sum()
}

fn delta(offset: &[i32]) -> i64 {
    offset
        .iter()
        .enumerate()
        .map(|(i, &x)| (x as i64) << (COORDINATE_BITS * i as u32))
        .sum()
}

fn binomial(n: u128, r: u128) -> u128 {
    (0..r).fold(1, |acc, i| acc * (n - i) / (i + 1))
}

/// A class of rules: grid, how a rule reads the neighbourhood, number of states, which cells count.
pub struct Spec {
    pub grid: Grid,
    pub class: Class,
    pub states: u8,
    pub lit: Lit,
    /// Configurations with more non-empty cells are not followed (only binds with hidden helpers).
    pub max_cells: usize,
    /// Rules that consult more table entries are not followed.
    pub max_entries: usize,
    offsets: Vec<Vec<i32>>,
    deltas: Vec<i64>,
    matrices: Vec<Vec<Vec<i32>>>,
    /// The symmetries as permutations of neighbour positions.
    perms: Vec<Vec<usize>>,
    /// pow[i] = states^i.
    pow: Vec<u64>,
}

impl Spec {
    pub fn new(grid: Grid, class: Class, states: u8, lit: Lit) -> Result<Spec, String> {
        if !(2..=6).contains(&states) {
            return Err("the number of states must be between 2 and 6".to_string());
        }
        let nb = grid.neighbourhood();
        let m = nb.offsets.len();
        let k = states as u64;
        if k.checked_pow(m as u32 + 1).is_none() {
            return Err(format!(
                "{} states are too many for {} neighbours",
                states, m
            ));
        }
        let pow = (0..=m).map(|i| k.pow(i as u32)).collect();
        let matrices = grid.symmetries();
        let perms = matrices
            .iter()
            .map(|matrix| {
                nb.offsets
                    .iter()
                    .map(|o| {
                        let image = apply_matrix(matrix, o);
                        nb.offsets
                            .iter()
                            .position(|p| *p == image)
                            .expect("a symmetry maps the neighbourhood onto itself")
                    })
                    .collect()
            })
            .collect();
        Ok(Spec {
            grid,
            class,
            states,
            lit,
            max_cells: 1000,
            max_entries: usize::MAX,
            deltas: nb.offsets.iter().map(|o| delta(o)).collect(),
            offsets: nb.offsets,
            matrices,
            perms,
            pow,
        })
    }

    pub fn dim(&self) -> usize {
        self.offsets[0].len()
    }

    pub fn is_lit(&self, state: u8) -> bool {
        match self.lit {
            Lit::NonZero => state != 0,
            Lit::One => state == 1,
        }
    }

    /// Table entries of the class, not counting the fixed "empty stays empty" entry.
    pub fn entries(&self) -> u128 {
        let k = self.states as u128;
        let m = self.offsets.len() as u32;
        let patterns = match self.class {
            Class::Table => k.pow(m),
            Class::Totalistic => binomial(m as u128 + k - 1, k - 1),
            Class::Isotropic => {
                // Burnside: the average number of patterns fixed by a symmetry.
                let fixed: u128 = self
                    .perms
                    .iter()
                    .map(|perm| {
                        let mut seen = vec![false; perm.len()];
                        let mut cycles = 0;
                        for start in 0..perm.len() {
                            if !seen[start] {
                                cycles += 1;
                                let mut i = start;
                                while !seen[i] {
                                    seen[i] = true;
                                    i = perm[i];
                                }
                            }
                        }
                        k.pow(cycles)
                    })
                    .sum();
                fixed / self.perms.len() as u128
            }
        };
        k * patterns - 1
    }

    /// What a neighbour in `state` at neighbour position `position` adds to a cell's pattern.
    fn contribution(&self, state: u8, position: usize) -> u64 {
        match self.class {
            Class::Totalistic => 1u64 << (COUNT_BITS * (state as u32 - 1)),
            _ => state as u64 * self.pow[position],
        }
    }

    fn key(&self, own: u8, pattern: u64, cache: &mut HashMap<u64, u64>) -> u64 {
        let m = self.offsets.len();
        match self.class {
            Class::Totalistic => {
                ((own as u64) << (COUNT_BITS * (self.states as u32 - 1))) | pattern
            }
            Class::Table => own as u64 * self.pow[m] + pattern,
            Class::Isotropic => {
                let canonical = *cache
                    .entry(pattern)
                    .or_insert_with(|| self.canonical(pattern));
                own as u64 * self.pow[m] + canonical
            }
        }
    }

    /// The smallest image of a pattern under the symmetries.
    fn canonical(&self, pattern: u64) -> u64 {
        let k = self.states as u64;
        let mut digits = [0u64; 32];
        let mut rest = pattern;
        for digit in digits.iter_mut().take(self.offsets.len()) {
            *digit = rest % k;
            rest /= k;
        }
        self.perms
            .iter()
            .map(|perm| {
                perm.iter()
                    .enumerate()
                    .map(|(i, &j)| digits[i] * self.pow[j])
                    .sum::<u64>()
            })
            .min()
            .expect("the identity is a symmetry")
    }

    /// A table entry in words: the own state and what the neighbours are.
    pub fn describe_key(&self, key: u64) -> String {
        let k = self.states as u64;
        let m = self.offsets.len();
        let (own, neighbours) = match self.class {
            Class::Totalistic => {
                let shift = COUNT_BITS * (self.states as u32 - 1);
                let counts: Vec<String> = (1..self.states)
                    .map(|s| {
                        let n = (key >> (COUNT_BITS * (s as u32 - 1))) & ((1 << COUNT_BITS) - 1);
                        format!("{}x{}", n, s)
                    })
                    .collect();
                (key >> shift, format!("neighbours {}", counts.join(" ")))
            }
            _ => {
                let mut rest = key % self.pow[m];
                let digits: Vec<String> = (0..m)
                    .map(|_| {
                        let d = rest % k;
                        rest /= k;
                        d.to_string()
                    })
                    .collect();
                (key / self.pow[m], format!("pattern {}", digits.join("")))
            }
        };
        format!("state {}, {}", own, neighbours)
    }
}

/// The starting cells and their states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seed {
    pub cells: Vec<(Vec<i32>, u8)>,
}

impl Seed {
    fn config(&self) -> HashMap<i64, u8> {
        self.cells.iter().map(|(c, s)| (encode(c), *s)).collect()
    }

    fn highest_state(&self) -> u8 {
        self.cells.iter().map(|&(_, s)| s).max().unwrap_or(1)
    }

    pub fn describe(&self) -> String {
        self.cells
            .iter()
            .map(|(c, s)| {
                let coordinates: Vec<String> = c.iter().map(|x| x.to_string()).collect();
                format!("({})={}", coordinates.join(","), s)
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The seeds to search from: one lit cell, or every close pair up to symmetry (with both state
/// combinations when every non-empty cell counts and a second state exists).
pub fn seeds(spec: &Spec, start: Start) -> Result<Vec<Seed>, String> {
    match start {
        Start::One => Ok(vec![Seed {
            cells: vec![(vec![0; spec.dim()], 1)],
        }]),
        Start::Pair => {
            if !spec.class.is_symmetric() {
                return Err("pairs are only reduced by symmetry for totalistic and isotropic rules; use --start one".to_string());
            }
            let nb = spec.grid.neighbourhood();
            let representatives =
                pair_representatives_linear(&interacting_pair_offsets(&nb), &spec.matrices);
            let mut states = vec![(1u8, 1u8)];
            if spec.lit == Lit::NonZero && spec.states >= 3 {
                states.push((1, 2));
            }
            let mut out = vec![];
            for d in &representatives {
                let cells = pair_seed(d);
                for &(a, b) in &states {
                    out.push(Seed {
                        cells: vec![(cells[0].clone(), a), (cells[1].clone(), b)],
                    });
                }
            }
            Ok(out)
        }
    }
}

struct Shared {
    counter: AtomicU64,
    limit: u64,
}

/// A search node handed to another thread.
struct Node {
    t: usize,
    config: HashMap<i64, u8>,
    rule: HashMap<u64, u8>,
    highest: u8,
}

struct Search<'a> {
    spec: &'a Spec,
    target: &'a [usize],
    shared: &'a Shared,
    rule: HashMap<u64, u8>,
    /// Highest state used so far (in the seed or as an output).
    highest: u8,
    canon: HashMap<u64, u64>,
    nodes: u64,
    pending: u64,
    stopped: bool,
    best: usize,
    witness: Vec<(u64, u8)>,
    /// min_entries[L]: fewest entries consulted by a node that reproduces the first L terms.
    min_entries: Vec<usize>,
    split_at: Option<usize>,
    frontier: Vec<Node>,
    /// Set when the split depth has more nodes than FRONTIER_CAP.
    overflow: bool,
}

/// Most nodes handed to other threads from one seed.
const FRONTIER_CAP: usize = 50_000;
/// Nodes counted locally before they are added to the shared count (and the limit checked).
const BATCH: u64 = 64;

impl<'a> Search<'a> {
    fn new(
        spec: &'a Spec,
        target: &'a [usize],
        shared: &'a Shared,
        rule: HashMap<u64, u8>,
        highest: u8,
        split_at: Option<usize>,
    ) -> Self {
        Search {
            spec,
            target,
            shared,
            rule,
            highest,
            canon: HashMap::new(),
            nodes: 0,
            pending: 0,
            stopped: false,
            best: 0,
            witness: vec![],
            min_entries: vec![usize::MAX; target.len() + 1],
            split_at,
            frontier: vec![],
            overflow: false,
        }
    }

    fn tick(&mut self) -> bool {
        self.nodes += 1;
        self.pending += 1;
        if self.pending >= BATCH {
            let total = self
                .shared
                .counter
                .fetch_add(self.pending, Ordering::Relaxed)
                + self.pending;
            self.pending = 0;
            if total >= self.shared.limit {
                self.stopped = true;
            }
        }
        !self.stopped
    }

    fn flush(&mut self) {
        self.shared
            .counter
            .fetch_add(self.pending, Ordering::Relaxed);
        self.pending = 0;
    }

    fn visit(&mut self, t: usize, config: &HashMap<i64, u8>) {
        if !self.tick() {
            return;
        }
        let terms = t + 1;
        let used = self.rule.len();
        self.min_entries[terms] = self.min_entries[terms].min(used);
        if terms > self.best {
            self.best = terms;
            let mut witness: Vec<(u64, u8)> = self.rule.iter().map(|(&k, &v)| (k, v)).collect();
            witness.sort_unstable();
            self.witness = witness;
        }
        if terms == self.target.len() {
            return;
        }
        if self.split_at == Some(t) {
            if self.frontier.len() >= FRONTIER_CAP {
                self.overflow = true;
                self.stopped = true;
                return;
            }
            self.frontier.push(Node {
                t,
                config: config.clone(),
                rule: self.rule.clone(),
                highest: self.highest,
            });
            return;
        }
        self.expand(t, config);
    }

    /// The cells that can be non-empty after the step, grouped by table entry.
    fn classify(&mut self, config: &HashMap<i64, u8>) -> Vec<(u64, Vec<i64>)> {
        let spec = self.spec;
        let mut patterns: HashMap<i64, u64> =
            HashMap::with_capacity(config.len() * (spec.deltas.len() + 1));
        for (&cell, &state) in config {
            patterns.entry(cell).or_insert(0);
            for (position, &d) in spec.deltas.iter().enumerate() {
                // The cell at cell - d sees this one at neighbour position `position`.
                *patterns.entry(cell - d).or_insert(0) += spec.contribution(state, position);
            }
        }
        let mut groups: HashMap<u64, Vec<i64>> = HashMap::new();
        for (cell, pattern) in patterns {
            let own = config.get(&cell).copied().unwrap_or(0);
            let key = spec.key(own, pattern, &mut self.canon);
            groups.entry(key).or_default().push(cell);
        }
        groups.into_iter().collect()
    }

    fn expand(&mut self, t: usize, config: &HashMap<i64, u8>) {
        let groups = self.classify(config);
        let (mut known, mut filled) = (0usize, 0usize);
        let mut open: Vec<usize> = vec![];
        for (index, (key, cells)) in groups.iter().enumerate() {
            match self.rule.get(key) {
                Some(&state) => {
                    if state != 0 {
                        filled += cells.len();
                    }
                    if self.spec.is_lit(state) {
                        known += cells.len();
                    }
                }
                None => open.push(index),
            }
        }
        let goal = self.target[t + 1];
        if known > goal
            || filled + (goal - known) > self.spec.max_cells
            || self.rule.len() + open.len() > self.spec.max_entries
        {
            return;
        }
        open.sort_unstable_by(|&a, &b| {
            groups[b]
                .1
                .len()
                .cmp(&groups[a].1.len())
                .then(groups[a].0.cmp(&groups[b].0))
        });
        let sizes: Vec<usize> = open.iter().map(|&g| groups[g].1.len()).collect();
        let reach = reachable_sums(&sizes, goal - known);
        if !reach[0].contains(goal - known) {
            return;
        }
        self.assign(t, &groups, &open, &reach, 0, goal - known, filled);
    }

    /// Decide the open entries one by one; `need` lit cells must still come from entries i.., and
    /// a choice is only made if those entries can still supply exactly that many. `filled` counts
    /// the non-empty cells so far; `filled + need` never exceeds the cap, so every branch ends in a
    /// configuration that is visited (and counted against the node limit).
    #[allow(clippy::too_many_arguments)]
    fn assign(
        &mut self,
        t: usize,
        groups: &[(u64, Vec<i64>)],
        open: &[usize],
        reach: &[Sums],
        i: usize,
        need: usize,
        filled: usize,
    ) {
        if self.stopped {
            return;
        }
        if i == open.len() {
            let mut next = HashMap::with_capacity(filled);
            for (key, cells) in groups {
                let state = self.rule[key];
                if state != 0 {
                    for &cell in cells {
                        next.insert(cell, state);
                    }
                }
            }
            debug_assert!(next.len() == filled && filled <= self.spec.max_cells);
            self.visit(t + 1, &next);
            return;
        }
        let (key, size) = (groups[open[i]].0, groups[open[i]].1.len());
        // A state not used yet may only be the next unused one.
        let top = (self.highest + 1).min(self.spec.states - 1);
        for state in 0..=top {
            let (rest, added) = if state == 0 {
                (need, 0)
            } else if self.spec.is_lit(state) {
                if size > need {
                    continue;
                }
                (need - size, size)
            } else {
                if filled + size + need > self.spec.max_cells {
                    continue;
                }
                (need, size)
            };
            if !reach[i + 1].contains(rest) {
                continue;
            }
            let saved = self.highest;
            self.highest = self.highest.max(state);
            self.rule.insert(key, state);
            self.assign(t, groups, open, reach, i + 1, rest, filled + added);
            self.rule.remove(&key);
            self.highest = saved;
        }
    }
}

/// The exact smallest rule for each prefix: for L = 2, 3, ... the fewest table entries any rule of
/// the class consults to reproduce the first L terms, found by searching with a growing entry bound.
/// Every value is exact: each smaller bound was ruled out by a complete search (and the minimum
/// never decreases as the prefix grows). The profile stops at the first prefix whose bound could not
/// be settled within `node_limit` nodes per search.
pub struct ProfilePoint {
    pub terms: usize,
    pub entries: usize,
    pub nodes: u64,
}

pub fn profile(
    spec: &mut Spec,
    start: Start,
    target: &[usize],
    node_limit: u64,
    mut report: impl FnMut(&ProfilePoint),
) -> Result<Vec<ProfilePoint>, String> {
    let saved = spec.max_entries;
    let mut points = vec![];
    let mut entries = 0;
    for terms in 2..=target.len() {
        let mut nodes = 0;
        let found = loop {
            spec.max_entries = entries;
            let result = search_target(spec, start, target[..terms].to_vec(), node_limit)?;
            nodes += result.nodes;
            if result.best() >= terms {
                break true;
            }
            if !result.complete {
                break false;
            }
            entries += 1;
            if entries as u128 > spec.entries() || entries > saved {
                break false;
            }
        };
        if !found {
            break;
        }
        let point = ProfilePoint {
            terms,
            entries,
            nodes,
        };
        report(&point);
        points.push(point);
    }
    spec.max_entries = saved;
    Ok(points)
}

/// A set of sums 0..=limit as a bitset.
#[derive(Clone)]
struct Sums(Vec<u64>);

impl Sums {
    fn contains(&self, value: usize) -> bool {
        self.0
            .get(value / 64)
            .is_some_and(|word| (word >> (value % 64)) & 1 == 1)
    }
}

/// reach[i]: the totals (up to `limit`) that some subset of sizes[i..] adds up to.
fn reachable_sums(sizes: &[usize], limit: usize) -> Vec<Sums> {
    let words = limit / 64 + 1;
    let mut reach = vec![Sums(vec![0u64; words]); sizes.len() + 1];
    reach[sizes.len()].0[0] = 1;
    for i in (0..sizes.len()).rev() {
        let mut bits = reach[i + 1].0.clone();
        let size = sizes[i];
        if size <= limit {
            let (shift_words, shift_bits) = (size / 64, size % 64);
            for w in (shift_words..words).rev() {
                let source = w - shift_words;
                let mut moved = reach[i + 1].0[source] << shift_bits;
                if shift_bits > 0 && source > 0 {
                    moved |= reach[i + 1].0[source - 1] >> (64 - shift_bits);
                }
                bits[w] |= moved;
            }
        }
        reach[i] = Sums(bits);
    }
    reach
}

#[derive(Clone, Debug)]
pub struct SeedResult {
    pub seed: Seed,
    /// Terms of the target reproduced (the seed's own count is the first).
    pub best: usize,
    /// The entries consulted by a rule reaching `best` terms.
    pub witness: Vec<(u64, u8)>,
    /// min_entries[L]: fewest table entries a rule consults to reproduce the first L terms.
    pub min_entries: Vec<Option<usize>>,
    pub nodes: u64,
}

pub struct SearchResult {
    pub target: Vec<usize>,
    pub seeds: Vec<SeedResult>,
    pub nodes: u64,
    /// The search covered every rule of the class (no node limit was hit).
    pub complete: bool,
    pub elapsed: Duration,
}

impl SearchResult {
    pub fn best(&self) -> usize {
        self.seeds.iter().map(|s| s.best).max().unwrap_or(0)
    }

    pub fn best_seed(&self) -> Option<&SeedResult> {
        let best = self.best();
        self.seeds.iter().find(|s| s.best == best)
    }

    /// Fewest table entries over all seeds, per number of terms reproduced.
    pub fn min_entries(&self) -> Vec<Option<usize>> {
        (0..=self.target.len())
            .map(|terms| self.seeds.iter().filter_map(|s| s.min_entries[terms]).min())
            .collect()
    }
}

/// The search from one seed down to `depth`, with the nodes there set aside for other threads.
struct Probe {
    best: usize,
    witness: Vec<(u64, u8)>,
    min_entries: Vec<usize>,
    nodes: u64,
    /// The node limit was hit before `depth` (overflows are not counted as stops).
    stopped: bool,
    overflow: bool,
    frontier: Vec<Node>,
}

fn probe(
    spec: &Spec,
    target: &[usize],
    config: &HashMap<i64, u8>,
    highest: u8,
    depth: usize,
    node_limit: u64,
) -> Probe {
    let shared = Shared {
        counter: AtomicU64::new(0),
        limit: node_limit,
    };
    let mut search = Search::new(spec, target, &shared, HashMap::new(), highest, Some(depth));
    search.visit(0, config);
    search.flush();
    Probe {
        best: search.best,
        witness: search.witness,
        min_entries: search.min_entries,
        nodes: search.nodes,
        stopped: search.stopped && !search.overflow,
        overflow: search.overflow,
        frontier: search.frontier,
    }
}

/// Exhaustive search of a class from one lit cell or from every close pair, for the first `terms`
/// terms of the primes, stopping after about `node_limit` nodes.
pub fn search(
    spec: &Spec,
    start: Start,
    terms: usize,
    node_limit: u64,
) -> Result<SearchResult, String> {
    search_target(spec, start, target_for("primes", start, terms)?, node_limit)
}

/// A named sequence (primes, naturals, odds, squares, ...) aligned to the seed: from 1 for one cell,
/// from 2 for a pair.
pub fn target_for(name: &str, start: Start, terms: usize) -> Result<Vec<usize>, String> {
    let first = match start {
        Start::One => 1,
        Start::Pair => 2,
    };
    parse_target_from(name, terms, first)
}

/// As `search`, for any target counts (target[0] is the seed's count).
pub fn search_target(
    spec: &Spec,
    start: Start,
    target: Vec<usize>,
    node_limit: u64,
) -> Result<SearchResult, String> {
    let seeds = seeds(spec, start)?;
    if let Some(seed) = seeds.first() {
        let lit = seed.cells.iter().filter(|&&(_, s)| spec.is_lit(s)).count();
        if target.first() != Some(&lit) {
            return Err(format!("the target must start with {} (the seed)", lit));
        }
    }
    let started = Instant::now();
    let shared = Shared {
        counter: AtomicU64::new(0),
        limit: node_limit,
    };
    let wanted = 8 * rayon::current_num_threads();
    let mut results: Vec<SeedResult> = vec![];
    let mut tasks: Vec<(usize, Node)> = vec![];
    let mut complete = true;
    // Expand each seed far enough to give every thread work: the shallowest depth with enough
    // nodes, or the one before it when that depth has too many to hand out.
    for (index, seed) in seeds.iter().enumerate() {
        let config = seed.config();
        let mut previous: Option<Probe> = None;
        let mut depth = 0;
        let outcome = loop {
            let current = probe(
                spec,
                &target,
                &config,
                seed.highest_state(),
                depth,
                node_limit,
            );
            if current.overflow {
                if let Some(done) = previous.take() {
                    break done;
                }
            }
            let enough = current.frontier.is_empty()
                || current.frontier.len() >= wanted
                || depth + 2 >= target.len()
                || current.stopped;
            if enough {
                break current;
            }
            previous = Some(current);
            depth += 1;
        };
        shared.counter.fetch_add(outcome.nodes, Ordering::Relaxed);
        complete &= !outcome.stopped;
        results.push(SeedResult {
            seed: seed.clone(),
            best: outcome.best,
            witness: outcome.witness,
            min_entries: outcome
                .min_entries
                .iter()
                .map(|&e| (e != usize::MAX).then_some(e))
                .collect(),
            nodes: outcome.nodes,
        });
        tasks.extend(outcome.frontier.into_iter().map(|node| (index, node)));
    }
    let outcomes: Vec<(usize, usize, Vec<(u64, u8)>, Vec<usize>, u64, bool)> = tasks
        .into_par_iter()
        .map(|(index, node)| {
            let mut search = Search::new(spec, &target, &shared, node.rule, node.highest, None);
            if shared.counter.load(Ordering::Relaxed) >= shared.limit {
                search.stopped = true;
            } else {
                search.expand(node.t, &node.config);
                search.flush();
            }
            (
                index,
                search.best,
                search.witness,
                search.min_entries,
                search.nodes,
                search.stopped,
            )
        })
        .collect();
    for (index, best, witness, min_entries, nodes, stopped) in outcomes {
        let result = &mut results[index];
        complete &= !stopped;
        result.nodes += nodes;
        if best > result.best {
            result.best = best;
            result.witness = witness;
        }
        for (slot, &entries) in result.min_entries.iter_mut().zip(&min_entries) {
            if entries != usize::MAX {
                *slot = Some(slot.map_or(entries, |e| e.min(entries)));
            }
        }
    }
    let nodes = results.iter().map(|r| r.nodes).sum();
    if nodes >= node_limit {
        complete = false;
    }
    Ok(SearchResult {
        target,
        seeds: results,
        nodes,
        complete,
        elapsed: started.elapsed(),
    })
}

#[cfg(test)]
/// Replays a witness on a plain simulator (coordinates as vectors, every symmetry applied to the
/// actual offsets) and returns the counts after each step. Panics when the trajectory needs an entry
/// the witness does not decide.
pub fn replay(spec: &Spec, seed: &Seed, witness: &[(u64, u8)], steps: usize) -> Vec<usize> {
    use std::collections::HashSet;
    let rule: HashMap<u64, u8> = witness.iter().copied().collect();
    let k = spec.states as u64;
    let m = spec.offsets.len();
    let index: HashMap<&Vec<i32>, usize> = spec
        .offsets
        .iter()
        .enumerate()
        .map(|(i, o)| (o, i))
        .collect();
    let add = |a: &[i32], b: &[i32]| -> Vec<i32> { a.iter().zip(b).map(|(x, y)| x + y).collect() };
    let sub = |a: &[i32], b: &[i32]| -> Vec<i32> { a.iter().zip(b).map(|(x, y)| x - y).collect() };
    let mut config: HashMap<Vec<i32>, u8> = seed.cells.iter().cloned().collect();
    let count = |c: &HashMap<Vec<i32>, u8>| c.values().filter(|&&s| spec.is_lit(s)).count();
    let mut counts = vec![count(&config)];
    for _ in 0..steps {
        let mut candidates: HashSet<Vec<i32>> = HashSet::new();
        for cell in config.keys() {
            candidates.insert(cell.clone());
            for o in &spec.offsets {
                candidates.insert(sub(cell, o));
            }
        }
        let mut next = HashMap::new();
        for z in candidates {
            let own = config.get(&z).copied().unwrap_or(0);
            let states: Vec<u8> = spec
                .offsets
                .iter()
                .map(|o| config.get(&add(&z, o)).copied().unwrap_or(0))
                .collect();
            if own == 0 && states.iter().all(|&s| s == 0) {
                continue;
            }
            let key = match spec.class {
                Class::Totalistic => {
                    let mut key = (own as u64) << (COUNT_BITS * (spec.states as u32 - 1));
                    for &s in states.iter().filter(|&&s| s != 0) {
                        key += 1 << (COUNT_BITS * (s as u32 - 1));
                    }
                    key
                }
                Class::Table => {
                    own as u64 * k.pow(m as u32)
                        + states
                            .iter()
                            .enumerate()
                            .map(|(i, &s)| s as u64 * k.pow(i as u32))
                            .sum::<u64>()
                }
                Class::Isotropic => {
                    let canonical = spec
                        .matrices
                        .iter()
                        .map(|matrix| {
                            spec.offsets
                                .iter()
                                .zip(&states)
                                .map(|(o, &s)| {
                                    s as u64 * k.pow(index[&apply_matrix(matrix, o)] as u32)
                                })
                                .sum::<u64>()
                        })
                        .min()
                        .unwrap();
                    own as u64 * k.pow(m as u32) + canonical
                }
            };
            let state = *rule.get(&key).unwrap_or_else(|| {
                panic!("the witness does not decide {}", spec.describe_key(key))
            });
            if state != 0 {
                next.insert(z, state);
            }
        }
        config = next;
        counts.push(count(&config));
    }
    counts
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn class_sizes_match_the_binary_classes() {
        let size = |grid, class, states| {
            Spec::new(grid, class, states, Lit::NonZero)
                .unwrap()
                .entries()
        };
        // Two states: the classes searched by the binary engines (see the README tables).
        assert_eq!(size(Grid::Square, Class::Totalistic, 2), 17);
        assert_eq!(size(Grid::Square, Class::Isotropic, 2), 101);
        assert_eq!(size(Grid::SquareVonNeumann, Class::Totalistic, 2), 9);
        assert_eq!(size(Grid::SquareVonNeumann, Class::Isotropic, 2), 11);
        assert_eq!(size(Grid::Hexagonal, Class::Totalistic, 2), 13);
        assert_eq!(size(Grid::Hexagonal, Class::Isotropic, 2), 25);
        assert_eq!(size(Grid::Cube, Class::Totalistic, 2), 53);
        assert_eq!(size(Grid::CubeVonNeumann, Class::Isotropic, 2), 19);
        assert_eq!(size(Grid::Square, Class::Table, 2), 511);
        // Three states: 3 own states times the ways to split 8 neighbours into 3 states.
        assert_eq!(size(Grid::Square, Class::Totalistic, 3), 3 * 45 - 1);
    }

    #[test]
    fn reachable_sums_are_the_subset_sums() {
        let sizes = [4, 8, 1, 70, 64, 6, 129];
        let limit = 200;
        let reach = reachable_sums(&sizes, limit);
        for i in 0..=sizes.len() {
            let mut expected = vec![false; limit + 1];
            for mask in 0u32..(1 << (sizes.len() - i)) {
                let sum: usize = (0..sizes.len() - i)
                    .filter(|b| mask >> b & 1 == 1)
                    .map(|b| sizes[i + b])
                    .sum();
                if sum <= limit {
                    expected[sum] = true;
                }
            }
            for (value, &want) in expected.iter().enumerate() {
                assert_eq!(reach[i].contains(value), want, "suffix {} sum {}", i, value);
            }
        }
    }

    #[test]
    fn symmetric_rules_never_take_one_cell_to_two() {
        let grids = [
            Grid::Square,
            Grid::SquareVonNeumann,
            Grid::Hexagonal,
            Grid::Cube,
            Grid::CubeVonNeumann,
        ];
        for grid in grids {
            for class in [Class::Totalistic, Class::Isotropic] {
                for states in 2..=4 {
                    for lit in [Lit::NonZero, Lit::One] {
                        let spec = Spec::new(grid, class, states, lit).unwrap();
                        let result = search(&spec, Start::One, 8, u64::MAX).unwrap();
                        assert!(result.complete);
                        assert_eq!(
                            result.best(),
                            1,
                            "{:?} {:?} {} {:?}",
                            grid,
                            class,
                            states,
                            lit
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn two_states_agree_with_the_binary_engines() {
        // Longest runs of 2, 3, 5, ... from every starting pattern, as found by the binary engines
        // (README, "Simple classes, every starting pattern").
        let cases = [
            (Grid::Square, Class::Totalistic, 3),
            (Grid::Square, Class::Isotropic, 3),
            (Grid::SquareVonNeumann, Class::Totalistic, 2),
            (Grid::SquareVonNeumann, Class::Isotropic, 2),
            (Grid::Hexagonal, Class::Totalistic, 5),
            (Grid::Hexagonal, Class::Isotropic, 7),
            (Grid::Cube, Class::Totalistic, 2),
            (Grid::CubeVonNeumann, Class::Totalistic, 2),
            (Grid::CubeVonNeumann, Class::Isotropic, 2),
        ];
        for (grid, class, expected) in cases {
            let spec = Spec::new(grid, class, 2, Lit::NonZero).unwrap();
            let result = search(&spec, Start::Pair, 12, u64::MAX).unwrap();
            assert!(result.complete);
            assert_eq!(result.best(), expected, "{:?} {:?}", grid, class);
        }
    }

    #[test]
    fn witnesses_replay_on_a_plain_simulator() {
        let cases = [
            (Grid::Square, Class::Totalistic, 3, Lit::NonZero),
            (Grid::Hexagonal, Class::Isotropic, 3, Lit::NonZero),
            (Grid::SquareVonNeumann, Class::Isotropic, 3, Lit::One),
            (Grid::SquareVonNeumann, Class::Table, 3, Lit::NonZero),
        ];
        for (grid, class, states, lit) in cases {
            let spec = Spec::new(grid, class, states, lit).unwrap();
            let start = if class.is_symmetric() {
                Start::Pair
            } else {
                Start::One
            };
            let result = search(&spec, start, 10, 2_000_000).unwrap();
            for seed in &result.seeds {
                let counts = replay(&spec, &seed.seed, &seed.witness, seed.best - 1);
                assert_eq!(
                    counts,
                    result.target[..seed.best],
                    "{:?} {:?} {:?}",
                    grid,
                    class,
                    seed.seed
                );
            }
        }
    }

    /// Every rule of the 3-state totalistic class on the line (17 free entries, 3^17 rules), simulated
    /// directly: for each seed, the longest run and the fewest entries consulted per prefix length.
    /// A run also ends when a step leaves more than `max_cells` non-empty cells.
    fn brute_force_line(
        lit: Lit,
        max_cells: usize,
        target: &[usize],
        seeds: &[Seed],
    ) -> Vec<(usize, Vec<Option<usize>>)> {
        // Entry index: own state * 6 + split of the two neighbours (in state 1, in state 2).
        const SPLITS: [(usize, usize); 6] = [(0, 0), (1, 0), (0, 1), (2, 0), (1, 1), (0, 2)];
        const WIDTH: usize = 48;
        const CENTRE: usize = WIDTH / 2;
        assert!(target.len() + 2 < CENTRE);
        let mut split = [[0usize; 3]; 3];
        for (i, &(n1, n2)) in SPLITS.iter().enumerate() {
            split[n1][n2] = i;
        }
        let is_lit = |s: u8| match lit {
            Lit::NonZero => s != 0,
            Lit::One => s == 1,
        };
        let total = 3u64.pow(17);
        let chunks = 3u64.pow(5);
        let per_chunk = total / chunks;
        let empty = || -> Vec<(usize, Vec<usize>)> {
            seeds
                .iter()
                .map(|_| (0, vec![usize::MAX; target.len() + 1]))
                .collect()
        };
        let merged = (0..chunks)
            .into_par_iter()
            .map(|chunk| {
                let mut found = empty();
                let mut rule = [0u8; 18];
                for n in chunk * per_chunk..(chunk + 1) * per_chunk {
                    let mut rest = n;
                    for slot in rule.iter_mut().skip(1) {
                        *slot = (rest % 3) as u8;
                        rest /= 3;
                    }
                    for (index, seed) in seeds.iter().enumerate() {
                        let mut cells = [0u8; WIDTH];
                        let (mut lo, mut hi) = (WIDTH, 0);
                        for (c, s) in &seed.cells {
                            let x = (CENTRE as i32 + c[0]) as usize;
                            cells[x] = *s;
                            lo = lo.min(x);
                            hi = hi.max(x);
                        }
                        let mut used = 0u32;
                        let mut terms = 1;
                        found[index].1[1] = 0;
                        while terms < target.len() {
                            let mut next = [0u8; WIDTH];
                            let (mut lit_count, mut filled) = (0, 0);
                            let (mut new_lo, mut new_hi) = (WIDTH, 0);
                            for x in lo - 1..=hi + 1 {
                                let (l, r) = (cells[x - 1], cells[x + 1]);
                                let n1 = (l == 1) as usize + (r == 1) as usize;
                                let n2 = (l == 2) as usize + (r == 2) as usize;
                                let key = cells[x] as usize * 6 + split[n1][n2];
                                if key != 0 {
                                    used |= 1 << key;
                                    let state = rule[key];
                                    next[x] = state;
                                    if state != 0 {
                                        new_lo = new_lo.min(x);
                                        new_hi = new_hi.max(x);
                                        filled += 1;
                                    }
                                    lit_count += is_lit(state) as usize;
                                }
                            }
                            if lit_count != target[terms] || filled > max_cells {
                                break;
                            }
                            terms += 1;
                            let entries = &mut found[index].1[terms];
                            *entries = (*entries).min(used.count_ones() as usize);
                            cells = next;
                            lo = new_lo;
                            hi = new_hi;
                        }
                        found[index].0 = found[index].0.max(terms);
                    }
                }
                found
            })
            .reduce(empty, |mut a, b| {
                for (x, y) in a.iter_mut().zip(b) {
                    x.0 = x.0.max(y.0);
                    for (p, q) in x.1.iter_mut().zip(y.1) {
                        *p = (*p).min(q);
                    }
                }
                a
            });
        merged
            .into_iter()
            .map(|(best, entries)| {
                (
                    best,
                    entries
                        .into_iter()
                        .map(|e| (e != usize::MAX).then_some(e))
                        .collect(),
                )
            })
            .collect()
    }

    #[test]
    #[cfg_attr(debug_assertions, ignore)]
    fn the_line_with_three_states_agrees_with_brute_force() {
        // A cap of 5 cells cuts runs with hidden helpers short on the line's last reachable step.
        for (lit, max_cells) in [(Lit::NonZero, 1000), (Lit::One, 1000), (Lit::One, 5)] {
            let mut spec = Spec::new(Grid::Line, Class::Totalistic, 3, lit).unwrap();
            spec.max_cells = max_cells;
            for start in [Start::One, Start::Pair] {
                let result = search(&spec, start, 10, u64::MAX).unwrap();
                assert!(result.complete);
                let seeds: Vec<Seed> = result.seeds.iter().map(|s| s.seed.clone()).collect();
                let brute = brute_force_line(lit, max_cells, &result.target, &seeds);
                for (found, (best, entries)) in result.seeds.iter().zip(brute) {
                    let case = (lit, max_cells, &found.seed);
                    assert_eq!(found.best, best, "{:?}", case);
                    assert_eq!(found.min_entries, entries, "{:?}", case);
                }
            }
        }
    }
}
