/*
 * Finite one-seed bootstrap
 * =========================
 *
 * A fixed-radius rule may carry a finite exceptional transient and then delegate to another
 * finite-radius rule.  The exception table is keyed by a complete sparse local pattern, so it
 * is a CA rule rather than a host-side branch on generation or configuration size.
 *
 * For each finite bootstrap configuration, the compiler records the output of every candidate
 * cell, including false births.  The configured radius is at least the largest diameter needed
 * by the transient and at least the delegated block machine's radius.  After the last handoff
 * configuration no exception is used and the block compiler is authoritative.
 */

use std::collections::{BTreeMap, BTreeSet};

use crate::construct::{BinaryConfig, BlockMachine, LocalRule, LogicalPoint};
use crate::sequences::primes_target;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LocalPattern {
    center: bool,
    neighbours: BTreeSet<LogicalPoint>,
}

pub struct BinaryBootstrapRule<R> {
    machine: BlockMachine<R>,
    pub radius: i32,
    exceptions: BTreeMap<LocalPattern, bool>,
}

impl<R: LocalRule> BinaryBootstrapRule<R> {
    pub fn compile(
        machine: BlockMachine<R>,
        configurations: &[BinaryConfig],
        handoff: &BinaryConfig,
    ) -> Result<Self, String> {
        if configurations.first().map(|config| config.len()) != Some(1) {
            return Err("a bootstrap must start from one live binary cell".to_string());
        }
        if configurations.len() < 2 {
            return Err("a bootstrap needs at least one finite transition".to_string());
        }
        for pair in configurations.windows(2) {
            if pair[0].is_empty() && !pair[1].is_empty() {
                return Err(
                    "a quiescent configuration cannot build live cells under a CA rule".to_string(),
                );
            }
        }
        if configurations.last().is_some_and(BinaryConfig::is_empty) && !handoff.is_empty() {
            return Err(
                "a quiescent bootstrap cannot hand off to a live configuration".to_string(),
            );
        }
        let mut support: Vec<_> = configurations
            .iter()
            .chain(std::iter::once(handoff))
            .flat_map(|config| config.iter().copied())
            .collect();
        if support.is_empty() {
            return Err("bootstrap support is empty".to_string());
        }
        let min_x = support.iter().map(|point| point.0).min().unwrap();
        let max_x = support.iter().map(|point| point.0).max().unwrap();
        let min_y = support.iter().map(|point| point.1).min().unwrap();
        let max_y = support.iter().map(|point| point.1).max().unwrap();
        support.clear();
        let diameter = (max_x as i64 - min_x as i64)
            .abs()
            .max((max_y as i64 - min_y as i64).abs());
        let machine_radius = i64::from(machine.locality_radius());
        let radius = diameter.max(machine_radius).max(1).min(i64::from(i32::MAX));
        let radius = radius as i32;
        let mut exceptions = BTreeMap::new();
        let mut bootstrap_next = configurations
            .iter()
            .skip(1)
            .chain(std::iter::once(handoff));
        for current in configurations {
            let Some(next) = bootstrap_next.next() else {
                break;
            };
            let mut specified = current.clone();
            specified.extend(next.iter().copied());
            let delegated = machine.step_binary_local(current)?;
            if current.len() == 1 {
                specified.extend(delegated);
            } else if !delegated.is_empty() {
                return Err(
                    "an unframed bootstrap configuration unexpectedly activates the block rule"
                        .to_string(),
                );
            }
            for point in specified {
                let pattern = local_pattern(current, point, radius);
                let output = next.contains(&point);
                if let Some(existing) = exceptions.insert(pattern.clone(), output) {
                    if existing != output {
                        return Err(format!(
                            "bootstrap pattern conflict at {:?}: {} versus {}",
                            point, existing, output
                        ));
                    }
                }
            }
        }
        let rule = BinaryBootstrapRule {
            machine,
            radius,
            exceptions,
        };
        if radius <= 512 {
            for point in candidates(handoff, radius) {
                let pattern = local_pattern(handoff, point, radius);
                if let Some(exception) = rule.exceptions.get(&pattern) {
                    let delegated = rule.machine.next_binary_bit(point, handoff)?;
                    if *exception != delegated {
                        return Err(format!(
                            "bootstrap exception conflicts with the handoff rule at {:?}",
                            point
                        ));
                    }
                }
            }
        }
        if radius <= 512 {
            let mut expected = configurations
                .iter()
                .skip(1)
                .chain(std::iter::once(handoff))
                .map(BinaryConfig::clone);
            for current in configurations {
                let actual = rule.step(current)?;
                let expected = expected.next().ok_or_else(|| {
                    "bootstrap expected transition stream ended early".to_string()
                })?;
                if actual != expected {
                    return Err(
                        "combined bootstrap predicate does not realize a supplied transition"
                            .to_string(),
                    );
                }
            }
        }
        Ok(rule)
    }

