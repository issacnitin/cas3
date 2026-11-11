/*
* Class-wide arguments that need no search.
*
* Light cone. With a quiescent background a cell can only become set next to a set cell, so after t
* steps every set cell is within Chebyshev distance r * t of the seed (r = neighbourhood radius) and
* the population is at most (2 r t + 1)^dim. In one dimension that is linear in t, while the t-th
* prime exceeds t ln t (Rosser), which is at least (2 r + 1) t once t >= e^(2 r + 1). So no
* one-dimensional rule of any finite radius can have the primes as its population for every t.
* From dimension 2 the bound grows faster than the primes and excludes nothing.
*
* Any starting configuration reduces to close pairs. Suppose the counts at two consecutive times are
* 2 and 3, and let {a, b} be the two set cells. A cell z reads a exactly when a is in z + N0, where
* N0 is the neighbourhood with the cell itself added. If b - a is not in N0 - N0, no cell reads both,
* so each set cell evolves alone: the next configuration is (a + S) union (b + S) for the image S of
* a single set cell, the two parts are disjoint (a common cell would read both), and the count is
* 2 |S|, which is even, not 3. So a run 2, 3, 5, 7, ... starting from ANY configuration, after any
* transient, passes through a pair {a, a + d} with d in (N0 - N0) \ {0}, and the CA forgets
* everything else: checking those finitely many pair seeds settles every starting configuration.
*
* Parity. If the rule is invariant under the point reflection x -> -x (every isotropic or totalistic
* rule is) and d has an odd coordinate, the midpoint of the pair is not a cell. The reflection through
* the midpoint maps the pair to itself and commutes with the rule, so every later configuration is
* symmetric under it; it fixes no cell, so the count stays even forever and never reaches 3.
*/

use crate::lattice::{Neighbourhood, SymmetryGroup};

/// Largest population a single seed can reach after `t` steps.
pub fn light_cone_limit(dim: usize, radius: usize, t: usize) -> u128 {
    ((2 * radius * t + 1) as u128).saturating_pow(dim as u32)
}

/// First term (counted from t = 0) of `target` that no rule of this dimension and radius can reach.
pub fn first_light_cone_violation(dim: usize, radius: usize, target: &[usize]) -> Option<usize> {
    target
        .iter()
        .enumerate()
        .find(|&(t, &count)| count as u128 > light_cone_limit(dim, radius, t))
        .map(|(t, _)| t)
}

/// From this step on, the t-th prime exceeds 2 r t + 1 for good (Rosser: p_t > t ln t >= (2r+1) t).
pub fn one_dimensional_deadline(radius: usize) -> usize {
    ((2 * radius + 1) as f64).exp().ceil() as usize
}

