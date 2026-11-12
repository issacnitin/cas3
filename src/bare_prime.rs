/*
* From a bare prime to the next one: a rule that takes about sqrt(p) steps, and why none is much faster.
*
* The question. Hand a rule the number p and nothing else: p lit cells in the staircase of
* prime_display (row y = 1, 2, ... holds y cells at x = 0..y-1, numbered 1, 2, 3, ... row by row; the
* input is the cells of 1..p on an empty background). After how many steps T(p) can it show exactly q
* lit cells for good, q the next prime? Constant time is impossible (constant_time). For the hardest
* primes no rule beats about sqrt(p) / 20 steps, and the rule below takes about 2.414 sqrt(p) while the
* gaps between primes are small next to sqrt(p).
*
* Lower bound (any rule of radius r, any states, even infinitely many). For large P some prime p <= P
* has T(p) >= (1 - o(1)) sqrt(P) / (20 r).
*
*  1. Census. After T steps a cell depends only on the input in the box of radius R = r T around it.
*     The input has two states (lit, empty), and an all-empty box gives an empty cell, so the count
*     after T steps is a sum over box contents: [content ends lit] * N_content(p).
*  2. Write p = Y (Y + 1) / 2 + t with 1 <= t <= Y + 1: rows 1..Y are full and row Y + 1 holds t
*     cells. In the middle regime (Y >= 8 R + 8, 2 R + 3 <= t <= Y - 4 R - 4) every N_content is
*     a Y^2 + b Y + c t + e with an integer c: the boxes that see the end of the partial row see no
*     other corner, and every other box sees one straight edge, a fixed corner or nothing. So the
*     count is A Y^2 + B Y + C t + D (checked on random rules by a test below).
*  3. Let T be the largest T(p) over the primes p <= P. For consecutive primes p_i < p_{i+1} in the
*     middle of the same row, t_{i+1} - t_i = p_{i+1} - p_i = count - p_i, so t_{i+1} = C t_i + K
*     with K depending only on the row. C <= 0 allows at most 2 of them. C = 1 makes them an
*     arithmetic progression of consecutive primes: every prime up to their number divides the
*     difference, so there are at most (1 + o(1)) ln Y. C >= 2 makes t + K / (C - 1) grow
*     geometrically: at most 2 + log2 Y. So the middle of each row holds O(ln P) primes.
*  4. The primes in (P/2, P] lie in about (sqrt 2 - 1) sqrt P rows. The two ends of a row (t < 2 R + 3
*     and t > Y - 4 R - 4) have 2 R + 2 and 4 R + 5 numbers, and N > 1 consecutive numbers hold fewer
*     than 2 N / ln N primes (Montgomery and Vaughan, The large sieve, 1973). As (P/2, P] holds
*     (1 + o(1)) P / (2 ln P) primes, (sqrt 2 - 1) sqrt P (12 R + 14) / ln(2 R + 2) must reach that,
*     which needs 2 R + 2 >= (1 - o(1)) sqrt(P) / 10. (If Y >= 8 R + 8 fails, R > sqrt(P) / 9.)
*
* On a line of p cells the count is a p + e (a in {0, 1}) once p >= 2 R, so the gaps would be
* constant: some prime p <= P needs T(p) >= P / (4 r). In d dimensions, on a d-dimensional staircase
* filled row by row, the same argument should give about p^(1/d) (not written out).
*
* Construction (2D, radius 1, cells holding 64-bit numbers like prime_display, exact while the
* numbers fit; it reads 5 of the 8 neighbours):
*  * numbering: the apex (nothing below or below-left) is 1 in row 1. A cell above a numbered cell of
*    row y gets n + y; the last cell of a row (nothing below) gets n + y + 1 from below-left. Row y
*    is numbered at step y.
*  * top row: the first cell of the top row (nothing to its left or above) starts a flag that moves
*    right. The flagged cell with nothing to its right is the last one, p, from step row(p).
*  * candidates: the last cell starts a line of candidates p + 1, p + 2, ... to its right, one every
*    two steps. Each tries one divisor per step, like prime_display.
*  * token: a candidate lights up when its left neighbour holds the token (the last cell, or a lit
*    composite) and its own answer is known. A composite passes the token on. The first prime keeps
*    it and sends a stop signal right, which catches the line (the line grows at half speed).
* The lit cells end as 1..q, for good, at step T(p) = row(p) + 2 (q - p) + floor(sqrt q) exactly
* (`settle_time`; every composite candidate is decided before q is). That is about
* (1 + sqrt 2) sqrt(p) + 2 (q - p): row(p) ~ sqrt(2 p) steps to read p, sqrt(q) to test q. It is
* O(sqrt p) whenever q - p = O(sqrt p), as for every prime up to 10^11 (T(p) <= 5.67 sqrt p there)
* and as Cramer's conjecture predicts; unconditionally q - p = O(p^0.525) (Baker, Harman, Pintz).
*/

