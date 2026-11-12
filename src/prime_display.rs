/*
* A cellular automaton whose number of set cells is the next prime at every step.
*
* Starting from one set cell, the count of set cells is 1, 2, 3, 5, 7, 11, 13, ... : after step t it is
* the t-th prime p_t. The automaton is two-dimensional, uses the Moore neighbourhood of radius 1 and
* one homogeneous rule. A cell is "set" or not; besides that flag its state carries hidden data
* (integers), so the state set is finite but very large. The count is the number of set cells; the
* machinery that computes the primes consists of unset cells.
*
* Layout. Every positive integer n has its own cell. Row y (y = 1, 2, ...) holds the y consecutive
* numbers S_y, ..., S_y + y - 1 with S_y = y(y-1)/2 + 1, at x = 0, ..., y - 1. Row 1 is the seed
* (n = 1). A new row is created from the one below: the cell above a tile gets n + y, and the cell
* above-right of the row's last tile gets n + y + 1. All cells of a row are created together and wait
* the same number of steps (`pace`) before the next row appears, so rows are always complete; the
* pace keeps the triangle just ahead of the numbers that are needed. n sits at distance about
* sqrt(2n) from the seed.
*
* Flip time. Cell n becomes set at step tau(n) = pi(n - 1) + 1 and stays set. Then after step t the
* set cells are exactly n = 1, ..., p_t (n <= p_t  <=>  pi(n - 1) < t), so the count is p_t.
*
* Computing tau(n) locally, for every n beyond the bootstrap rows:
*  1. primality: the cell tries divisors d = 2, 3, ... one per step until d^2 > n or d | n;
*  2. suffix pass (right to left): suffix(x) = [n_x prime] + suffix(x + 1) counts the primes from the
*     cell to the end of its row; the row's first cell ends up with the row's prime count c_y;
*  3. chain (left edge, bottom to top): the first cell of row y reads P_y = pi(S_y - 1) from the first
*     cell below and publishes P_{y+1} = P_y + c_y = pi(last number of row y);
*  4. broadcast (left to right): every cell of the row copies P_{y+1} from its left neighbour and
*     computes tau = P_{y+1} - suffix + 1, since pi(n - 1) = P_{y+1} - suffix.
* Each cell also carries the global time (the creation step of its row plus one per step) and turns
* set when the time reaches tau. This takes about 3y steps after row y appears, while tau(S_y) grows
* like y^2 / (4 ln y), so the machinery is on time from some row on. The rows below that (numbers
* up to BOOTSTRAP_LAST) read tau from a finite table that is part of the rule.
*
* Integers are 64-bit words. `timing_certificate` checks every row whose numbers fit in 64 bits
* (rows up to 6,074,000,998): each tile learns tau before it is due, the tightest by 26 steps (row
* 52). Row 6,074,001,000 would overflow 64 bits; it appears at step 114,179,426,570,746,131, so this
* rule is exact for every step before that one. `prime_columns` extends it to a finite alphabet that
* is exact at every step.
*/

use std::sync::OnceLock;

/// Rows whose numbers all read their flip time from the table: rows 1..=BOOTSTRAP_ROWS.
pub const BOOTSTRAP_ROWS: u64 = 50;
/// Largest number handled by the table: the last number of row BOOTSTRAP_ROWS.
pub const BOOTSTRAP_LAST: u64 = BOOTSTRAP_ROWS * (BOOTSTRAP_ROWS + 1) / 2;
/// Row y waits about y / (PACE_DIVISOR * bit length of y) steps before it creates row y + 1.
pub const PACE_DIVISOR: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tile {
    /// The number this cell stands for.
    pub n: u64,
    /// Its row y (the row holds y numbers).
    pub row: u64,
    /// Global time: the step this state belongs to.
    pub time: u64,
    /// The only visible part of the state.
    pub set: bool,
    /// Next trial divisor (0 for bootstrap cells).
    pub d: u64,
    pub prime: Option<bool>,
    /// Primes among this cell and the cells to its right in the row.
    pub suffix: Option<u64>,
    /// pi(last number of the row).
    pub p_end: Option<u64>,
    /// Flip time pi(n - 1) + 1.
    pub tau: Option<u64>,
    /// Steps left before the row above may be created.
    pub wait: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Tile(Tile),
}

