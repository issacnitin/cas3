/*
* The prime-counting automaton with a finite alphabet, exact at every step.
*
* `prime_display` is a 2D automaton whose cells hold 64-bit integers, so it is exact only until those
* words overflow. Here every integer that outgrows one word is spread over a column of cells, one
* W-bit digit per cell, so each cell holds a bounded amount of data and the count is the next prime
* forever. The automaton is three-dimensional (Moore neighbourhood, radius 1):
*
*  - The plane z = 0 runs the 2D rule of `prime_display` for the "narrow" rows, whose values fit in
*    one digit (4 y^2 < 2^W). With W = 64 these are the rows below 2^31, i.e. every step before
*    about 1.5e16: on that range this automaton is the 2D one, cell for cell.
*  - Every later ("wide") row keeps the same triangle in the plane z = 0, but each number becomes a
*    column of h = digits(4 y^2 + 2K) cells at z = 0, ..., h - 1 (K = 0 below), least significant
*    digit at the bottom. Only the top cell of a column carries the visible "set" flag.
*
* A wide column computes the same things as a 2D tile, digit by digit:
*  - creation: row y + 1 is built from row y one digit per step (n + y, y + 1 and 4 (y + 1)^2 with
*    carries moving up); a column grows one extra digit when 4 (y + 1)^2 needs it; every value a
*    column holds is below 4 y^2 (+ 2K);
*  - primality: a head walks up and down the column doing trial division, d = 2, 3, ... while
*    d^2 <= n, each remainder by restoring binary long division (one bit of n per round trip);
*  - suffix pass (right to left), chain (up the left edge) and broadcast (left to right) as in 2D,
*    with carries moving up the columns;
*  - the clock is replaced by running counters. The chain and broadcast carry Q = pi(row end) - t
*    (t = the step number), which every column decrements at every step; each column then runs a
*    countdown CD = Q + 1 - suffix = tau - t and turns set when it reads 0.
* Running counters are "skewed": digit k lags k steps behind digit 0, the speed at which borrows climb
* the column, so every operation reads a consistent value. A flag climbing with the borrows ("every
* digit below me is 0") is therefore exact, so the top cell knows exactly when the whole countdown
* reads 0, but h - 1 steps late. To make up for that, when the countdown is set up each digit above the
* bottom sends one extra decrement down to the bottom: the top then sees exactly the current value,
* for every column height. No bound on the height is needed anywhere.
*
* Parameters: W (digit bits) and K (the clock reads t - K). The real automaton is W = 64, K = 0.
* Small W makes columns appear after a few rows so tests can run the wide machinery; for the tiny
* early deadlines it is too slow, so tests give it a head start K > 0: the count after step t is then
* p_(t - K) (0 before step K). Everything else is the same rule.
*/

use std::cmp::Ordering;

use crate::prime_display::{new_tile_offset, update_offset, Cell as PlaneCell, Neighbours, Tile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Params {
    /// Bits per digit, 3..=64.
    pub w: u32,
    /// The clock reads (step - k): 0 in the real automaton.
    pub k: u64,
}

impl Params {
    /// The automaton itself: 64-bit digits, no head start.
    pub const REAL: Params = Params { w: 64, k: 0 };

    fn mask(&self) -> u128 {
        (1u128 << self.w) - 1
    }

    /// Rows whose values all fit in one digit run the 2D rule.
    pub fn is_narrow(&self, row: u64) -> bool {
        4 * (row as u128) * (row as u128) < (1u128 << self.w)
    }

    /// The value that sets the height of the columns of a wide row: 4 row^2 (+ 2K for a head
    /// start, so that counters up to tau + K fit).
    pub fn height_value(&self, row: u64) -> u128 {
        4 * (row as u128) * (row as u128) + 2 * self.k as u128
    }
}

/// Program counter of the primality head (the head itself is the cell that holds it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Head {
    /// Just created: wait one step at the bottom.
    Start,
    /// Up pass: compare d^2 with n (and clear the remainder).
    Cmp {
        ord: Ordering,
    },
    /// Down pass of one division round: carry the pending subtraction `s`, take the next bit of n
    /// at the cursor (`arrives`: the cursor moves into the cell below).
    Down {
        s: bool,
        b: Option<bool>,
        last: bool,
        arrives: bool,
    },
    /// Up pass of one division round: r <- 2 (r - s d) + b, comparing the result with d.
    Up {
        s: bool,
        b: bool,
        borrow: bool,
        shift: bool,
        ord: Ordering,
        last: bool,
    },
    /// After the last bit: down, then r <- r - s d, testing r = 0.
    FinalDown {
        s: bool,
    },
    FinalUp {
        s: bool,
        borrow: bool,
        zero: bool,
    },
    /// Next divisor: down, then d <- d + 1, d^2 <- d^2 + 2 d + 1, comparing d^2 with n.
    IncDown,
    IncUp {
        carry_d: bool,
        carry_dd: u64,
        top_bit: bool,
        ord: Ordering,
    },
    /// Carry the verdict down to the bottom cell.
    Report {
        prime: bool,
    },
}

/// One digit of a wide column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Digit {
    pub n: u64,
    pub row: u64,
    /// Digit of 4 row^2 (+ 2K), which sets the column height.
    pub sq4: u64,
    pub bottom: bool,
    pub top: bool,
    /// Created diagonally (the last column of its row): its source column is below-left.
    pub diag: bool,
    /// Carries out of this digit when it was created, for the digit above.
    pub c_n: u64,
    pub c_row: u64,
    pub c_sq4: u64,
    /// Trial divisor, its square and the remainder.
    pub d: u64,
    pub dd: u64,
    pub r: u64,
    /// Next bit of n to bring down, if it is in this digit.
    pub cursor: Option<u32>,
    pub head: Option<Head>,
    /// Primality verdict (bottom cell).
    pub prime: Option<bool>,
    /// Primes from this column to the end of the row.
    pub suffix: Option<u64>,
    pub c_suffix: u64,
    /// Running counter Q = pi(last number of the row) - clock.
    pub q: Option<u64>,
    pub q_ready: bool,
    pub q_carry: u64,
    pub q_borrow: bool,
    /// Higher digits of a narrow tile's Q, passed up the first column of the first wide row.
    pub q_high: u64,
    /// Running countdown CD = Q + 1 - suffix = tau - clock.
    pub cd: Option<u64>,
    pub cd_ready: bool,
    pub cd_carry: u64,
    pub cd_borrow: bool,
    /// An extra decrement on its way down to the bottom digit.
    pub req: bool,
    /// Every countdown digit below this one is 0 (at this digit's lag).
    pub lz: bool,
    /// The visible flag (top cell).
    pub set: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell3 {
    Empty,
    /// A tile of the 2D rule (narrow rows, z = 0 only).
    Narrow(Tile),
    Digit(Digit),
}

impl Cell3 {
    pub fn is_set(&self) -> bool {
        match self {
            Cell3::Narrow(t) => t.set,
            Cell3::Digit(d) => d.set,
            Cell3::Empty => false,
        }
    }
}

static EMPTY: Cell3 = Cell3::Empty;

/// The 3x3x3 block around a cell.
pub struct Nb<'a> {
    cells: [&'a Cell3; 27],
}