use rayon::prelude::*;

/// One of the p input cells (always lit).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input {
    /// Its number, 0 until the numbering wave arrives.
    pub n: u64,
    /// Its row, 0 until known.
    pub row: u64,
    /// In the top row: set from the row's first cell rightwards.
    pub top: bool,
    /// The last input cell, which holds p.
    pub last: bool,
}

/// A candidate p + j, in the line to the right of the last input cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub n: u64,
    /// Next trial divisor.
    pub d: u64,
    pub prime: Option<bool>,
    /// Every number from p + 1 to n is composite, or n is the first prime after p.
    pub lit: bool,
    /// n is the first prime after p.
    pub found: bool,
    /// The line stops growing.
    pub stop: bool,
    /// Steps before this candidate may create the next one.
    pub wait: u8,
}

impl Candidate {
    fn new(n: u64) -> Self {
        Candidate {
            n,
            d: 2,
            prime: None,
            lit: false,
            found: false,
            stop: false,
            wait: 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Input(Input),
    Candidate(Candidate),
}

impl Cell {
    pub fn is_lit(&self) -> bool {
        match self {
            Cell::Empty => false,
            Cell::Input(_) => true,
            Cell::Candidate(c) => c.lit,
        }
    }
}

/// The neighbours the rule reads (y grows upwards).
#[derive(Clone, Copy)]
pub struct Neighbours {
    pub left: Cell,
    pub right: Cell,
    pub below: Cell,
    pub below_left: Cell,
    pub above: Cell,
}

/// The rule: the next state of a cell from its own state and its neighbours.
pub fn next(centre: Cell, nb: &Neighbours) -> Cell {
    match centre {
        Cell::Empty => match nb.left {
            Cell::Input(l) if l.last => Cell::Candidate(Candidate::new(l.n + 1)),
            Cell::Candidate(l) if l.wait == 0 && !l.stop => {
                Cell::Candidate(Candidate::new(l.n + 1))
            }
            _ => Cell::Empty,
        },
        Cell::Input(c) => Cell::Input(update_input(c, nb)),
        Cell::Candidate(c) => Cell::Candidate(update_candidate(c, nb)),
    }
}

fn update_input(c: Input, nb: &Neighbours) -> Input {
    let mut o = c;
    if c.n == 0 {
        match (nb.below, nb.below_left) {
            (Cell::Input(b), _) if b.n > 0 => {
                o.n = b.n + b.row;
                o.row = b.row + 1;
            }
            (Cell::Empty, Cell::Input(b)) if b.n > 0 => {
                o.n = b.n + b.row + 1;
                o.row = b.row + 1;
            }
            (Cell::Empty, Cell::Empty) => {
                o.n = 1;
                o.row = 1;
            }
            _ => {}
        }
    }
    if !c.top {
        o.top = match nb.left {
            Cell::Empty => nb.above == Cell::Empty,
            Cell::Input(l) => l.top,
            Cell::Candidate(_) => false,
        };
    }
    if !c.last && o.top && o.n > 0 && nb.right == Cell::Empty {
        o.last = true;
    }
    o
}

fn update_candidate(c: Candidate, nb: &Neighbours) -> Candidate {
    let mut o = c;
    o.wait = c.wait.saturating_sub(1);
    if c.prime.is_none() {
        if (c.d as u128) * (c.d as u128) > c.n as u128 {
            o.prime = Some(true);
        } else if c.n % c.d == 0 {
            o.prime = Some(false);
        } else {
            o.d = c.d + 1;
        }
    }
    let (token, stop) = match nb.left {
        Cell::Input(l) => (l.last, false),
        Cell::Candidate(l) => (l.lit && !l.found, l.stop),
        Cell::Empty => (false, false),
    };
    if token && !c.lit {
        match c.prime {
            Some(false) => o.lit = true,
            Some(true) => {
                o.lit = true;
                o.found = true;
                o.stop = true;
            }
            None => {}
        }
    }
    if stop {
        o.stop = true;
    }
    o
}

pub fn isqrt(n: u64) -> u64 {
    let mut x = (n as f64).sqrt() as u64;
    while (x as u128) * (x as u128) > n as u128 {
        x -= 1;
    }
    while ((x + 1) as u128) * ((x + 1) as u128) <= n as u128 {
        x += 1;
    }
    x
}

/// The row of n in the staircase: the y with y (y - 1) / 2 < n <= y (y + 1) / 2.
pub fn row_of(n: u64) -> u64 {
    let triangle = |y: u64| y as u128 * (y as u128 + 1) / 2;
    let mut y = ((2.0 * n as f64).sqrt() as u64).max(1);
    while triangle(y) < n as u128 {
        y += 1;
    }
    while y > 1 && triangle(y - 1) >= n as u128 {
        y -= 1;
    }
    y
}

/// The step from which the construction shows exactly q lit cells, q the first prime after p.
pub fn settle_time(p: u64, q: u64) -> u64 {
    row_of(p) + 2 * (q - p) + isqrt(q)
}

/// The construction run directly on the staircase of p lit cells.
pub struct Run {
    /// rows[y - 1] is row y.
    rows: Vec<Vec<Cell>>,
    pub step: u64,
}

impl Run {
    /// The staircase of the numbers 1..=p, all lit, and nothing else.
    pub fn new(p: u64) -> Self {
        let top = row_of(p);
        let rows = (1..=top)
            .map(|y| {
                let len = if y < top { y } else { p - y * (y - 1) / 2 };
                vec![Cell::Input(Input::default()); len as usize]
            })
            .collect();
        Run { rows, step: 0 }
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
            above: self.get(x, y + 1),
        }
    }

    /// One application of the rule to every cell that can change (every cell of a row and the one
    /// after it; every other empty cell has an empty left neighbour and stays empty). Returns whether
    /// any cell changed.
    pub fn advance(&mut self) -> bool {
        let mut changed = false;
        let mut rows = Vec::with_capacity(self.rows.len());
        for y in 1..=self.rows.len() as i64 {
            let width = self.rows[(y - 1) as usize].len() as i64 + 1;
            let mut row = Vec::with_capacity(width as usize);
            for x in 0..width {
                let before = self.get(x, y);
                let after = next(before, &self.neighbours(x, y));
                changed |= after != before;
                row.push(after);
            }
            while row.last() == Some(&Cell::Empty) {
                row.pop();
            }
            rows.push(row);
        }
        self.rows = rows;
        self.step += 1;
        changed
    }

    pub fn lit(&self) -> u64 {
        self.rows.iter().flatten().filter(|c| c.is_lit()).count() as u64
    }

    /// The configuration as text, top row first: '#' lit, 'o' unlit candidate.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for row in self.rows.iter().rev() {
            for cell in row {
                out.push(match cell {
                    Cell::Empty => ' ',
                    c if c.is_lit() => '#',
                    _ => 'o',
                });
            }
            out.push('\n');
        }
        out
    }
}