impl Cell {
    pub fn is_set(&self) -> bool {
        matches!(self, Cell::Tile(t) if t.set)
    }
}

/// pi(m) for m <= BOOTSTRAP_LAST (the finite table built into the rule).
fn bootstrap_pi() -> &'static [u64] {
    static TABLE: OnceLock<Vec<u64>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let limit = BOOTSTRAP_LAST as usize;
        let mut composite = vec![false; limit + 1];
        let mut pi = vec![0u64; limit + 1];
        let mut count = 0;
        for m in 2..=limit {
            if !composite[m] {
                count += 1;
                let mut k = m * m;
                while k <= limit {
                    composite[k] = true;
                    k += m;
                }
            }
            pi[m] = count;
        }
        pi
    })
}

fn row_last(row: u64) -> u64 {
    row * (row + 1) / 2
}

/// Extra steps row `row` waits before creating the next row. Rows of the table come one per step
/// (their numbers are needed almost at once); later rows slow down to roughly the rate at which their
/// numbers are needed (about y / (2 ln y) steps per row), with a safety factor.
pub fn pace(row: u64) -> u64 {
    if row <= BOOTSTRAP_ROWS {
        0
    } else {
        let bits = 64 - row.leading_zeros() as u64;
        row / (PACE_DIVISOR * bits)
    }
}

/// A freshly created cell for number n in row `row` at global time `time`.
fn new_tile(n: u64, row: u64, time: u64) -> Tile {
    new_tile_offset(n, row, time, 0)
}

/// `new_tile` for a clock that reads `time - k` (`k` = 0 is this automaton; `prime_columns` tests
/// use k > 0 to give slow test machinery a head start).
pub(crate) fn new_tile_offset(n: u64, row: u64, time: u64, k: u64) -> Tile {
    let mut tile = Tile {
        n,
        row,
        time,
        set: false,
        d: 2,
        prime: None,
        suffix: None,
        p_end: None,
        tau: None,
        wait: pace(row),
    };
    if row <= BOOTSTRAP_ROWS {
        let pi = bootstrap_pi();
        tile.d = 0;
        tile.tau = Some(if n == 1 { 0 } else { pi[(n - 1) as usize] + 1 });
        tile.p_end = Some(pi[row_last(row) as usize]);
        tile.set = tile.tau.unwrap() + k <= time;
    }
    tile
}

/// The initial configuration: the seed cell (n = 1), the only set cell.
pub fn seed() -> Cell {
    Cell::Tile(new_tile(1, 1, 0))
}

/// The eight Moore neighbours, named by direction (y grows upwards).
#[derive(Clone, Copy)]
pub struct Neighbours {
    pub left: Cell,
    pub right: Cell,
    pub below: Cell,
    pub below_left: Cell,
    pub below_right: Cell,
    pub above: Cell,
    pub above_left: Cell,
    pub above_right: Cell,
}

/// The rule: the next state of a cell from its own state and its Moore neighbours.
pub fn next(centre: Cell, nb: &Neighbours) -> Cell {
    match centre {
        Cell::Empty => match (nb.below, nb.below_left) {
            // The cell above a tile continues its column: S_{y+1} = S_y + y.
            (Cell::Tile(b), _) if b.wait == 0 => {
                Cell::Tile(new_tile(b.n + b.row, b.row + 1, b.time + 1))
            }
            // Above-right of a row's last tile: the new row is one cell longer.
            (Cell::Empty, Cell::Tile(b)) if b.wait == 0 => {
                Cell::Tile(new_tile(b.n + b.row + 1, b.row + 1, b.time + 1))
            }
            _ => Cell::Empty,
        },
        Cell::Tile(c) => Cell::Tile(update(c, nb)),
    }
}

