/*
* Class-wide arguments that need no search.
*
* Light cone. With a quiescent background a cell can only become set next to a set cell, so after t
* steps every set cell is within Chebyshev distance r * t of the seed (r = neighbourhood radius) and
* the population is at most (2 r t + 1)^dim. In one dimension that is linear in t, while the t-th
* prime exceeds t ln t (Rosser), which is at least (2 r + 1) t once t >= e^(2 r + 1). So no
* one-dimensional rule of any finite radius can have the primes as its population for every t.
* From dimension 2 the bound grows faster than the primes and excludes nothing.
*/

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