/// What a direct run from the staircase of p cells did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Lit cells at the end.
    pub lit: u64,
    /// The step from which the count no longer changed.
    pub settled: u64,
    /// The first step with no change anywhere (nothing changes after it).
    pub still: u64,
    /// Candidate cells created.
    pub candidates: u64,
}

/// Runs the construction from the staircase of p cells until nothing changes. Fails if the count
/// ever drops or the run does not stop within `max_steps`.
pub fn simulate(p: u64, max_steps: u64) -> Result<Outcome, String> {
    let mut run = Run::new(p);
    let mut lit = run.lit();
    let mut settled = 0;
    loop {
        let changed = run.advance();
        let now = run.lit();
        if now < lit {
            return Err(format!("p = {}: the count dropped at step {}", p, run.step));
        }
        if now != lit {
            lit = now;
            settled = run.step;
        }
        if !changed {
            let candidates = run
                .rows
                .iter()
                .flatten()
                .filter(|c| matches!(c, Cell::Candidate(_)))
                .count() as u64;
            return Ok(Outcome {
                lit,
                settled,
                still: run.step,
                candidates,
            });
        }
        if run.step >= max_steps {
            return Err(format!(
                "p = {}: still changing after {} steps",
                p, max_steps
            ));
        }
    }
}

/// The first prime after n (trial division).
pub fn next_prime(n: u64) -> u64 {
    let is_prime = |m: u64| m >= 2 && (2..).take_while(|d| d * d <= m).all(|d| m % d != 0);
    (n + 1..).find(|&m| is_prime(m)).unwrap()
}