fn update(c: Tile, nb: &Neighbours) -> Tile {
    update_offset(c, nb, 0)
}

/// `update` for a clock that reads `time - k` (see `new_tile_offset`).
pub(crate) fn update_offset(c: Tile, nb: &Neighbours, k: u64) -> Tile {
    let mut t = c;
    t.time = c.time + 1;
    t.wait = c.wait.saturating_sub(1);
    // 1. Primality, one divisor per step.
    if c.prime.is_none() && c.d >= 2 {
        if (c.d as u128) * (c.d as u128) > c.n as u128 {
            t.prime = Some(true);
        } else if c.n % c.d == 0 {
            t.prime = Some(false);
        } else {
            t.d = c.d + 1;
        }
    }
    // 2. Suffix pass, right to left.
    if c.suffix.is_none() {
        if let Some(p) = c.prime {
            match nb.right {
                Cell::Empty => t.suffix = Some(p as u64),
                Cell::Tile(r) => {
                    if let Some(s) = r.suffix {
                        t.suffix = Some(s + p as u64);
                    }
                }
            }
        }
    }
    // 3. Chain on the left edge and 4. broadcast along the row.
    if c.p_end.is_none() {
        match nb.left {
            Cell::Empty => {
                if let (Cell::Tile(b), Some(s)) = (nb.below, c.suffix) {
                    if let Some(p) = b.p_end {
                        t.p_end = Some(p + s);
                    }
                }
            }
            Cell::Tile(l) => t.p_end = l.p_end,
        }
    }
    if c.tau.is_none() {
        if let (Some(p), Some(s)) = (c.p_end, c.suffix) {
            t.tau = Some(p - s + 1);
        }
    }
    if let Some(tau) = t.tau {
        if tau + k <= t.time {
            t.set = true;
        }
    }
    t
}

/// Direct simulation on the triangle. Row y occupies x = 0..y.
pub struct Simulation {
    rows: Vec<Vec<Cell>>,
    pub time: u64,
    /// Smallest tau - time over the cells that computed tau (the machinery), with its number.
    pub min_margin: Option<(i64, u64)>,
}

impl Default for Simulation {
    fn default() -> Self {
        Self::new()
    }
}

impl Simulation {
    pub fn new() -> Self {
        Simulation {
            rows: vec![vec![seed()]],
            time: 0,
            min_margin: None,
        }
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    fn get(&self, x: i64, y: i64) -> Cell {
        if y < 1 || x < 0 {
            return Cell::Empty;
        }
        match self.rows.get((y - 1) as usize) {
            Some(row) => row.get(x as usize).copied().unwrap_or(Cell::Empty),
            None => Cell::Empty,
        }
    }

    fn neighbours(&self, x: i64, y: i64) -> Neighbours {
        Neighbours {
            left: self.get(x - 1, y),
            right: self.get(x + 1, y),
            below: self.get(x, y - 1),
            below_left: self.get(x - 1, y - 1),
            below_right: self.get(x + 1, y - 1),
            above: self.get(x, y + 1),
            above_left: self.get(x - 1, y + 1),
            above_right: self.get(x + 1, y + 1),
        }
    }

    /// One application of the rule to every cell that can be non-empty: the triangle and the cells
    /// next to it (the rule leaves every other empty cell empty).
    pub fn step(&mut self) {
        let height = self.rows.len() as i64 + 1;
        let mut next_rows = Vec::with_capacity(height as usize);
        for y in 1..=height {
            let width = y + 1;
            let mut row = Vec::with_capacity(width as usize);
            for x in 0..width {
                let before = self.get(x, y);
                let after = next(before, &self.neighbours(x, y));
                if let (Cell::Tile(b), Cell::Tile(a)) = (before, after) {
                    if let (None, Some(tau)) = (b.tau, a.tau) {
                        let margin = tau as i64 - a.time as i64;
                        if self.min_margin.map_or(true, |(m, _)| margin < m) {
                            self.min_margin = Some((margin, a.n));
                        }
                    }
                }
                row.push(after);
            }
            while matches!(row.last(), Some(Cell::Empty)) {
                row.pop();
            }
            next_rows.push(row);
        }
        while matches!(next_rows.last(), Some(r) if r.is_empty()) {
            next_rows.pop();
        }
        self.rows = next_rows;
        self.time += 1;
    }

    pub fn set_count(&self) -> u64 {
        self.rows.iter().flatten().filter(|c| c.is_set()).count() as u64
    }

    pub fn cells(&self) -> impl Iterator<Item = (i64, i64, &Cell)> {
        self.rows.iter().enumerate().flat_map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(move |(x, c)| (x as i64, y as i64 + 1, c))
        })
    }
}