impl<'a> Nb<'a> {
    pub fn from_fn(get: impl Fn(i64, i64, i64) -> &'a Cell3) -> Self {
        let mut cells = [&EMPTY; 27];
        for dz in -1..=1 {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    cells[Self::index(dx, dy, dz)] = get(dx, dy, dz);
                }
            }
        }
        Nb { cells }
    }

    fn index(dx: i64, dy: i64, dz: i64) -> usize {
        ((dz + 1) * 9 + (dy + 1) * 3 + (dx + 1)) as usize
    }

    fn at(&self, dx: i64, dy: i64, dz: i64) -> &'a Cell3 {
        self.cells[Self::index(dx, dy, dz)]
    }

    fn digit(&self, dx: i64, dy: i64, dz: i64) -> Option<&'a Digit> {
        match self.at(dx, dy, dz) {
            Cell3::Digit(d) => Some(d),
            _ => None,
        }
    }
}

/// The rule.
pub fn next(p: &Params, nb: &Nb) -> Cell3 {
    match nb.at(0, 0, 0) {
        Cell3::Narrow(t) => Cell3::Narrow(update_offset(*t, &plane(nb), p.k)),
        Cell3::Digit(d) => Cell3::Digit(step_digit(p, d, nb)),
        Cell3::Empty => create(p, nb),
    }
}

/// What a narrow tile sees: its plane neighbours, columns counting as empty.
fn plane(nb: &Nb) -> Neighbours {
    let f = |dx, dy| match nb.at(dx, dy, 0) {
        Cell3::Narrow(t) => PlaneCell::Tile(*t),
        _ => PlaneCell::Empty,
    };
    Neighbours {
        left: f(-1, 0),
        right: f(1, 0),
        below: f(0, -1),
        below_left: f(-1, -1),
        below_right: f(1, -1),
        above: f(0, 1),
        above_left: f(-1, 1),
        above_right: f(1, 1),
    }
}

fn fresh_digit(bottom: bool) -> Digit {
    Digit {
        n: 0,
        row: 0,
        sq4: 0,
        bottom,
        top: false,
        diag: false,
        c_n: 0,
        c_row: 0,
        c_sq4: 0,
        d: if bottom { 2 } else { 0 },
        dd: if bottom { 4 } else { 0 },
        r: 0,
        cursor: None,
        head: if bottom { Some(Head::Start) } else { None },
        prime: None,
        suffix: None,
        c_suffix: 0,
        q: None,
        q_ready: false,
        q_carry: 0,
        q_borrow: false,
        q_high: 0,
        cd: None,
        cd_ready: false,
        cd_carry: 0,
        cd_borrow: false,
        req: false,
        lz: false,
        set: false,
    }
}

/// An empty cell: becomes the first cell of a new row, or the next digit of a column being built.
fn create(p: &Params, nb: &Nb) -> Cell3 {
    let (source, diag) = match (nb.at(0, -1, 0), nb.at(-1, -1, 0)) {
        (Cell3::Empty, s) => (s, true),
        (s, _) => (s, false),
    };
    match source {
        // As in 2D: above a tile whose row may grow, or above-right of the last tile of its row.
        Cell3::Narrow(b) if b.wait == 0 => {
            let n = b.n + b.row + diag as u64;
            let row = b.row + 1;
            if p.is_narrow(row) {
                return Cell3::Narrow(new_tile_offset(n, row, b.time + 1, p.k));
            }
            return Cell3::Digit(first_wide_digit(p, n, row, diag));
        }
        // Wide rows grow at once (one row per step).
        Cell3::Digit(b) if b.bottom => {
            return Cell3::Digit(spawn(p, b, 0, (0, 0, 0), true, diag));
        }
        _ => {}
    }
    if let Some(d) = nb.digit(0, 0, -1) {
        if !d.top {
            let sx = if d.diag { -1 } else { 0 };
            let source_below_row = nb.digit(sx, -1, -1).map_or(0, |s| s.row);
            return Cell3::Digit(match nb.digit(sx, -1, 0) {
                Some(s) => spawn(
                    p,
                    s,
                    source_below_row,
                    (d.c_n, d.c_row, d.c_sq4),
                    false,
                    d.diag,
                ),
                None => extension(p, d, source_below_row),
            });
        }
    }
    Cell3::Empty
}

/// Bottom digit of a column in the first wide row, created from a narrow tile.
fn first_wide_digit(p: &Params, n: u64, row: u64, diag: bool) -> Digit {
    let mask = p.mask();
    let sq4 = p.height_value(row);
    debug_assert!((n as u128) <= mask);
    let mut d = fresh_digit(true);
    d.n = n;
    d.row = row;
    d.sq4 = (sq4 & mask) as u64;
    d.c_sq4 = (sq4 >> p.w) as u64;
    d.top = d.c_sq4 == 0;
    d.diag = diag;
    d
}

/// Digit of the column above (or above-right of) `s`: n + row (+ 1 diagonally), row + 1 and
/// 4 (row + 1)^2 = 4 row^2 + 8 row + 4, with the carries from the digit below.
fn spawn(
    p: &Params,
    s: &Digit,
    source_below_row: u64,
    carries: (u64, u64, u64),
    bottom: bool,
    diag: bool,
) -> Digit {
    let w = p.w;
    let mask = p.mask();
    let one = bottom as u128;
    let n = s.n as u128 + s.row as u128 + carries.0 as u128 + (one & diag as u128);
    let row = s.row as u128 + carries.1 as u128 + one;
    let eight_row = (((s.row as u128) << 3) | ((source_below_row as u128) >> (w - 3))) & mask;
    let sq4 = s.sq4 as u128 + eight_row + carries.2 as u128 + 4 * one;
    let mut d = fresh_digit(bottom);
    d.n = (n & mask) as u64;
    d.row = (row & mask) as u64;
    d.sq4 = (sq4 & mask) as u64;
    d.c_n = (n >> w) as u64;
    d.c_row = (row >> w) as u64;
    d.c_sq4 = (sq4 >> w) as u64;
    d.top = s.top && d.c_n == 0 && d.c_row == 0 && d.c_sq4 == 0 && (s.row >> (w - 3)) == 0;
    d.diag = diag;
    d
}