    pub fn states(
        &self,
        seed: &BinaryConfig,
        bootstrap_steps: usize,
        delegated_steps: usize,
    ) -> Result<Vec<BinaryConfig>, String> {
        let mut states = vec![seed.clone()];
        for _ in 0..bootstrap_steps + delegated_steps {
            let next = self.step(states.last().unwrap())?;
            states.push(next);
        }
        Ok(states)
    }

    pub fn step(&self, current: &BinaryConfig) -> Result<BinaryConfig, String> {
        let mut next = BinaryConfig::new();
        for point in candidates(current, self.radius) {
            let pattern = local_pattern(current, point, self.radius);
            let output = if self.machine.has_framed_block_near(point, current)? {
                self.machine.next_binary_bit(point, current)?
            } else {
                match self.exceptions.get(&pattern) {
                    Some(output) => *output,
                    None => self.machine.next_binary_bit(point, current)?,
                }
            };
            if output {
                next.insert(point);
            }
        }
        Ok(next)
    }

    pub fn exception_count(&self) -> usize {
        self.exceptions.len()
    }
}

fn candidates(current: &BinaryConfig, radius: i32) -> BinaryConfig {
    let mut points = BTreeSet::new();
    for &(x, y) in current {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if let (Some(x), Some(y)) = (x.checked_add(dx), y.checked_add(dy)) {
                    points.insert((x, y));
                }
            }
        }
    }
    points
}

fn local_pattern(current: &BinaryConfig, point: LogicalPoint, radius: i32) -> LocalPattern {
    let mut neighbours = BTreeSet::new();
    let min_x = point.0.saturating_sub(radius);
    let max_x = point.0.saturating_add(radius);
    let min_y = point.1.saturating_sub(radius);
    let max_y = point.1.saturating_add(radius);
    for &(x, y) in current.range((min_x, i32::MIN)..=(max_x, i32::MAX)) {
        if y < min_y || y > max_y || (x, y) == point {
            continue;
        }
        if let (Some(dx), Some(dy)) = (x.checked_sub(point.0), y.checked_sub(point.1)) {
            neighbours.insert((dx, dy));
        }
    }
    LocalPattern {
        center: current.contains(&point),
        neighbours,
    }
}