/// Exact prime counting below CERTIFICATE_EXACT (a sieve used only by the timing certificate).
const CERTIFICATE_EXACT: u64 = 10_000_000;

fn exact_pi_table() -> &'static Vec<u32> {
    static TABLE: OnceLock<Vec<u32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let limit = CERTIFICATE_EXACT as usize;
        let mut composite = vec![false; limit + 1];
        let mut pi = vec![0u32; limit + 1];
        let mut count = 0u32;
        for m in 2..=limit {
            if !composite[m] {
                count += 1;
                let mut k = m * m;
                while k <= limit {
                    composite[k] = true;
                    k += m;
                }
            }
            pi[m] = count;
        }
        pi
    })
}

/// Lower bound for pi(m): exact below CERTIFICATE_EXACT, and Dusart (2010)
/// pi(x) >= x / (ln x - 1) for x >= 5393 above it (rounded down to absorb floating-point error).
fn pi_lower_bound(m: u64) -> f64 {
    if m <= CERTIFICATE_EXACT {
        return exact_pi_table()[m as usize] as f64;
    }
    let x = m as f64;
    x / (x.ln() - 1.0) * (1.0 - 1e-12) - 1.0
}

/// Timing certificate for the rows after the table. From the rule: row y appears at
/// C(y) = C(y-1) + pace(y-1) + 1; a tile knows whether it is prime after at most floor(sqrt(n)) steps;
/// the suffix pass, the chain and the broadcast each advance one cell per step. So the tile at x
/// learns tau by P_y + x + 1, where P_y = max(P_{y-1}, C(y) + floor(sqrt(last)) + y) + 1 bounds the
/// step at which the row's first tile learns pi(last number of the row). It is on time when that is
/// at most tau = pi(n - 1) + 1, and the tightest tile of a row is its last one. Returns the smallest
/// margin over rows BOOTSTRAP_ROWS + 1 ..= y_max (row, margin), or the first failing row.
pub fn timing_certificate(y_max: u64) -> Result<(u64, f64), u64> {
    let mut created = BOOTSTRAP_ROWS - 1; // C(BOOTSTRAP_ROWS): table rows come one per step
    let mut chain = created; // the table rows know p_end when they appear
    let mut worst: (u64, f64) = (0, f64::INFINITY);
    let mut y = BOOTSTRAP_ROWS;
    while y < y_max {
        created += pace(y) + 1;
        y += 1;
        let last = ((y as u128) * (y as u128 + 1) / 2) as u64;
        let sqrt = (last as f64).sqrt().floor() as u64 + 1;
        chain = chain.max(created + sqrt + y) + 1;
        let known = chain + (y - 1) + 1;
        let deadline = pi_lower_bound(last - 1) + 1.0;
        let margin = deadline - known as f64;
        if margin < 0.0 {
            return Err(y);
        }
        if margin < worst.1 {
            worst = (y, margin);
        }
    }
    Ok(worst)
}