/// Every d in (N0 - N0) \ {0}, one of each pair {d, -d} (the lexicographically larger).
pub fn interacting_pair_offsets(nb: &Neighbourhood) -> Vec<Vec<i32>> {
    let mut reach: Vec<Vec<i32>> = nb.offsets.clone();
    reach.push(vec![0; nb.dim]);
    let mut out: Vec<Vec<i32>> = vec![];
    for a in &reach {
        for b in &reach {
            let d: Vec<i32> = a.iter().zip(b).map(|(x, y)| x - y).collect();
            let negated: Vec<i32> = d.iter().map(|x| -x).collect();
            if d.iter().any(|&x| x != 0) && d > negated {
                out.push(d);
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// One representative per orbit of {d, -d} under `group`: the representative is the largest image.
pub fn pair_representatives(offsets: &[Vec<i32>], group: &SymmetryGroup) -> Vec<Vec<i32>> {
    let mut reps: Vec<Vec<i32>> = offsets
        .iter()
        .map(|d| {
            let mut best = d.clone();
            for image in group.elements.iter().map(|g| g.apply_offset(d)) {
                let negated: Vec<i32> = image.iter().map(|x| -x).collect();
                best = best.max(image).max(negated);
            }
            let negated: Vec<i32> = d.iter().map(|x| -x).collect();
            best.max(negated)
        })
        .collect();
    reps.sort();
    reps.dedup();
    reps
}

/// The two seed cells of the pair with offset d: centred on the origin when the midpoint is a cell.
pub fn pair_seed(d: &[i32]) -> Vec<Vec<i32>> {
    if d.iter().all(|x| x % 2 == 0) {
        vec![d.iter().map(|x| -x / 2).collect(), d.iter().map(|x| x / 2).collect()]
    } else {
        vec![vec![0; d.len()], d.to_vec()]
    }
}

/// Parity lemma: false when a point-symmetric rule keeps the count of this pair even forever.
pub fn pair_has_central_cell(d: &[i32]) -> bool {
    d.iter().all(|x| x % 2 == 0)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::lattice::{hyperoctahedral_group, Neighbourhood, SymmetryGroup};
    use crate::sequences::primes_target;
    use std::collections::HashSet;

    /// primes[0] = 1 (the seed term), primes[t] = t-th prime.
    fn primes_by_index(limit: usize) -> Vec<usize> {
        let mut composite = vec![false; limit + 1];
        let mut primes = vec![1usize];
        for n in 2..=limit {
            if !composite[n] {
                primes.push(n);
                let mut m = n * n;
                while m <= limit {
                    composite[m] = true;
                    m += n;
                }
            }
        }
        primes
    }

    #[test]
    fn light_cone_limits() {
        assert_eq!(light_cone_limit(1, 1, 0), 1);
        assert_eq!(light_cone_limit(1, 3, 4), 25);
        assert_eq!(light_cone_limit(2, 1, 3), 49);
        assert_eq!(light_cone_limit(3, 1, 2), 125);
    }

    #[test]
    fn one_dimension_runs_out_of_room_and_higher_dimensions_do_not() {
        let primes = primes_target(200);
        assert_eq!(first_light_cone_violation(1, 1, &primes), Some(7));
        // The 31st prime is 127 > 4 * 31 + 1 = 125, and no earlier prime exceeds 4 t + 1.
        assert_eq!(first_light_cone_violation(1, 2, &primes), Some(31));
        assert_eq!(first_light_cone_violation(2, 1, &primes), None);
        assert_eq!(first_light_cone_violation(3, 1, &primes), None);
    }

    #[test]
    fn primes_leave_every_one_dimensional_light_cone_for_good() {
        for radius in 1..=5usize {
            let deadline = one_dimensional_deadline(radius);
            let primes = primes_by_index(4_000_000);
            assert!(primes.len() > 3 * deadline, "sieve too small for radius {}", radius);
            // Past the analytic deadline the prime always exceeds the light-cone limit ...
            for t in deadline..=3 * deadline {
                assert!(primes[t] > 2 * radius * t + 1, "radius {}, t {}", radius, t);
            }
            // ... and before it there are only finitely many steps where it fits.
            let last_fit = (1..deadline).rev().find(|&t| primes[t] <= 2 * radius * t + 1);
            assert!(last_fit.is_some());
        }
    }

    #[test]
    fn close_pairs_of_the_square_and_the_cube() {
        let square = Neighbourhood::moore(2, 1).unwrap();
        let pairs = interacting_pair_offsets(&square);
        assert_eq!(pairs.len(), 12);
        let reps = pair_representatives(&pairs, &SymmetryGroup::preserving(&square));
        assert_eq!(reps, vec![vec![1, 0], vec![1, 1], vec![2, 0], vec![2, 1], vec![2, 2]]);
        assert_eq!(reps.iter().filter(|d| pair_has_central_cell(d)).count(), 2);
        assert_eq!(pair_seed(&[2, 0]), vec![vec![-1, 0], vec![1, 0]]);
        assert_eq!(pair_seed(&[2, 1]), vec![vec![0, 0], vec![2, 1]]);

        let cube = Neighbourhood::moore(3, 1).unwrap();
        let pairs = interacting_pair_offsets(&cube);
        assert_eq!(pairs.len(), 62);
        let reps = pair_representatives(&pairs, &SymmetryGroup::preserving(&cube));
        assert_eq!(reps.len(), 9);
        assert_eq!(reps.iter().filter(|d| pair_has_central_cell(d)).count(), 3);
        let tee = Neighbourhood::new(2, vec![vec![1, 0], vec![-1, 0], vec![0, 1]]).unwrap();
        // N0 = {0, (1,0), (-1,0), (0,1)}: up to sign (1,0), (0,1), (2,0), (1,1), (1,-1).
        assert_eq!(
            interacting_pair_offsets(&tee),
            vec![vec![0, 1], vec![1, -1], vec![1, 0], vec![1, 1], vec![2, 0]]
        );
    }

    #[test]
    fn distant_pairs_evolve_as_two_copies_with_an_even_count() {
        // Brute-force check of the reduction: for random rules, two cells outside N0 - N0 always
        // produce an even count after one step, and the configuration is two translated copies.
        use crate::reference::{reference_step, splitmix64, Config, RuleFn};
        struct Random(u64);
        impl RuleFn for Random {
            fn birth(&self, mask: u64) -> bool {
                mask != 0 && splitmix64(self.0 ^ mask) & 1 == 1
            }
            fn survive(&self, mask: u64) -> bool {
                splitmix64(self.0 ^ mask ^ 0xABCD) & 1 == 1
            }
        }
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let close: HashSet<Vec<i32>> = interacting_pair_offsets(&nb).into_iter().collect();
        for seed in 0..200u64 {
            let rule = Random(splitmix64(seed));
            let single: Config = [vec![0, 0]].into_iter().collect();
            let image = reference_step(&nb, &rule, &single);
            for dx in -4..=4i32 {
                for dy in -4..=4i32 {
                    let d = vec![dx, dy];
                    let negated = vec![-dx, -dy];
                    if (dx, dy) == (0, 0) || close.contains(&d) || close.contains(&negated) {
                        continue;
                    }
                    let pair: Config = [vec![0, 0], d.clone()].into_iter().collect();
                    let next = reference_step(&nb, &rule, &pair);
                    assert_eq!(next.len(), 2 * image.len(), "rule {} offset {:?}", seed, d);
                }
            }
        }
    }

    #[test]
    fn isotropic_rules_cannot_reach_a_count_of_two_in_higher_dimensions() {
        // A rule invariant under the symmetries of the neighbourhood decides births per orbit, so after
        // one step the population is [seed survives] + a sum of whole orbit sizes. A count of 2 needs an
        // orbit of size 1 (with the seed surviving) or of size 2 (with it dying).
        let neighbourhoods = vec![
            Neighbourhood::moore(2, 1).unwrap(),
            Neighbourhood::moore(2, 2).unwrap(),
            Neighbourhood::von_neumann(2).unwrap(),
            Neighbourhood::moore(3, 1).unwrap(),
            Neighbourhood::von_neumann(3).unwrap(),
            Neighbourhood::von_neumann(4).unwrap(),
        ];
        for nb in neighbourhoods {
            let mut elements = vec![crate::lattice::GroupElement { perm: (0..nb.dim).collect(), sign: vec![1; nb.dim] }];
            elements.extend(SymmetryGroup::preserving(&nb).elements);
            let mut smallest = usize::MAX;
            for offset in &nb.offsets {
                let orbit: HashSet<Vec<i32>> = elements.iter().map(|g| g.apply_offset(offset)).collect();
                smallest = smallest.min(orbit.len());
            }
            assert!(smallest >= 4, "dimension {}: smallest orbit {}", nb.dim, smallest);
        }
        // One dimension is different: the two neighbours form an orbit of size 2.
        let line = Neighbourhood::moore(1, 1).unwrap();
        let orbit: HashSet<Vec<i32>> = hyperoctahedral_group(1).iter().map(|g| g.apply_offset(&line.offsets[0])).collect();
        assert_eq!(orbit.len(), 2);
    }
}
