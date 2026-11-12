/*
* Can a cellular automaton find the next prime in constant time? Not in any dimension.
*
* The question. Lay out a prime p as p lit cells. Is there a rule, in some dimension, that turns this
* into exactly p' lit cells, p' the next prime, in a fixed number of steps k (the same k for every
* p)? The automaton of prime_display goes from one prime count to the next in one step, but only
* because its hidden machinery has been working ahead since step 0. Here the input is the prime.
*
* The model is as general as possible: the lattice Z^d for any d (the argument is the same on the
* Cayley graph of any finitely generated group, such as the hexagonal grid), any states (even
* infinitely many, like the 64-bit cells of prime_display), a local rule of radius r, any choice of
* the states that count as lit, and a layout: for every prime p a configuration c_p with exactly p
* lit cells, which may also hold any helper states. F(c) is the number of lit cells k steps after c.
*
* Light cone. After k steps a cell's state depends only on the cells within distance r k of it. So
* if two configurations differ in D cells, the configurations k steps later differ in at most M D
* cells, where M is the number of cells within distance r k (at most (2 r k + 1)^d on Z^d), and
* their lit counts by at most M D. Up to a translation, too: the rule commutes with translations.
*
* Theorem 1 (layouts that change little). Suppose that for consecutive primes q < p, c_p differs
* from a translate of c_q in at most A (p - q) + B ln p cells, for fixed A and B. Then for no k is
* F(c_p) the next prime for every prime p, nor even for every prime beyond some point.
*
*   Every layout that grows by adding cells has A = 1, B = 0: a line, the staircase of
*   prime_display, a spiral, any fixed filling order, also with any fixed helper pattern (finite or
*   infinite) added to all of them. Helper cells that spell out p in binary, or in any base, add
*   B ln p.
*
*   Proof. Write p_1 < p_2 < ... for the primes and d_n = p_{n+1} - p_n. If F(c_{p_n}) = p_{n+1}
*   for all large n, the light cone applied to c_{p_n} and c_{p_{n+1}} gives
*       d_{n+1} = F(c_{p_{n+1}}) - F(c_{p_n}) <= M (A d_n + B ln p_{n+1}).
*   Pintz (On the ratio of consecutive gaps between primes, arXiv:1406.2658, Theorem 1 with k = 1)
*   proved that for every large N some n in [N, 2N] has d_{n+1} > c L(N) ln N d_n, where c > 0 and
*   L(N) = ln ln N ln ln ln ln N / (ln ln ln N)^2 grows without bound. For those n,
*   ln p_{n+1} <= 2 ln N and d_n >= 1, so the right-hand side is at most M (A + 2 B) d_n ln N,
*   which is less than d_{n+1} as soon as c L(N) > M (A + 2 B). []
*
* Theorem 2 (every layout). If the rule must work for every arrangement of p lit cells with nothing
* else, it fails at p = 2. Two lit cells more than 2 r k apart evolve independently, and empty space
* produces no lit cells (or the count would be infinite), so their count after k steps is twice the
* count of one lit cell: even, never 3.
*
* Other layouts. F(c) = sum over cells x of f(patch of c of radius r k around x), with f = 1 when
* that patch makes x lit after k steps: a fixed weighted census of the input's small patches.
* Theorems 1 and 2 show the census cannot be the next prime for growing layouts, nor for rules that
* must accept every arrangement. A layout that rearranges its cells wholesale between consecutive
* primes is not covered; its census can only give the next prime if the layout already encodes
* the gap.
*
* What is possible instead:
*  * one step per prime once the automaton has been running since step 0 (prime_display). The work
*    for each prime is done in advance, so the total work does not shrink;
*  * from a bare prime the time must grow with p. A rule that works for every prime up to x needs a
*    light cone M of at least the largest ratio d_{n+1} / d_n up to x. `ratio_records` lists the
*    consecutive primes where that ratio reaches a new high; the first record above M refutes every
*    rule whose light cone has M cells.
*/

use rayon::prelude::*;

/// Three consecutive primes p < q < r.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Triple {
    pub p: u64,
    pub q: u64,
    pub r: u64,
}

impl Triple {
    pub fn gaps(&self) -> (u64, u64) {
        (self.q - self.p, self.r - self.q)
    }

    /// The second gap divided by the first.
    pub fn ratio(&self) -> f64 {
        let (first, second) = self.gaps();
        second as f64 / first as f64
    }

    fn exceeds(&self, other: &Triple) -> bool {
        let (a1, a2) = self.gaps();
        let (b1, b2) = other.gaps();
        a2 as u128 * b1 as u128 > b2 as u128 * a1 as u128
    }

