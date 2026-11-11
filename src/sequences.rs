/*
* Target set-cell counts. Named sequences start with 1, the single seed cell at t = 0; for a larger
* seed they are aligned to start at the seed size (primes from a two-cell seed: 2, 3, 5, 7, ...).
*/

fn is_prime(n: usize) -> bool {
    n >= 2 && (2..).take_while(|d| d * d <= n).all(|d| n % d != 0)
}

/// 1 followed by the first `terms - 1` primes: 1, 2, 3, 5, 7, 11, ...
pub fn primes_target(terms: usize) -> Vec<usize> {
    let mut sequence = vec![1];
    let mut candidate = 2;
    while sequence.len() < terms {
        if is_prime(candidate) {
            sequence.push(candidate);
        }
        candidate += 1;
    }
    sequence.truncate(terms);
    sequence
}

/// `spec` is a named sequence (primes, naturals, odds, squares, powers2) of `terms` entries,
/// or an explicit comma separated list such as `1,2,3,5,7`.
pub fn parse_target(spec: &str, terms: usize) -> Result<Vec<usize>, String> {
    parse_target_from(spec, terms, 1)
}

/// As `parse_target` for a seed of `start` cells: a named sequence drops its terms below `start`
/// and must then begin with `start`; an explicit list must begin with `start`.
pub fn parse_target_from(spec: &str, terms: usize, start: usize) -> Result<Vec<usize>, String> {
    if terms == 0 {
        return Err("the target needs at least one term".to_string());
    }
    let named = |n: usize| -> Option<Vec<usize>> {
        Some(match spec {
            "primes" => primes_target(n),
            "naturals" => (1..=n).collect(),
            "odds" => (0..n).map(|i| 2 * i + 1).collect(),
            "squares" => (1..=n).map(|i| i * i).collect(),
            "powers2" => (0..n.min(63)).map(|i| 1usize << i).collect(),
            _ => return None,
        })
    };
    let sequence = match named(terms + start) {
        Some(full) => full.into_iter().skip_while(|&x| x < start).take(terms).collect(),
        None => spec
            .split(',')
            .map(|part| {
                part.trim()
                    .parse::<usize>()
                    .map_err(|_| format!("'{}' is neither a named sequence nor a list of counts", spec))
            })
            .collect::<Result<Vec<usize>, String>>()?,
    };
    if sequence.first() != Some(&start) {
        return Err(if start == 1 {
            "a target must start with 1 (the seed cell)".to_string()
        } else {
            format!("a target must start with {} (the number of seed cells)", start)
        });
    }
    Ok(sequence)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn primes_start_with_the_seed() {
        assert_eq!(primes_target(1), vec![1]);
        assert_eq!(primes_target(10), vec![1, 2, 3, 5, 7, 11, 13, 17, 19, 23]);
    }

    #[test]
    fn named_and_explicit_targets() {
        assert_eq!(parse_target("naturals", 4).unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(parse_target("odds", 4).unwrap(), vec![1, 3, 5, 7]);
        assert_eq!(parse_target("squares", 4).unwrap(), vec![1, 4, 9, 16]);
        assert_eq!(parse_target("1, 2, 3, 5", 99).unwrap(), vec![1, 2, 3, 5]);
        assert!(parse_target("2,3,5", 3).is_err());
        assert!(parse_target("primes", 0).is_err());
        assert!(parse_target("nonsense", 3).is_err());
    }

    #[test]
    fn targets_align_with_the_seed_size() {
        assert_eq!(parse_target_from("primes", 5, 2).unwrap(), vec![2, 3, 5, 7, 11]);
        assert_eq!(parse_target_from("naturals", 3, 2).unwrap(), vec![2, 3, 4]);
        assert_eq!(parse_target_from("2,3,5", 9, 2).unwrap(), vec![2, 3, 5]);
        assert!(parse_target_from("odds", 4, 2).is_err());
        assert!(parse_target_from("1,2,3", 3, 2).is_err());
    }
}