/// The certificate's schedule at row `y` (> BOOTSTRAP_ROWS): (C(y), P_y), the step at which row y
/// appears and the bound P_y on the step at which its first tile knows pi(last number of the row).
pub fn narrow_schedule(y: u64) -> (u128, u128) {
    let mut created = (BOOTSTRAP_ROWS - 1) as u128;
    let mut chain = created;
    let mut row = BOOTSTRAP_ROWS;
    while row < y {
        created += pace(row) as u128 + 1;
        row += 1;
        let last = (row as u128) * (row as u128 + 1) / 2;
        let sqrt = (last as f64).sqrt().floor() as u128 + 1;
        chain = chain.max(created + sqrt + row as u128) + 1;
    }
    (created, chain)
}

/// Step at which row `y` appears: C(y) = C(y-1) + pace(y-1) + 1, C(1) = 0.
pub fn creation_step(y: u64) -> u128 {
    let mut created: u128 = 0;
    for row in 1..y {
        created += pace(row) as u128 + 1;
    }
    created
}

/// Last row whose numbers (and the next row's) fit in 64 bits, i.e. the rows this rule handles.
pub fn last_row_in_64_bits() -> u64 {
    // row_last(y) = y (y + 1) / 2 < 2^64 for y up to about 2^32.5.
    let mut y: u64 = 6_100_000_000;
    while (y as u128) * (y as u128 + 1) / 2 > u64::MAX as u128 {
        y -= 1;
    }
    // Row y + 1 must fit too: it is created (n + row) from row y.
    y - 1
}

/// The same automaton, simulated by events: every cell evolves by `next`, but the rule is only
/// evaluated where the result can differ from "same state, one step later". The global time, the
/// row countdown `wait` and the `set` flag of a tile whose `tau` is known are functions of the time,
/// so they are reconstructed instead of stored; everything else changes only for cells that test
/// divisors, cells next to a cell that changed in the previous step, and the row being created.
/// Rows whose tiles all know `tau` are released (only their first tile is kept for the chain).
pub struct EventSimulation {
    rows: Vec<EventRow>,
    pub time: u64,
    /// taus known so far: tau -> number of tiles; count(t) = tiles with known tau <= t.
    taus: Vec<u64>,
    /// Tiles with known tau <= time.
    reached: u64,
    testing: Vec<(i64, i64)>,
    in_testing: std::collections::HashSet<(i64, i64)>,
    changed: Vec<(i64, i64)>,
    released: usize,
    pub min_margin: Option<(i64, u64)>,
    pub evaluations: u64,
}

struct EventRow {
    created: u64,
    cells: Vec<Cell>,
    pending: usize,
}