    /// True when no rule whose light cone has `cone` cells can turn p into q lit cells and q into r,
    /// with q laid out as p plus q - p lit cells: those cells move the count by at most
    /// cone (q - p), less than r - q.
    pub fn refutes(&self, cone: u128) -> bool {
        let (first, second) = self.gaps();
        second as u128 > cone.saturating_mul(first as u128)
    }

    /// The largest light cone this triple refutes.
    pub fn largest_cone_refuted(&self) -> u64 {
        let (first, second) = self.gaps();
        (second - 1) / first
    }
}

/// Every prime below or equal to `limit` (plain sieve, for the base primes and for tests).
fn small_primes(limit: u64) -> Vec<u64> {
    let n = limit as usize;
    let mut composite = vec![false; n + 1];
    let mut primes = vec![];
    for i in 2..=n {
        if !composite[i] {
            primes.push(i as u64);
            let mut j = i * i;
            while j <= n {
                composite[j] = true;
                j += i;
            }
        }
    }
    primes
}

fn isqrt(n: u64) -> u64 {
    let mut x = (n as f64).sqrt() as u64;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// What the merge needs from one stretch of numbers: its first and last two primes, its prime
/// count, and the triples inside it that beat every earlier triple inside it.
struct Chunk {
    head: Vec<u64>,
    tail: Vec<u64>,
    count: u64,
    records: Vec<Triple>,
}

/// Sieves the odd numbers in [lo, hi) (lo odd) with the base primes and summarises the primes found.
fn scan_chunk(lo: u64, hi: u64, base: &[u64]) -> Chunk {
    let len = ((hi - lo + 1) / 2) as usize;
    let mut composite = vec![false; len];
    for &p in base.iter().skip(1) {
        if p * p >= hi {
            break;
        }
        let mut m = (p * p).max(lo.div_ceil(p) * p);
        if m % 2 == 0 {
            m += p;
        }
        let mut i = ((m - lo) / 2) as usize;
        while i < len {
            composite[i] = true;
            i += p as usize;
        }
    }
    let mut chunk = Chunk {
        head: vec![],
        tail: vec![],
        count: 0,
        records: vec![],
    };
    let mut last: [u64; 2] = [0, 0];
    let mut best: Option<Triple> = None;
    let mut push = |n: u64, chunk: &mut Chunk| {
        if chunk.head.len() < 2 {
            chunk.head.push(n);
        }
        if chunk.count >= 2 {
            let t = Triple {
                p: last[0],
                q: last[1],
                r: n,
            };
            if best.is_none_or(|b| t.exceeds(&b)) {
                best = Some(t);
                chunk.records.push(t);
            }
        }
        last = [last[1], n];
        chunk.count += 1;
    };
    if lo <= 2 && 2 < hi {
        push(2, &mut chunk);
    }
    for (i, &is_composite) in composite.iter().enumerate() {
        let n = lo + 2 * i as u64;
        if !is_composite && n > 1 {
            push(n, &mut chunk);
        }
    }
    chunk.tail = match chunk.count {
        0 => vec![],
        1 => vec![last[1]],
        _ => last.to_vec(),
    };
    chunk
}

/// The record triples up to `limit`: each has a larger ratio d_{n+1}/d_n than every earlier triple.
/// Also returns the number of primes up to `limit`.
pub fn ratio_records(limit: u64) -> (Vec<Triple>, u64) {
    ratio_records_in_chunks(limit, 1 << 23)
}

fn ratio_records_in_chunks(limit: u64, chunk_numbers: u64) -> (Vec<Triple>, u64) {
    debug_assert!(chunk_numbers % 2 == 0, "chunks must start at odd numbers");
    let base = small_primes(isqrt(limit) + 1);
    let end = limit + 1;
    // Chunks are sieved in parallel, a batch at a time, and merged in order.
    let batch = 4 * rayon::current_num_threads() as u64;
    let mut merged = Merge::default();
    let mut next = 0;
    while 1 + next * chunk_numbers < end {
        let summaries: Vec<Chunk> = (next..next + batch)
            .into_par_iter()
            .map(|j| {
                (
                    1 + j * chunk_numbers,
                    (1 + (j + 1) * chunk_numbers).min(end),
                )
            })
            .filter(|&(lo, _)| lo < end)
            .map(|(lo, hi)| scan_chunk(lo, hi, &base))
            .collect();
        for chunk in summaries {
            merged.add(chunk);
        }
        next += batch;
    }
    (merged.records, merged.count)
}

/// Joins chunk summaries in order: the last two primes so far, the records so far, the prime count.
#[derive(Default)]
struct Merge {
    last: Vec<u64>,
    records: Vec<Triple>,
    count: u64,
}

impl Merge {
    fn consider(&mut self, t: Triple) {
        if self.records.last().is_none_or(|b| t.exceeds(b)) {
            self.records.push(t);
        }
    }

    fn add(&mut self, chunk: Chunk) {
        // Triples that start in earlier chunks come first, then the chunk's own records.
        let mut window = self.last.clone();
        for &n in &chunk.head {
            if window.len() >= 2 && window.len() < self.last.len() + 2 {
                let k = window.len();
                self.consider(Triple {
                    p: window[k - 2],
                    q: window[k - 1],
                    r: n,
                });
            }
            window.push(n);
        }
        for t in chunk.records {
            self.consider(t);
        }
        self.count += chunk.count;
        self.last.extend(chunk.tail);
        let keep = self.last.len().saturating_sub(2);
        self.last.drain(..keep);
    }
}

/// The first three consecutive primes that refute every rule with a light cone of `cone` cells.
pub fn first_refutation(records: &[Triple], cone: u128) -> Option<Triple> {
    records.iter().copied().find(|t| t.refutes(cone))
}

/// Deterministic Miller-Rabin for 64-bit numbers, independent of the sieve.
pub fn is_prime(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    const BASES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for &p in &BASES {
        if n % p == 0 {
            return n == p;
        }
    }
    let mul = |a: u64, b: u64| (a as u128 * b as u128 % n as u128) as u64;
    let pow = |mut a: u64, mut e: u64| {
        let mut result = 1;
        while e > 0 {
            if e & 1 == 1 {
                result = mul(result, a);
            }
            a = mul(a, a);
            e >>= 1;
        }
        result
    };
    let s = (n - 1).trailing_zeros();
    let d = (n - 1) >> s;
    'bases: for &a in &BASES {
        let mut x = pow(a, d);
        if x == 1 || x == n - 1 {
            continue;
        }
        for _ in 1..s {
            x = mul(x, x);
            if x == n - 1 {
                continue 'bases;
            }
        }
        return false;
    }
    true
}