/// An extra top digit of a column that is taller than its source (one digit, except possibly in
/// the first wide row).
fn extension(p: &Params, d: &Digit, source_top_row: u64) -> Digit {
    let sq4 = d.c_sq4 as u128 + ((source_top_row as u128) >> (p.w - 3));
    debug_assert!(d.c_n == 0 && d.c_row == 0);
    let mut e = fresh_digit(false);
    e.n = d.c_n;
    e.row = d.c_row;
    e.sq4 = (sq4 & p.mask()) as u64;
    e.c_sq4 = (sq4 >> p.w) as u64;
    e.top = e.c_sq4 == 0;
    e.diag = d.diag;
    e
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Move {
    Up,
    Down,
    Stay,
}

struct HeadOut {
    r: u64,
    d: u64,
    dd: u64,
    cursor: Option<u32>,
    prime: Option<bool>,
    next: Option<(Move, Head)>,
}

fn combine(digit: Ordering, lower: Ordering) -> Ordering {
    if digit == Ordering::Equal {
        lower
    } else {
        digit
    }
}

/// a - b - borrow on one digit: (result, borrow out).
fn sub_digit(a: u64, b: u64, borrow: bool, mask: u128) -> (u128, bool) {
    let sub = b as u128 + borrow as u128;
    if a as u128 >= sub {
        (a as u128 - sub, false)
    } else {
        (a as u128 + mask + 1 - sub, true)
    }
}

/// The head in cell `c`: what it writes there and where it goes. The cells above and below evaluate
/// the same function to see the head arrive.
fn head_step(p: &Params, c: &Digit, head: Head) -> HeadOut {
    let w = p.w;
    let mask = p.mask();
    let mut out = HeadOut {
        r: c.r,
        d: c.d,
        dd: c.dd,
        cursor: c.cursor,
        prime: c.prime,
        next: None,
    };
    let divide = Head::Down {
        s: false,
        b: None,
        last: false,
        arrives: false,
    };
    out.next = match head {
        Head::Start => Some((
            Move::Stay,
            Head::Cmp {
                ord: Ordering::Equal,
            },
        )),
        Head::Cmp { ord } => {
            out.r = 0;
            let ord = combine(c.dd.cmp(&c.n), ord);
            if !c.top {
                Some((Move::Up, Head::Cmp { ord }))
            } else if ord == Ordering::Greater {
                Some((Move::Stay, Head::Report { prime: true }))
            } else {
                out.cursor = Some(w - 1);
                Some((Move::Stay, divide))
            }
        }
        Head::Down { s, b, last, .. } => {
            let (mut b, mut last, mut arrives) = (b, last, false);
            if b.is_none() {
                if let Some(j) = c.cursor {
                    b = Some((c.n >> j) & 1 == 1);
                    if j > 0 {
                        out.cursor = Some(j - 1);
                    } else {
                        out.cursor = None;
                        if c.bottom {
                            last = true;
                        } else {
                            arrives = true;
                        }
                    }
                }
            }
            if !c.bottom {
                Some((
                    Move::Down,
                    Head::Down {
                        s,
                        b,
                        last,
                        arrives,
                    },
                ))
            } else {
                Some((
                    Move::Stay,
                    Head::Up {
                        s,
                        b: b.expect("every round takes one bit"),
                        borrow: false,
                        shift: false,
                        ord: Ordering::Equal,
                        last,
                    },
                ))
            }
        }
        Head::Up {
            s,
            b,
            borrow,
            shift,
            ord,
            last,
        } => {
            let (t, borrow_out) = sub_digit(c.r, if s { c.d } else { 0 }, borrow, mask);
            let shift_in = if c.bottom { b } else { shift };
            let r = ((t << 1) | shift_in as u128) & mask;
            let shift_out = (t >> (w - 1)) & 1 == 1;
            out.r = r as u64;
            let ord = combine(out.r.cmp(&c.d), ord);
            if !c.top {
                Some((
                    Move::Up,
                    Head::Up {
                        s,
                        b,
                        borrow: borrow_out,
                        shift: shift_out,
                        ord,
                        last,
                    },
                ))
            } else {
                debug_assert!(!borrow_out && !shift_out, "remainder overflow");
                let s = ord != Ordering::Less;
                if last {
                    Some((Move::Stay, Head::FinalDown { s }))
                } else {
                    Some((
                        Move::Stay,
                        Head::Down {
                            s,
                            b: None,
                            last: false,
                            arrives: false,
                        },
                    ))
                }
            }
        }
        Head::FinalDown { s } => {
            if !c.bottom {
                Some((Move::Down, Head::FinalDown { s }))
            } else {
                Some((
                    Move::Stay,
                    Head::FinalUp {
                        s,
                        borrow: false,
                        zero: true,
                    },
                ))
            }
        }
        Head::FinalUp { s, borrow, zero } => {
            let (t, borrow_out) = sub_digit(c.r, if s { c.d } else { 0 }, borrow, mask);
            out.r = t as u64;
            let zero = zero && t == 0;
            if !c.top {
                Some((
                    Move::Up,
                    Head::FinalUp {
                        s,
                        borrow: borrow_out,
                        zero,
                    },
                ))
            } else if zero {
                Some((Move::Stay, Head::Report { prime: false }))
            } else {
                Some((Move::Stay, Head::IncDown))
            }
        }
        Head::IncDown => {
            if !c.bottom {
                Some((Move::Down, Head::IncDown))
            } else {
                Some((
                    Move::Stay,
                    Head::IncUp {
                        carry_d: true,
                        carry_dd: 1,
                        top_bit: false,
                        ord: Ordering::Equal,
                    },
                ))
            }
        }
        Head::IncUp {
            carry_d,
            carry_dd,
            top_bit,
            ord,
        } => {
            let twice_d = (((c.d as u128) << 1) | top_bit as u128) & mask;
            let dd = c.dd as u128 + twice_d + carry_dd as u128;
            let d = c.d as u128 + carry_d as u128;
            out.dd = (dd & mask) as u64;
            out.d = (d & mask) as u64;
            out.r = 0;
            let ord = combine(out.dd.cmp(&c.n), ord);
            if !c.top {
                Some((
                    Move::Up,
                    Head::IncUp {
                        carry_d: d >> w == 1,
                        carry_dd: (dd >> w) as u64,
                        top_bit: (c.d >> (w - 1)) & 1 == 1,
                        ord,
                    },
                ))
            } else if ord == Ordering::Greater {
                Some((Move::Stay, Head::Report { prime: true }))
            } else {
                out.cursor = Some(w - 1);
                Some((Move::Stay, divide))
            }
        }
        Head::Report { prime } => {
            if c.bottom {
                out.prime = Some(prime);
                None
            } else {
                Some((Move::Down, Head::Report { prime }))
            }
        }
    };
    out
}

/// Decrement a digit by `by` (at most 2): (result, borrow out).
fn decrement(v: u64, by: u64, mask: u128) -> (u64, bool) {
    if v >= by {
        (v - by, false)
    } else {
        ((v as u128 + mask + 1 - by as u128) as u64, true)
    }
}

/// Q in the tile below (the chain source of the first column of a row), at this digit's level: a
/// digit of a wide column (missing digits are 0), or the whole of a narrow tile's
/// pi(row end) - clock, which the bottom digit splits into digits.
fn chain_source(p: &Params, nb: &Nb) -> Option<u128> {
    match nb.at(0, -1, 0) {
        Cell3::Digit(b) => b.q.map(|v| v as u128),
        Cell3::Narrow(b) => b.p_end.map(|p_end| {
            let q = p_end as i128 + p.k as i128 - b.time as i128;
            debug_assert!(q >= 0, "chain reached a wide row after its deadline");
            q as u128
        }),
        Cell3::Empty => Some(0),
    }
}

fn step_digit(p: &Params, c: &Digit, nb: &Nb) -> Digit {
    let w = p.w;
    let mask = p.mask();
    let mut t = *c;
    let down = if c.bottom { None } else { nb.digit(0, 0, -1) };
    let up = if c.top { None } else { nb.digit(0, 0, 1) };

    // Primality head: act here, or arrive from the cell above or below.
    if let Some(h) = c.head {
        let out = head_step(p, c, h);
        t.r = out.r;
        t.d = out.d;
        t.dd = out.dd;
        t.cursor = out.cursor;
        t.prime = out.prime;
        t.head = match out.next {
            Some((Move::Stay, h)) => Some(h),
            _ => None,
        };
    }
    if let Some(a) = up {
        if let Some(h) = a.head {
            if let Some((Move::Down, h)) = head_step(p, a, h).next {
                t.head = Some(match h {
                    Head::Down {
                        s,
                        b,
                        last,
                        arrives: true,
                    } => {
                        t.cursor = Some(w - 1);
                        Head::Down {
                            s,
                            b,
                            last,
                            arrives: false,
                        }
                    }
                    h => h,
                });
            }
        }
    }
    if let Some(b) = down {
        if let Some(h) = b.head {
            if let Some((Move::Up, h)) = head_step(p, b, h).next {
                t.head = Some(h);
            }
        }
    }

    // Suffix pass: primes from here to the end of the row (right to left, carries upwards).
    if c.suffix.is_none() {
        let addend = if c.bottom {
            c.prime.map(|prime| prime as u128)
        } else {
            down.and_then(|b| b.suffix.map(|_| b.c_suffix as u128))
        };
        let right = match nb.at(1, 0, 0) {
            Cell3::Digit(r) => r.suffix.map(|v| v as u128),
            _ => Some(0),
        };
        if let (Some(a), Some(r)) = (addend, right) {
            let v = a + r;
            t.suffix = Some((v & mask) as u64);
            t.c_suffix = (v >> w) as u64;
        }
    }

    // Q: copied from the left (broadcast) or, in the first column, from the column below plus this
    // row's prime count (chain). Digit k starts one step after digit k - 1.
    let first = nb.digit(-1, 0, 0).is_none();
    match c.q {
        None => {
            let start = if c.bottom {
                if first {
                    c.q_ready
                } else {
                    nb.digit(-1, 0, 0).is_some_and(|l| l.q.is_some())
                }
            } else {
                down.is_some_and(|b| b.q.is_some())
            };
            if start {
                let (source, addend) = if first {
                    let whole = chain_source(p, nb).expect("chain source ready")
                        + down.map_or(0, |b| b.q_high as u128);
                    t.q_high = (whole >> w) as u64;
                    (whole & mask, c.suffix.expect("suffix ready") as u128)
                } else {
                    let l = nb.digit(-1, 0, 0).unwrap();
                    (l.q.expect("broadcast keeps the skew") as u128, 0)
                };
                // The bottom digit also takes this step's tick; higher digits take the borrow.
                let (carry_in, borrow_in) = match down {
                    None => (0, 1),
                    Some(b) => (b.q_carry as u128, b.q_borrow as u64),
                };
                let v = source + addend + carry_in;
                let (value, borrow) = decrement((v & mask) as u64, borrow_in, mask);
                t.q = Some(value);
                t.q_carry = (v >> w) as u64;
                t.q_borrow = borrow;
            } else if first {
                t.q_ready = c.suffix.is_some()
                    && chain_source(p, nb).is_some()
                    && (c.top || up.is_some_and(|a| a.q_ready));
            }
        }
        Some(q) => {
            let dec = match down {
                None => 1,
                Some(b) => b.q_borrow as u64,
            };
            let (value, borrow) = decrement(q, dec, mask);
            t.q = Some(value);
            t.q_borrow = borrow;
        }
    }

    // Countdown CD = Q + 1 - suffix (= Q + !suffix + 2), set up digit by digit like Q, then
    // decremented at every step. The top digit lags the bottom by h - 1 steps, so when a digit above
    // the bottom is set up it sends one extra decrement down (`req`); once all have arrived, the top
    // sees exactly the current countdown, and `lz` tells it whether every digit below is 0.
    let extra = up.is_some_and(|a| a.req);
    match c.cd {
        None => {
            let start = match down {
                None => c.cd_ready,
                Some(b) => b.cd.is_some(),
            };
            if start {
                let (carry_in, borrow_in) = match down {
                    None => (2, 1),
                    Some(b) => (b.cd_carry as u128, b.cd_borrow as u64),
                };
                let not_suffix = (!c.suffix.expect("suffix ready") as u128) & mask;
                let v = c.q.expect("q ready") as u128 + not_suffix + carry_in;
                let (value, borrow) = decrement((v & mask) as u64, borrow_in, mask);
                t.cd = Some(value);
                t.cd_carry = (v >> w) as u64;
                t.cd_borrow = borrow;
                debug_assert!(!extra);
                t.req = !c.bottom;
            } else {
                t.cd_ready = c.q.is_some()
                    && c.suffix.is_some()
                    && (c.top || up.is_some_and(|a| a.cd_ready));
            }
        }
        Some(cd) => {
            let dec = match down {
                None => 1 + extra as u64,
                Some(b) => b.cd_borrow as u64,
            };
            let (value, borrow) = decrement(cd, dec, mask);
            t.cd = Some(value);
            t.cd_borrow = borrow;
            t.req = !c.bottom && extra;
        }
    }
    t.lz = c.bottom || down.is_some_and(|b| b.cd == Some(0) && b.lz);
    if c.top && t.cd == Some(0) && t.lz {
        t.set = true;
    }
    t
}

/// Upper bounds on the timing of the wide machinery, read off the rule (tested against simulation in
/// `latency_bounds_hold_in_simulation`). Times are steps; `h` is the column height of row `y`.
pub mod timing {
    fn isqrt(n: u128) -> u128 {
        let mut r = (n as f64).sqrt() as u128;
        while r * r > n {
            r -= 1;
        }
        while (r + 1) * (r + 1) <= n {
            r += 1;
        }
        r
    }

    /// Every column of a row created at step `created`, holding numbers up to `n_max`, has its
    /// primality verdict at the bottom by this step: the head waits 1 step and compares d^2 with n
    /// (h steps); for each d = 2..=floor(sqrt n) it runs W h division rounds, a final round and an
    /// increment round (2h steps each); then it reports down the column (h steps).
    pub fn verdicts(w: u32, h: u64, created: u128, n_max: u128) -> u128 {
        let h = h as u128;
        created + 2 * h + 1 + isqrt(n_max).saturating_sub(1) * 2 * h * (w as u128 * h + 2)
    }

    /// The first column of row y starts Q at its bottom digit by this step, if every verdict of the
    /// row is known by `verdicts` and the source below is complete by `source`: the suffix pass takes
    /// up to y + h - 1 more steps, the ready flags h, the start 1.
    pub fn chain(h: u64, y: u64, verdicts: u128, source: u128) -> u128 {
        (verdicts + y as u128 + h as u128 - 1).max(source) + h as u128 + 1
    }

    /// Every column of row y has set up the bottom digit of its countdown by this step (the
    /// broadcast moves one column per step; the countdown waits for Q and the suffix and for its
    /// ready flags). Its top digit reads the countdown exactly 3h - 3 steps later, once the extra
    /// decrements have reached the bottom and their borrows have climbed back.
    pub fn armed(h: u64, y: u64, chain: u128, verdicts: u128) -> u128 {
        let (h, y) = (h as u128, y as u128);
        (chain + y + h - 2).max(verdicts + y + h - 1) + h + 1
    }
}

/// Lower bound for pi(x) (Dusart 2010: pi(x) >= x / (ln x - 1) for x >= 5393), rounded down.
fn pi_lower_f64(x: f64) -> f64 {
    debug_assert!(x >= 5393.0);
    x / (x.ln() - 1.0) * (1.0 - 1e-12) - 1.0
}

#[derive(Clone, Copy, Debug)]
pub struct WideCertificate {
    /// First row stored as columns.
    pub first_wide_row: u64,
    /// The step at which it appears.
    pub created: u128,
    /// Largest ratio (latency bound) / (deadline) over the wide rows checked.
    pub worst_ratio: f64,
    pub worst_row: f64,
    /// Rows checked numerically, up to this one; the rest follows from the tail estimate.
    pub checked_to: f64,
}

/// Timing certificate for the columns of the automaton with W-bit digits and no head start (W = 64 is
/// the real automaton). Every column of every wide row must have its countdown exact before its
/// deadline tau(n) = pi(n - 1) + 1 >= pi(S_y - 1) + 1.
///
/// With C the step at which the first wide row Y0 appears, wide rows appear one per step, so row y
/// appears at C(y) = C + y - Y0. With h(y) <= ceil((2 log2 y + 3) / W) digits and the bounds of
/// `timing`, every column of row y is exact by
///     A(y) = V(y) + 2y + 7h + E + 1,  V(y) = C(y) + 2h + 1 + sqrt(y (y + 1) / 2) 2h (W h + 2),
/// where E >= 0 absorbs the narrow chain at row Y0 - 1. (By induction the chain start of row y is at
/// most V(y) + y + 2h + E: V grows by more than 3h per row.) Both A and the deadline increase with y,
/// so it suffices that A(b) <= pi_lower(S_a - 1) + 1 on a fine geometric grid a < b, which is checked
/// up to y = 1e150. Beyond, with L = log2 y >= W + 3: h <= 3L / W, so 2h (W h + 2) <= 24 L^2 / W and
/// A(y) <= (C + E) + y (3 + 17.1 L^2 / W) + 30 L^2 <= 6 y L^2 once y >= C + E and y >= 30 L^2; the
/// deadline is at least (y^2 / 2.01) / (2 ln y) >= y^2 / (2.79 L). So A(y) <= deadline whenever
/// y >= 16.74 L^3, which holds for y = 1e150 and, as 2^L / L^3 increases, for every larger y.
pub fn wide_certificate(w: u32) -> Result<WideCertificate, String> {
    use crate::prime_display::{narrow_schedule, pace, timing_certificate, BOOTSTRAP_ROWS};
    let params = Params { w, k: 0 };
    let mut y0 = 1u64;
    while params.is_narrow(y0) {
        y0 += 1;
    }
    if y0 <= BOOTSTRAP_ROWS + 1 {
        return Err("the first wide row must come after the table rows".into());
    }
    // Narrow rows: the 2D certificate.
    timing_certificate(y0 - 1).map_err(|row| format!("narrow row {} is late", row))?;
    let (created_before, narrow_chain) = narrow_schedule(y0 - 1);
    let created = created_before + pace(y0 - 1) as u128 + 1;
    let wf = w as f64;
    let h_of = |y: f64| ((2.0 * y.log2() + 3.0 + 1e-9) / wf).ceil();
    let verdicts = |y: f64| {
        let h = h_of(y);
        created as f64
            + (y - y0 as f64)
            + 2.0 * h
            + 1.0
            + (y * (y + 1.0) / 2.0).sqrt() * 2.0 * h * (wf * h + 2.0)
    };
    let y0f = y0 as f64;
    let slack = narrow_chain as f64 - (verdicts(y0f) + y0f + h_of(y0f) - 1.0);
    let extra = slack.max(0.0);
    let bound =
        |y: f64| (verdicts(y) + 2.0 * y + 7.0 * h_of(y) + extra + 1.0) * (1.0 + 1e-12) + 1.0;
    let deadline = |y: f64| pi_lower_f64(y * (y - 1.0) / 2.0) + 1.0;
    let checked_to = 1e150;
    let ratio = 1.0 + 1.0 / 4096.0;
    let mut a = y0f;
    let mut worst = (0.0f64, y0f);
    while a < checked_to {
        let b = (a * ratio).floor().max(a + 1.0);
        let r = bound(b) / deadline(a);
        if r > worst.0 {
            worst = (r, a);
        }
        if r >= 1.0 {
            return Err(format!("the bound fails near row {:.6e}", a));
        }
        a = b;
    }
    // Tail (see above), at y = 1e150 with L = log2 y.
    let l = checked_to.log2();
    let tail = l >= wf + 3.0
        && checked_to >= created as f64 + extra
        && checked_to >= 30.0 * l * l
        && checked_to >= 16.74 * l.powi(3);
    if !tail {
        return Err("tail estimate does not apply".into());
    }
    Ok(WideCertificate {
        first_wide_row: y0,
        created,
        worst_ratio: worst.0,
        worst_row: worst.1,
        checked_to,
    })
}

/// Direct simulation: the rule applied to every cell of the triangle's columns (and the empty cell
/// above each) at every step, rows 1..=y_max. Rows never read the rows above them, so the cells of
/// rows <= y_max evolve exactly as in the unbounded automaton.
pub struct ColumnSim {
    pub params: Params,
    /// rows[y - 1][x][z]
    rows: Vec<Vec<Vec<Cell3>>>,
    pub time: u64,
    pub y_max: u64,
    /// Columns (x, y) whose countdown started at the bottom during the last step.
    pub armed: Vec<(i64, i64)>,
}

impl ColumnSim {
    pub fn new(params: Params, y_max: u64) -> Self {
        assert!((3..=64).contains(&params.w) && y_max >= 1);
        let seed = Cell3::Narrow(new_tile_offset(1, 1, 0, params.k));
        ColumnSim {
            params,
            rows: vec![vec![vec![seed]]],
            time: 0,
            y_max,
            armed: vec![],
        }
    }

    pub fn get(&self, x: i64, y: i64, z: i64) -> &Cell3 {
        if x < 0 || y < 1 || z < 0 {
            return &EMPTY;
        }
        self.rows
            .get((y - 1) as usize)
            .and_then(|row| row.get(x as usize))
            .and_then(|column| column.get(z as usize))
            .unwrap_or(&EMPTY)
    }

    pub fn step(&mut self) {
        let height = (self.rows.len() as u64 + 1).min(self.y_max) as i64;
        let mut rows = Vec::with_capacity(height as usize);
        self.armed.clear();
        for y in 1..=height {
            let mut row = Vec::with_capacity(y as usize);
            for x in 0..y {
                let len = self
                    .rows
                    .get((y - 1) as usize)
                    .and_then(|row| row.get(x as usize))
                    .map_or(0, |column| column.len()) as i64;
                let mut column = Vec::with_capacity(len as usize + 1);
                for z in 0..=len {
                    let nb = Nb::from_fn(|dx, dy, dz| self.get(x + dx, y + dy, z + dz));
                    let cell = next(&self.params, &nb);
                    if let (Cell3::Digit(before), Cell3::Digit(after)) = (nb.at(0, 0, 0), &cell) {
                        if before.bottom && before.cd.is_none() && after.cd.is_some() {
                            self.armed.push((x, y));
                        }
                    }
                    column.push(cell);
                }
                while matches!(column.last(), Some(Cell3::Empty)) {
                    column.pop();
                }
                row.push(column);
            }
            rows.push(row);
        }
        while rows
            .last()
            .is_some_and(|row: &Vec<Vec<Cell3>>| row.iter().all(|c| c.is_empty()))
        {
            rows.pop();
        }
        self.rows = rows;
        self.time += 1;
    }

    pub fn set_count(&self) -> u64 {
        self.rows
            .iter()
            .flatten()
            .filter(|column| column.iter().any(|c| c.is_set()))
            .count() as u64
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }

    /// Tallest column.
    pub fn depth(&self) -> usize {
        self.rows
            .iter()
            .flatten()
            .map(|column| column.len())
            .max()
            .unwrap_or(0)
    }

    /// The number a wide column stands for, and its height.
    pub fn column_number(&self, x: i64, y: i64) -> Option<(u128, usize)> {
        let column = self.rows.get((y - 1) as usize)?.get(x as usize)?;
        let mut n = 0u128;
        for (k, cell) in column.iter().enumerate() {
            match cell {
                Cell3::Digit(d) => n |= (d.n as u128) << (k as u32 * self.params.w),
                _ => return None,
            }
        }
        Some((n, column.len()))
    }

    /// Whether the column (x, y) shows its set flag.
    pub fn column_set(&self, x: i64, y: i64) -> bool {
        self.rows
            .get((y - 1) as usize)
            .and_then(|row| row.get(x as usize))
            .is_some_and(|column| column.iter().any(|c| c.is_set()))
    }

    /// Progress of a column's machinery, for diagnostics.
    pub fn column_status(&self, x: i64, y: i64) -> String {
        match self.get(x, y, 0) {
            Cell3::Digit(d) => format!(
                "prime {:?}, suffix {}, Q {}, countdown {}",
                d.prime,
                d.suffix.is_some(),
                d.q.is_some(),
                d.cd.is_some()
            ),
            Cell3::Narrow(t) => format!("narrow tile, tau {:?}", t.tau),
            Cell3::Empty => "not created".to_string(),
        }
    }

    pub fn cells(&self) -> impl Iterator<Item = (i64, i64, i64, &Cell3)> {
        self.rows.iter().enumerate().flat_map(|(y, row)| {
            row.iter().enumerate().flat_map(move |(x, column)| {
                column
                    .iter()
                    .enumerate()
                    .map(move |(z, c)| (x as i64, y as i64 + 1, z as i64, c))
            })
        })
    }
}

/// Sparse simulation of any finite configuration: every cell that is not empty, and every empty
/// cell next to one, is evaluated at every step (rows above `y_max` are not simulated).
pub struct SparseSim {
    pub params: Params,
    pub cells: std::collections::HashMap<(i64, i64, i64), Cell3>,
    pub time: u64,
    pub y_max: i64,
}

impl SparseSim {
    pub fn new(params: Params, y_max: i64) -> Self {
        let mut cells = std::collections::HashMap::new();
        cells.insert((0, 1, 0), Cell3::Narrow(new_tile_offset(1, 1, 0, params.k)));
        SparseSim {
            params,
            cells,
            time: 0,
            y_max,
        }
    }

    fn get(&self, x: i64, y: i64, z: i64) -> &Cell3 {
        self.cells.get(&(x, y, z)).unwrap_or(&EMPTY)
    }

    pub fn step(&mut self) {
        let mut todo = std::collections::HashSet::new();
        for &(x, y, z) in self.cells.keys() {
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (a, b, c) = (x + dx, y + dy, z + dz);
                        if a >= 0 && b >= 1 && c >= 0 && b <= self.y_max {
                            todo.insert((a, b, c));
                        }
                    }
                }
            }
        }
        let mut cells = std::collections::HashMap::with_capacity(self.cells.len() + 64);
        for (x, y, z) in todo {
            let nb = Nb::from_fn(|dx, dy, dz| self.get(x + dx, y + dy, z + dz));
            let cell = next(&self.params, &nb);
            if cell != Cell3::Empty {
                cells.insert((x, y, z), cell);
            }
        }
        self.cells = cells;
        self.time += 1;
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

    /// pi(m) for m <= limit.
    fn pi_table(limit: usize) -> Vec<u64> {
        let primes = primes(limit);
        let mut pi = vec![0u64; limit + 1];
        for m in 2..=limit {
            pi[m] = pi[m - 1] + primes.binary_search(&(m as u64)).is_ok() as u64;
        }
        pi
    }

    /// Set cells expected after `step` steps with head start k: p_(step - k), p_0 = 1, none before k.
    fn expected(primes: &[u64], k: u64, step: u64) -> u64 {
        match step.checked_sub(k) {
            None => 0,
            Some(0) => 1,
            Some(t) => primes[t as usize - 1],
        }
    }

    /// Runs the head on a lone column holding n and returns its verdict.
    fn head_verdict(w: u32, n: u64, h: usize) -> bool {
        let p = Params { w, k: 0 };
        let mask = p.mask() as u64;
        let mut column: Vec<Cell3> = (0..h)
            .map(|k| {
                let mut d = fresh_digit(k == 0);
                d.n = if k * (w as usize) < 64 {
                    (n >> (k as u32 * w)) & mask
                } else {
                    0
                };
                d.top = k == h - 1;
                Cell3::Digit(d)
            })
            .collect();
        for _ in 0..5_000_000u64 {
            let next_column: Vec<Cell3> = (0..h as i64)
                .map(|z| {
                    let nb = Nb::from_fn(|dx, dy, dz| {
                        if dx != 0 || dy != 0 || z + dz < 0 || z + dz >= h as i64 {
                            &EMPTY
                        } else {
                            &column[(z + dz) as usize]
                        }
                    });
                    match nb.at(0, 0, 0) {
                        Cell3::Digit(d) => Cell3::Digit(step_digit(&p, d, &nb)),
                        _ => unreachable!(),
                    }
                })
                .collect();
            column = next_column;
            if let Cell3::Digit(d) = &column[0] {
                if let Some(prime) = d.prime {
                    return prime;
                }
            }
        }
        panic!("no verdict for n = {}", n);
    }

    #[test]
    fn the_head_tests_primality_by_long_division() {
        let primes = primes(3000);
        for w in [3u32, 4, 5, 8, 64] {
            for n in 2..=1500u64 {
                // Enough digits for 4 n, as in a real column (at least two).
                let bits = 128 - (4 * n as u128).leading_zeros();
                let h = bits.div_ceil(w) as usize;
                let prime = head_verdict(w, n, h.max(2));
                assert_eq!(
                    prime,
                    primes.binary_search(&n).is_ok(),
                    "w = {} n = {}",
                    w,
                    n
                );
            }
        }
    }

    /// Reads the values of the column at (x, y): (n, row, 4 row^2 below 2^128, height).
    fn column_values(sim: &SparseSim, x: i64, y: i64) -> (u128, u128, u128, usize) {
        let w = sim.params.w;
        let (mut n, mut row, mut sq4, mut h) = (0u128, 0u128, 0u128, 0usize);
        while let Some(Cell3::Digit(d)) = sim.cells.get(&(x, y, h as i64)) {
            let shift = w * h as u32;
            if shift < 128 {
                n |= (d.n as u128) << shift;
                row |= (d.row as u128) << shift;
                sq4 |= (d.sq4 as u128) << shift;
            }
            if d.top {
                return (n, row, sq4, h + 1);
            }
            h += 1;
        }
        panic!("column ({}, {}) is incomplete", x, y);
    }

    /// Row creation with 64-bit digits where columns gain digits: from the last narrow row
    /// (2^31 - 1) to the first wide row, and from row 2^63 - 1 to row 2^63, whose 4 y^2 = 2^128
    /// needs a third digit. The stored row numbers are huge; the rule never reads coordinates, so
    /// the columns can stand at small ones. Both the column above and the new last column (above
    /// right) are checked.
    #[test]
    fn rows_are_built_digit_by_digit_with_64_bit_digits() {
        let p = Params::REAL;
        let start = |y: u128| y * (y - 1) / 2 + 1;
        let empty = |y_max| SparseSim {
            params: p,
            cells: std::collections::HashMap::new(),
            time: 0,
            y_max,
        };
        // Narrow to wide.
        let y = (1u64 << 31) - 1;
        let last = start(y as u128) + y as u128 - 1;
        let mut tile = new_tile_offset(last as u64, y, 0, 0);
        tile.wait = 0;
        let mut sim = empty(6);
        sim.cells.insert((0, 5, 0), Cell3::Narrow(tile));
        for _ in 0..4 {
            sim.step();
        }
        let y = y as u128;
        let sq4 = 4 * (y + 1) * (y + 1);
        assert_eq!(column_values(&sim, 0, 6), (last + y, y + 1, sq4, 2));
        assert_eq!(column_values(&sim, 1, 6), (last + y + 1, y + 1, sq4, 2));
        // Wide to wide, gaining a third digit.
        let y: u128 = (1 << 63) - 1;
        let last = start(y) + y - 1;
        let sq4 = 4 * y * y;
        let mut sim = empty(6);
        for k in 0..2u32 {
            let mut d = fresh_digit(k == 0);
            d.n = (last >> (64 * k)) as u64;
            d.row = (y >> (64 * k)) as u64;
            d.sq4 = (sq4 >> (64 * k)) as u64;
            d.top = k == 1;
            sim.cells.insert((0, 5, k as i64), Cell3::Digit(d));
        }
        for _ in 0..5 {
            sim.step();
        }
        for (x, n) in [(0, last + y), (1, last + y + 1)] {
            // 4 (y + 1)^2 = 2^128: digits 0, 0, 1.
            assert_eq!(column_values(&sim, x, 6), (n, y + 1, 0, 3));
            assert!(matches!(
                sim.cells.get(&(x, 6, 2)),
                Some(Cell3::Digit(d)) if d.sq4 == 1 && d.n == 0 && d.row == 0
            ));
        }
    }

    /// The chain from the last narrow row, the suffix pass, the broadcast and the countdowns with
    /// 64-bit digits and the real clock: four decided columns standing for numbers of row 2^31
    /// (a short stand-in row; the rule reads only the stored values). With P = pi(end of the row
    /// below) and c primes in the row, column x must turn set exactly at P + c - suffix(x) + 1.
    #[test]
    fn countdowns_with_64_bit_digits_flip_exactly_on_time() {
        let p = Params::REAL;
        let y = 1u64 << 31;
        let first = (y as u128) * (y as u128 - 1) / 2 + 1;
        let big_p = 50_000_000_000_000_000u64; // about pi of the numbers involved
        let t0 = big_p - 300;
        let mut sim = SparseSim {
            params: p,
            cells: std::collections::HashMap::new(),
            time: t0,
            y_max: 6,
        };
        let mut source = new_tile_offset((first - y as u128 + 1) as u64, y - 1, t0, 0);
        source.p_end = Some(big_p);
        source.prime = Some(false);
        source.suffix = Some(0);
        source.tau = Some(u64::MAX / 2);
        sim.cells.insert((0, 5, 0), Cell3::Narrow(source));
        let primes = [true, false, false, true];
        for (x, &prime) in primes.iter().enumerate() {
            let n = first + x as u128;
            for (z, d) in decided_column(&p, n as u64, y, prime)
                .into_iter()
                .enumerate()
            {
                sim.cells.insert((x as i64, 6, z as i64), Cell3::Digit(d));
            }
        }
        let c = primes.iter().filter(|&&q| q).count() as u64;
        let tau: Vec<u64> = (0..4)
            .map(|x| big_p + c - primes[x..].iter().filter(|&&q| q).count() as u64 + 1)
            .collect();
        while sim.time < tau[3] + 3 {
            sim.step();
            for (x, &due) in tau.iter().enumerate() {
                let set = sim.cells.get(&(x as i64, 6, 1)).is_some_and(|c| c.is_set());
                assert_eq!(set, due <= sim.time, "column {} at step {}", x, sim.time);
            }
        }
    }

    #[test]
    fn narrow_plane_is_the_2d_rule() {
        let mut columns = ColumnSim::new(Params::REAL, 1_000);
        let mut plane = crate::prime_display::Simulation::new();
        for step in 1..=400 {
            columns.step();
            plane.step();
            assert_eq!(columns.set_count(), plane.set_count(), "step {}", step);
            assert_eq!(columns.depth(), 1);
        }
        for (x, y, z, cell) in columns.cells() {
            assert_eq!(z, 0);
            let expected = match plane.cells().find(|&(px, py, _)| px == x && py == y) {
                Some((_, _, PlaneCell::Tile(t))) => Cell3::Narrow(*t),
                _ => Cell3::Empty,
            };
            assert_eq!(*cell, expected, "cell ({}, {})", x, y);
        }
    }

    /// From the seed: the count after every step, until the next prime lies beyond row y_max.
    fn check_counts(params: Params, y_max: u64) {
        let last = (y_max * (y_max + 1) / 2) as usize;
        let primes = primes(last);
        let mut sim = ColumnSim::new(params, y_max);
        assert_eq!(sim.set_count(), expected(&primes, params.k, 0));
        for step in 1..=params.k + primes.len() as u64 {
            sim.step();
            assert_eq!(
                sim.set_count(),
                expected(&primes, params.k, step),
                "w = {} k = {}: step {}",
                params.w,
                params.k,
                step
            );
        }
        assert!(sim.depth() >= 3, "taller columns were not exercised");
    }

    #[test]
    fn wide_columns_count_the_primes() {
        check_counts(Params { w: 4, k: 1500 }, 13);
        check_counts(Params { w: 3, k: 1500 }, 10);
    }

    #[test]
    fn sparse_and_dense_simulations_agree() {
        let p = Params { w: 4, k: 400 };
        let mut dense = ColumnSim::new(p, 9);
        let mut sparse = SparseSim::new(p, 9);
        for step in 1..=440 {
            dense.step();
            sparse.step();
            if step < 40 || step % 20 == 0 {
                let cells: std::collections::HashMap<_, _> = dense
                    .cells()
                    .filter(|(_, _, _, c)| **c != Cell3::Empty)
                    .map(|(x, y, z, c)| ((x, y, z), *c))
                    .collect();
                assert!(cells == sparse.cells, "step {}", step);
            }
        }
    }

    /// The bounds of `timing`, applied row by row with the simulated creation steps, hold for every
    /// column: verdicts, chain starts and countdown starts.
    #[test]
    fn latency_bounds_hold_in_simulation() {
        let p = Params { w: 4, k: 5000 };
        let y_max = 22u64;
        let mut sim = ColumnSim::new(p, y_max);
        let mut created = std::collections::HashMap::new();
        let mut verdict = std::collections::HashMap::new();
        let mut chain_start = std::collections::HashMap::new();
        let mut armed = std::collections::HashMap::new();
        for _ in 0..p.k + 60 {
            sim.step();
            for (x, y, z, cell) in sim.cells() {
                if let (Cell3::Digit(d), 0) = (cell, z) {
                    created.entry(y).or_insert(sim.time);
                    if d.prime.is_some() {
                        verdict.entry((x, y)).or_insert(sim.time);
                    }
                    if x == 0 && d.q.is_some() {
                        chain_start.entry(y).or_insert(sim.time);
                    }
                }
            }
            for &(x, y) in &sim.armed {
                armed.insert((x, y), sim.time);
            }
        }
        // Wide rows appear one per step (the certificate's C(y) = C + y - Y0).
        for y in 2..y_max as i64 {
            assert_eq!(created[&(y + 1)], created[&y] + 1, "row {}", y + 1);
        }
        // Row 1 is the seed (narrow, from the table): its p_end is known from step 0.
        let mut source = 0u128;
        let mut checked = 0;
        for y in 2..=y_max as i64 {
            let h = sim.column_number(0, y).unwrap().1 as u64;
            let last = (y * (y + 1) / 2) as u128;
            let v = timing::verdicts(p.w, h, created[&y] as u128, last);
            let c = timing::chain(h, y as u64, v, source);
            let a = timing::armed(h, y as u64, c, v);
            assert!(chain_start[&y] as u128 <= c, "row {} chain", y);
            for x in 0..y {
                assert!(
                    verdict[&(x, y)] as u128 <= v,
                    "column ({}, {}) verdict",
                    x,
                    y
                );
                assert!(armed[&(x, y)] as u128 <= a, "column ({}, {}) armed", x, y);
                checked += 1;
            }
            source = c + h as u128 - 1;
        }
        assert_eq!(checked, (2..=y_max).sum::<u64>());
    }

    #[test]
    fn wide_rows_are_on_time_for_32_bit_digits() {
        // The same certificate as for the real automaton (W = 64), with a first wide row (2^15)
        // small enough for a quick test.
        let certificate = wide_certificate(32).unwrap();
        assert_eq!(certificate.first_wide_row, 1 << 15);
        assert!(certificate.worst_ratio < 1.0);
    }

    /// A column whose primality is already decided, as the head leaves it.
    fn decided_column(p: &Params, n: u64, row: u64, prime: bool) -> Vec<Digit> {
        let sq4 = p.height_value(row);
        let h = (128 - sq4.leading_zeros()).div_ceil(p.w);
        (0..h)
            .map(|k| {
                let digit = |v: u128| ((v >> (k * p.w)) & p.mask()) as u64;
                let mut d = fresh_digit(k == 0);
                d.n = digit(n as u128);
                d.row = digit(row as u128);
                d.sq4 = digit(sq4);
                d.top = k == h - 1;
                d.head = None;
                if k == 0 {
                    d.prime = Some(prime);
                }
                d
            })
            .collect()
    }

    /// The first column of a wide row after its chain step: Q(L) = p_end + k - L running, digit j
    /// holding Q(t0 - j) at step t0 together with the borrow it sends up.
    fn running_source(p: &Params, n: u64, row: u64, p_end: u64, t0: u64) -> Vec<Digit> {
        let mut column = decided_column(p, n, row, false);
        let bits = p.w * column.len() as u32;
        let q = |l: i128| (p_end as i128 + p.k as i128 - l).rem_euclid(1i128 << bits);
        for (j, d) in column.iter_mut().enumerate() {
            let j = j as u32;
            let l = t0 as i128 - j as i128;
            d.prime = None;
            d.q = Some(((q(l) >> (j * p.w)) & p.mask() as i128) as u64);
            d.q_borrow = q(l - 1) & ((1i128 << (p.w * (j + 1))) - 1) == 0;
        }
        column
    }

    /// The suffix pass, the chain (from a narrow tile, then from a wide row), the broadcast and the
    /// countdowns with the real clock and no head start: two wide rows y0, y0 + 1 whose primality is
    /// decided, 3 y0 + 50 steps before the first deadline. Cases: the real narrow-to-wide boundary for
    /// W = 14; countdowns several digits wide (W = 6, 4); columns one digit taller than the ones
    /// below (W = 4, rows 127 and 128).
    #[test]
    fn wide_rows_flip_exactly_on_time() {
        let pi = pi_table(9000);
        let is_prime = |n: u64| pi[n as usize] > pi[n as usize - 1];
        let tau = |n: u64| pi[(n - 1) as usize] + 1;
        let start = |y: u64| y * (y - 1) / 2 + 1;
        // (W, y0, K, the chain source below row y0 is a narrow tile rather than a wide column)
        let cases = [
            (14u32, 64u64, 0u64, true),
            (6, 50, 0, false),
            (4, 50, 0, false),
            (4, 127, 0, false),
        ];
        for (w, y0, k, narrow_source) in cases {
            let p = Params { w, k };
            assert!(!p.is_narrow(y0) && p.is_narrow(y0 - 1) == narrow_source);
            // Enough lead for the suffix pass and the broadcast along a row (about 2 y0 steps).
            let t0 = tau(start(y0)) + k - (3 * y0 + 50);
            let mut sim = SparseSim {
                params: p,
                cells: std::collections::HashMap::new(),
                time: t0,
                y_max: y0 as i64 + 1,
            };
            // The chain source: the first tile of row y0 - 1, as the rule leaves it.
            let first = start(y0 - 1);
            let last = start(y0) - 1;
            if narrow_source {
                sim.cells.insert(
                    (0, y0 as i64 - 1, 0),
                    Cell3::Narrow(Tile {
                        n: first,
                        row: y0 - 1,
                        time: t0,
                        set: false,
                        d: 0,
                        prime: Some(is_prime(first)),
                        suffix: Some(pi[last as usize] - pi[first as usize - 1]),
                        p_end: Some(pi[last as usize]),
                        tau: Some(tau(first)),
                        wait: 0,
                    }),
                );
            } else {
                let column = running_source(&p, first, y0 - 1, pi[last as usize], t0);
                for (z, d) in column.into_iter().enumerate() {
                    sim.cells
                        .insert((0, y0 as i64 - 1, z as i64), Cell3::Digit(d));
                }
            }
            let mut heights = vec![];
            for y in y0..=y0 + 1 {
                for x in 0..y {
                    let n = start(y) + x;
                    let column = decided_column(&p, n, y, is_prime(n));
                    heights.push(column.len());
                    for (z, d) in column.into_iter().enumerate() {
                        sim.cells
                            .insert((x as i64, y as i64, z as i64), Cell3::Digit(d));
                    }
                }
            }
            let numbers: Vec<u64> = (start(y0)..start(y0 + 2)).collect();
            let end = numbers.iter().map(|&n| tau(n)).max().unwrap() + k + 3;
            while sim.time < end {
                sim.step();
                let set = sim
                    .cells
                    .iter()
                    .filter(|(&(_, y, _), c)| y >= y0 as i64 && c.is_set())
                    .count();
                let due = numbers.iter().filter(|&&n| tau(n) + k <= sim.time).count();
                assert_eq!(set, due, "w = {} y0 = {}: step {}", w, y0, sim.time);
            }
            // Each case exercises what it is meant to.
            let taller = heights.iter().max() != heights.iter().min();
            let wide_countdown = end - t0 >= 1 << w;
            assert!(taller || wide_countdown || narrow_source);
        }
    }
}