impl Default for EventSimulation {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSimulation {
    pub fn new() -> Self {
        let seed = seed();
        let mut sim = EventSimulation {
            rows: vec![EventRow {
                created: 0,
                cells: vec![seed],
                pending: 0,
            }],
            time: 0,
            taus: vec![],
            reached: 0,
            testing: vec![],
            in_testing: std::collections::HashSet::new(),
            changed: vec![],
            released: 0,
            min_margin: None,
            evaluations: 0,
        };
        sim.add_tau(0);
        sim
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    fn add_tau(&mut self, tau: u64) {
        let index = tau as usize;
        if self.taus.len() <= index {
            self.taus.resize(index + 1, 0);
        }
        self.taus[index] += 1;
        if tau <= self.time {
            self.reached += 1;
        }
    }

    /// Number of set cells now: tiles whose tau is known and has been reached.
    pub fn set_count(&self) -> u64 {
        self.reached
    }

    fn get(&self, x: i64, y: i64) -> Cell {
        if y < 1 || x < 0 {
            return Cell::Empty;
        }
        let row = match self.rows.get((y - 1) as usize) {
            Some(row) => row,
            None => return Cell::Empty,
        };
        match row.cells.get(x as usize) {
            Some(Cell::Tile(stored)) => {
                let mut t = *stored;
                t.time = self.time;
                t.wait = pace(t.row).saturating_sub(self.time - row.created);
                t.set = matches!(t.tau, Some(tau) if tau <= self.time);
                Cell::Tile(t)
            }
            _ => Cell::Empty,
        }
    }

    fn neighbours(&self, x: i64, y: i64) -> Neighbours {
        Neighbours {
            left: self.get(x - 1, y),
            right: self.get(x + 1, y),
            below: self.get(x, y - 1),
            below_left: self.get(x - 1, y - 1),
            below_right: self.get(x + 1, y - 1),
            above: self.get(x, y + 1),
            above_left: self.get(x - 1, y + 1),
            above_right: self.get(x + 1, y + 1),
        }
    }

    fn same_data(a: &Cell, b: &Cell) -> bool {
        match (a, b) {
            (Cell::Empty, Cell::Empty) => true,
            (Cell::Tile(p), Cell::Tile(q)) => {
                (p.n, p.row, p.d, p.prime, p.suffix, p.p_end, p.tau)
                    == (q.n, q.row, q.d, q.prime, q.suffix, q.p_end, q.tau)
            }
            _ => false,
        }
    }

    pub fn step(&mut self) {
        let mut todo: Vec<(i64, i64)> =
            Vec::with_capacity(self.testing.len() + 9 * self.changed.len());
        todo.extend(self.testing.iter().copied());
        for &(x, y) in &self.changed {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    todo.push((x + dx, y + dy));
                }
            }
        }
        // The newest row creates the next one when its countdown is over.
        let top = self.rows.len() as i64;
        let newest = &self.rows[top as usize - 1];
        if let Some(Cell::Tile(t)) = newest.cells.first() {
            if pace(t.row) <= self.time - newest.created {
                for x in 0..=top {
                    todo.push((x, top + 1));
                }
            }
        }
        todo.sort_unstable();
        todo.dedup();
        let mut updates: Vec<(i64, i64, Cell)> = vec![];
        for (x, y) in todo {
            // Released rows are complete tiles whose data never changes again.
            if y < 1 || x < 0 || x >= y || ((y - 1) as usize) < self.released {
                continue;
            }
            let before = self.get(x, y);
            let after = next(before, &self.neighbours(x, y));
            self.evaluations += 1;
            if !Self::same_data(&before, &after) {
                updates.push((x, y, after));
            }
        }
        self.time += 1;
        self.reached += self.taus.get(self.time as usize).copied().unwrap_or(0);
        self.changed.clear();
        for (x, y, after) in updates {
            let index = (y - 1) as usize;
            if index == self.rows.len() {
                self.rows.push(EventRow {
                    created: self.time,
                    cells: vec![],
                    pending: 0,
                });
            }
            let (old_tau, new_tile) = {
                let row = &mut self.rows[index];
                if row.cells.len() <= x as usize {
                    row.cells.resize(x as usize + 1, Cell::Empty);
                }
                let old = row.cells[x as usize];
                row.cells[x as usize] = after;
                let old_tau = match old {
                    Cell::Tile(t) => t.tau,
                    Cell::Empty => None,
                };
                (old_tau, matches!(old, Cell::Empty))
            };
            if let Cell::Tile(t) = after {
                if new_tile && t.tau.is_none() {
                    self.rows[index].pending += 1;
                }
                if old_tau.is_none() {
                    if let Some(tau) = t.tau {
                        self.add_tau(tau);
                        if !new_tile {
                            self.rows[index].pending -= 1;
                            let margin = tau as i64 - self.time as i64;
                            if self.min_margin.map_or(true, |(m, _)| margin < m) {
                                self.min_margin = Some((margin, t.n));
                            }
                        }
                    }
                }
            }
            self.changed.push((x, y));
        }
        let rows = &self.rows;
        let in_testing = &mut self.in_testing;
        self.testing.retain(|&(x, y)| {
            let still = matches!(rows[(y - 1) as usize].cells[x as usize], Cell::Tile(t) if t.prime.is_none());
            if !still {
                in_testing.remove(&(x, y));
            }
            still
        });
        for &(x, y) in &self.changed {
            if let Cell::Tile(t) = self.rows[(y - 1) as usize].cells[x as usize] {
                if t.prime.is_none() && t.d >= 2 && self.in_testing.insert((x, y)) {
                    self.testing.push((x, y));
                }
            }
        }
        self.release_finished_rows();
    }