pub fn prime_line_config(prime_population: usize) -> Result<BinaryConfig, String> {
    (0..prime_population)
        .map(|x| {
            i32::try_from(x)
                .map(|x| (x, 0))
                .map_err(|_| "prime bootstrap population does not fit i32".to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::{
        prepared_prime_output_config, prime_mass_trace, LogicalConfig, PreparedPrimeOutputRule,
        TAIL_BASE, TAIL_RESIDUES,
    };

    struct HandoffRule;

    impl LocalRule for HandoffRule {
        fn state_count(&self) -> usize {
            3
        }

        fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
            if center != 0 {
                center
            } else if neighbours[3] != 0 {
                2
            } else {
                0
            }
        }
    }

    struct TailHandoffRule;

    impl LocalRule for TailHandoffRule {
        fn state_count(&self) -> usize {
            6
        }

        fn next(&self, center: usize, _: [usize; 8]) -> usize {
            if center == 5 {
                5
            } else {
                0
            }
        }
    }

    #[test]
    fn finite_prime_bootstrap_is_a_compiled_local_binary_rule() {
        let target = primes_target(5);
        let lines = target
            .iter()
            .map(|population| prime_line_config(*population))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let machine = BlockMachine::new(HandoffRule).unwrap();
        let bootstrap = BinaryBootstrapRule::compile(machine, &lines[..4], &lines[4]).unwrap();
        let states = bootstrap.states(&lines[0], 4, 0).unwrap();
        assert_eq!(
            states.iter().map(BinaryConfig::len).collect::<Vec<_>>(),
            target
        );
        assert_eq!(states, lines);
        assert!(bootstrap.exception_count() > target.len());
    }

    #[test]
    fn bootstrap_hands_off_to_the_block_compiler() {
        let target = primes_target(5);
        let bootstrap_lines = target[..4]
            .iter()
            .map(|population| prime_line_config(*population))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let codec = crate::construct::BlockCodec::with_tail_states(2, 4).unwrap();
        let machine = BlockMachine::with_codec(TailHandoffRule, codec).unwrap();
        let logical_handoff = LogicalConfig::from([((0, 0), 5)]);
        let handoff = machine.codec.encode(&logical_handoff).unwrap();
        let bootstrap = BinaryBootstrapRule::compile(machine, &bootstrap_lines, &handoff).unwrap();
        let states = bootstrap.states(&bootstrap_lines[0], 4, 2).unwrap();
        assert_eq!(states[4], handoff);
        assert_eq!(states[5].len(), 7);
        assert_eq!(states[6].len(), 7);
    }

    #[test]
    fn one_binary_rule_reaches_the_first_scheduled_prime_split() {
        let target = primes_target(6);
        let bootstrap_lines = target[..5]
            .iter()
            .map(|population| prime_line_config(*population))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let transitions = prime_mass_trace(6, 4).unwrap();
        let logical_handoff = prepared_prime_output_config(&transitions[5], 4).unwrap();
        let codec =
            crate::construct::BlockCodec::with_tail_states(TAIL_BASE, TAIL_RESIDUES).unwrap();
        let machine = BlockMachine::with_codec(PreparedPrimeOutputRule, codec).unwrap();
        let handoff = machine.codec.encode(&logical_handoff).unwrap();
        assert_eq!(handoff.len(), 11);

        let bootstrap = BinaryBootstrapRule::compile(machine, &bootstrap_lines, &handoff).unwrap();
        let states = bootstrap.states(&bootstrap_lines[0], 5, 1).unwrap();
        assert_eq!(
            states.iter().map(BinaryConfig::len).collect::<Vec<_>>(),
            vec![1, 2, 3, 5, 7, 11, 13]
        );
    }

    #[test]
    fn framed_marker_permanently_disables_bootstrap_exceptions() {
        let machine = BlockMachine::new(HandoffRule).unwrap();
        let logical = LogicalConfig::from([((0, 0), 1)]);
        let handoff = machine.codec.encode(&logical).unwrap();
        let delegated = machine.step_binary(&handoff).unwrap();
        let raw = vec![prime_line_config(1).unwrap(), prime_line_config(2).unwrap()];
        let mut bootstrap = BinaryBootstrapRule::compile(machine, &raw, &handoff).unwrap();
        let point = *delegated.iter().next().unwrap();
        bootstrap
            .exceptions
            .insert(local_pattern(&handoff, point, bootstrap.radius), false);
        assert_eq!(bootstrap.step(&handoff).unwrap(), delegated);
    }
}