/// Checks with Miller-Rabin that p < q < r are primes with no prime between them.
pub fn are_consecutive_primes(t: &Triple) -> bool {
    t.p < t.q
        && t.q < t.r
        && [t.p, t.q, t.r].iter().all(|&n| is_prime(n))
        && !(t.p + 1..t.q).chain(t.q + 1..t.r).any(is_prime)
}

#[cfg(test)]
mod test {
    use super::*;

    fn naive_records(limit: u64) -> Vec<Triple> {
        let primes = small_primes(limit);
        let mut records: Vec<Triple> = vec![];
        for w in primes.windows(3) {
            let t = Triple {
                p: w[0],
                q: w[1],
                r: w[2],
            };
            if records.last().is_none_or(|b| t.exceeds(b)) {
                records.push(t);
            }
        }
        records
    }

    #[test]
    fn records_and_counts_match_a_plain_sieve_for_any_chunk_size() {
        let limit = 300_000;
        let expected = naive_records(limit);
        let primes = small_primes(limit).len() as u64;
        // Chunks of 2 and 6 numbers can hold zero or one prime, which tests the stitching.
        for chunk in [2, 6, 64, 1000, 1 << 23] {
            let (records, count) = ratio_records_in_chunks(limit, chunk);
            assert_eq!(records, expected, "chunk {}", chunk);
            assert_eq!(count, primes, "chunk {}", chunk);
        }
        assert_eq!(ratio_records(10_000_000).1, 664_579);
    }

    #[test]
    fn the_first_refutation_is_the_first_triple_beyond_the_cone() {
        let limit = 2_000_000;
        let (records, _) = ratio_records(limit);
        let primes = small_primes(limit);
        for cone in 1..=40u128 {
            let first = primes
                .windows(3)
                .map(|w| Triple {
                    p: w[0],
                    q: w[1],
                    r: w[2],
                })
                .find(|t| t.refutes(cone));
            assert_eq!(first_refutation(&records, cone), first, "cone {}", cone);
            if let Some(t) = first {
                assert!(t.largest_cone_refuted() as u128 >= cone);
                assert!(!t.refutes(t.largest_cone_refuted() as u128 + 1));
            }
        }
    }

    #[test]
    fn miller_rabin_agrees_with_the_sieve_and_confirms_the_records() {
        let primes = small_primes(100_000);
        let sieve: Vec<u64> = (0..=100_000).filter(|&n| is_prime(n)).collect();
        assert_eq!(sieve, primes);
        // Large primes and strong pseudoprimes to the first few bases.
        assert!(is_prime(18_446_744_073_709_551_557));
        for n in [3_215_031_751, 3_825_123_056_546_413_051] {
            assert!(!is_prime(n), "{}", n);
        }
        let (records, _) = ratio_records(50_000_000);
        assert!(records.iter().all(are_consecutive_primes));
    }
}