    /// Rows below the active band never change again except for their (reconstructed) set flags:
    /// keep only the first tile, which the chain of the row above may still read.
    fn release_finished_rows(&mut self) {
        while self.released + 2 < self.rows.len() {
            let row = &self.rows[self.released];
            let complete = row.pending == 0 && !row.cells.is_empty();
            let above_has_chain = matches!(self.rows[self.released + 1].cells.first(), Some(Cell::Tile(t)) if t.p_end.is_some());
            if !(complete && above_has_chain) {
                break;
            }
            let first = self.rows[self.released].cells[0];
            self.rows[self.released].cells = vec![first];
            self.released += 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn primes(limit: usize) -> Vec<u64> {
        let mut composite = vec![false; limit + 1];
        let mut out = vec![];
        for m in 2..=limit {
            if !composite[m] {
                out.push(m as u64);
                let mut k = m * m;
                while k <= limit {
                    composite[k] = true;
                    k += m;
                }
            }
        }
        out
    }

    #[test]
    fn count_is_the_next_prime_at_every_step() {
        let primes = primes(20_000);
        let mut sim = Simulation::new();
        assert_eq!(sim.set_count(), 1);
        for t in 1..=600usize {
            sim.step();
            assert_eq!(sim.set_count(), primes[t - 1], "step {}", t);
        }
    }

    #[test]
    fn event_simulation_equals_direct_simulation() {
        let mut direct = Simulation::new();
        let mut events = EventSimulation::new();
        assert_eq!(direct.set_count(), events.set_count());
        for step in 1..=900u64 {
            direct.step();
            events.step();
            assert_eq!(direct.set_count(), events.set_count(), "step {}", step);
            assert_eq!(direct.height(), events.height(), "step {}", step);
            if step % 150 == 0 {
                for (x, y, cell) in direct.cells() {
                    let reconstructed = events.get(x, y);
                    let kept = events.rows[(y - 1) as usize].cells.len() > x as usize;
                    if kept {
                        assert_eq!(*cell, reconstructed, "cell ({}, {}) at step {}", x, y, step);
                    }
                }
            }
        }
        assert_eq!(direct.min_margin, events.min_margin);
    }

    #[test]
    fn every_row_after_the_table_is_on_time_up_to_a_million() {
        let (row, margin) = timing_certificate(1_000_000).unwrap();
        assert!(margin > 0.0 && row > BOOTSTRAP_ROWS);
        // The bound is conservative: the simulated tightest margin is 31 steps, the bound's is smaller.
        let mut sim = EventSimulation::new();
        for _ in 0..3000 {
            sim.step();
        }
        assert!(sim.min_margin.unwrap().0 as f64 >= margin.min(31.0) - 1e-9);
    }

    #[test]
    fn rule_is_local_and_the_layout_is_the_triangle() {
        let mut sim = Simulation::new();
        for _ in 0..40 {
            sim.step();
        }
        for (x, y, cell) in sim.cells() {
            if let Cell::Tile(t) = cell {
                assert_eq!(t.row as i64, y);
                assert_eq!(t.n as i64, y * (y - 1) / 2 + 1 + x);
                assert_eq!(t.time, 40);
            }
        }
    }
}