/// T(p) / sqrt(p) over the primes of one decade [from, to).
#[derive(Clone, Copy, Debug)]
pub struct Decade {
    pub from: u64,
    pub to: u64,
    pub primes: u64,
    pub mean: f64,
    pub worst: f64,
    pub worst_at: u64,
}

/// Room after a segment for the next prime of its last primes (prime gaps below 2^64 are under 1,600).
const MARGIN: u64 = 4096;

/// T(p) / sqrt(p), from `settle_time`, for every prime p <= limit, decade by decade.
pub fn decades(limit: u64) -> Vec<Decade> {
    let base: Vec<u64> = {
        let n = isqrt(limit + MARGIN) as usize + 1;
        let mut composite = vec![false; n + 1];
        (2..=n)
            .filter(|&i| {
                if composite[i] {
                    return false;
                }
                (i * i..=n).step_by(i).for_each(|j| composite[j] = true);
                true
            })
            .map(|i| i as u64)
            .collect()
    };
    let segment = 1u64 << 21;
    let starts: Vec<u64> = (0..)
        .map(|j| j * segment)
        .take_while(|&lo| lo <= limit)
        .collect();
    let decade_of = |p: u64| (p as f64).log10().floor() as usize;
    let parts: Vec<Vec<(u64, f64, f64, u64)>> = starts
        .into_par_iter()
        .map(|lo| {
            let hi = (lo + segment).min(limit + 1);
            let end = hi + MARGIN;
            let mut composite = vec![false; (end - lo) as usize];
            for &b in &base {
                if b * b >= end {
                    break;
                }
                let mut m = (b * b).max(lo.div_ceil(b) * b);
                while m < end {
                    composite[(m - lo) as usize] = true;
                    m += b;
                }
            }
            let primes: Vec<u64> = (lo.max(2)..end)
                .filter(|&n| !composite[(n - lo) as usize])
                .collect();
            let mut stats = vec![(0u64, 0f64, 0f64, 0u64); 21];
            for w in primes.windows(2) {
                let (p, q) = (w[0], w[1]);
                if p >= hi {
                    break;
                }
                let ratio = settle_time(p, q) as f64 / (p as f64).sqrt();
                let s = &mut stats[decade_of(p)];
                s.0 += 1;
                s.1 += ratio;
                if ratio > s.2 {
                    s.2 = ratio;
                    s.3 = p;
                }
            }
            stats
        })
        .collect();
    let mut total = vec![(0u64, 0f64, 0f64, 0u64); 21];
    for part in parts {
        for (t, s) in total.iter_mut().zip(part) {
            t.0 += s.0;
            t.1 += s.1;
            if s.2 > t.2 {
                t.2 = s.2;
                t.3 = s.3;
            }
        }
    }
    total
        .into_iter()
        .enumerate()
        .filter(|(_, s)| s.0 > 0)
        .map(|(k, s)| Decade {
            from: 10u64.pow(k as u32),
            to: 10u64.pow(k as u32 + 1).min(limit + 1),
            primes: s.0,
            mean: s.1 / s.0 as f64,
            worst: s.2,
            worst_at: s.3,
        })
        .collect()
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn from_every_staircase_the_count_settles_at_the_next_prime() {
        let outcomes: Vec<(u64, Result<Outcome, String>)> = (1..=500u64)
            .into_par_iter()
            .map(|n| (n, simulate(n, 100_000)))
            .collect();
        for (n, outcome) in outcomes {
            let outcome = outcome.unwrap();
            let q = next_prime(n);
            assert_eq!(outcome.lit, q, "n = {}", n);
            assert_eq!(outcome.settled, settle_time(n, q), "n = {}", n);
            // The line stops: the candidates past q are few.
            assert!(
                outcome.candidates <= 2 * (q - n) + isqrt(q) + 4,
                "n = {}",
                n
            );
        }
    }

    #[test]
    fn rows_and_square_roots() {
        let mut n = 0;
        for y in 1..200u64 {
            for _ in 0..y {
                n += 1;
                assert_eq!(row_of(n), y);
            }
        }
        for m in 0..10_000u64 {
            let r = isqrt(m);
            assert!(r * r <= m && (r + 1) * (r + 1) > m);
        }
        // Rows of 2^32 cells end at 2^63 + 2^31, past the point where 2 n overflows.
        assert_eq!(row_of(1 << 63), 1 << 32);
        assert_eq!(row_of(u64::MAX), 6_074_001_000);
    }

    #[test]
    fn the_decade_table_matches_a_direct_computation() {
        let limit = 200_000;
        let table = decades(limit);
        let mut worst = vec![(0f64, 0u64); 6];
        let mut count = vec![0u64; 6];
        let mut p = 2;
        while p <= limit {
            let q = next_prime(p);
            let ratio = settle_time(p, q) as f64 / (p as f64).sqrt();
            let k = (p as f64).log10().floor() as usize;
            count[k] += 1;
            if ratio > worst[k].0 {
                worst[k] = (ratio, p);
            }
            p = q;
        }
        assert_eq!(table.len(), 6);
        for (k, d) in table.iter().enumerate() {
            assert_eq!(
                (d.primes, d.worst_at),
                (count[k], worst[k].1),
                "decade {}",
                k
            );
        }
    }

    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    /// Lit cells after `steps` steps of a two-state radius-1 rule (table indexed by the 3x3
    /// neighbourhood, table[0] = 0) from the staircase with rows 1..=full and `t` cells above.
    fn census(table: &[u8; 512], full: usize, t: usize, steps: usize) -> u64 {
        let pad = steps + 2;
        let size = full + 1 + 2 * pad;
        let mut grid = vec![0u8; size * size];
        for y in 1..=full + 1 {
            let len = if y <= full { y } else { t };
            for x in 0..len {
                grid[(y - 1 + pad) * size + x + pad] = 1;
            }
        }
        for _ in 0..steps {
            let mut next = vec![0u8; size * size];
            for y in 1..size - 1 {
                for x in 1..size - 1 {
                    let mut index = 0;
                    for (k, (dx, dy)) in (0..9).map(|k| (k % 3, k / 3)).enumerate() {
                        index |= (grid[(y + dy - 1) * size + x + dx - 1] as usize) << k;
                    }
                    next[y * size + x] = table[index];
                }
            }
            grid = next;
        }
        grid.iter().map(|&c| c as u64).sum()
    }

    #[test]
    fn after_a_few_steps_any_rule_counts_affinely_in_the_middle_regime() {
        // Step 2 of the lower bound: in the middle regime the count after T steps is
        // A Y^2 + B Y + C t + D, for any rule. Here: random two-state rules, radius 1, R = T.
        let mut rng = XorShift(0x2545_F491_4F6C_DD1D);
        for steps in 1..=3usize {
            for _ in 0..10 {
                let mut table = [0u8; 512];
                for entry in table.iter_mut().skip(1) {
                    *entry = (rng.next() >> 33) as u8 & 1;
                }
                let r = steps;
                let mut slope: Option<i64> = None;
                let mut rest = vec![];
                for full in 8 * r + 8..8 * r + 13 {
                    let ts: Vec<usize> = (2 * r + 3..=full - 4 * r - 4).collect();
                    let counts: Vec<i64> = ts
                        .iter()
                        .map(|&t| census(&table, full, t, steps) as i64)
                        .collect();
                    for w in counts.windows(2) {
                        assert_eq!(*slope.get_or_insert(w[1] - w[0]), w[1] - w[0]);
                    }
                    rest.push(counts[0] - slope.unwrap() * ts[0] as i64);
                }
                let second: Vec<i64> = rest.windows(3).map(|w| w[2] - 2 * w[1] + w[0]).collect();
                assert!(second.windows(2).all(|w| w[0] == w[1]), "{:?}", second);
            }
        }
    }
}
