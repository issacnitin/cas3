use std::sync::OnceLock;

use crate::construct::{LocalRule, LogicalConfig};

const WEST: usize = 3;
const EAST: usize = 4;

pub const SOLDIER: usize = 1;
pub const STATE_A: usize = 2;
pub const STATE_B: usize = 3;
pub const STATE_C: usize = 4;
pub const FIRE: usize = 5;
pub const GENERAL: usize = 6;
pub const STATE_Q: usize = 7;
pub const STATE_R: usize = 8;
pub const FIRING_ERROR: usize = 9;
pub const FIRING_STATE_COUNT: usize = 10;

const BOUNDARY: usize = usize::MAX;

#[derive(Clone, Copy)]
struct Transition {
    west: usize,
    center: usize,
    east: usize,
    next: usize,
}

fn decode(symbol: &str) -> usize {
    match symbol {
        "-" => SOLDIER,
        "A" => STATE_A,
        "B" => STATE_B,
        "C" => STATE_C,
        "F" => FIRE,
        "M" => GENERAL,
        "Q" => STATE_Q,
        "R" => STATE_R,
        "X" => BOUNDARY,
        _ => panic!("unknown firing-squad state {symbol}"),
    }
}

fn transitions() -> &'static [Transition] {
    static TRANSITIONS: OnceLock<Vec<Transition>> = OnceLock::new();
    TRANSITIONS.get_or_init(|| {
        BALZER_RULES
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                let fields = line.split_whitespace().collect::<Vec<_>>();
                assert_eq!(fields.len(), 4, "invalid Balzer transition {line}");
                Transition {
                    west: decode(fields[0]),
                    center: decode(fields[1]),
                    east: decode(fields[2]),
                    next: decode(fields[3]),
                }
            })
            .collect()
    })
}

/// Robert Balzer's 8-state minimal-time firing-squad rule.
///
/// The transition table is reproduced from David Moews' transcription:
/// https://djm.cc/fsquad/firing-solution-balzer.html
pub struct FiringSquadRule;

impl LocalRule for FiringSquadRule {
    fn state_count(&self) -> usize {
        FIRING_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 {
            return 0;
        }
        if center == FIRE || center == FIRING_ERROR {
            return center;
        }
        let west = if neighbours[WEST] == 0 {
            BOUNDARY
        } else {
            neighbours[WEST]
        };
        let east = if neighbours[EAST] == 0 {
            BOUNDARY
        } else {
            neighbours[EAST]
        };
        transitions()
            .iter()
            .find(|rule| rule.west == west && rule.center == center && rule.east == east)
            .map(|rule| rule.next)
            .unwrap_or(FIRING_ERROR)
    }
}

pub fn firing_line(length: usize) -> Result<LogicalConfig, String> {
    if length == 0 {
        return Err("a firing squad needs at least one soldier".to_string());
    }
    let mut config = LogicalConfig::new();
    for index in 0..length {
        let x =
            i32::try_from(index).map_err(|_| "firing-squad length does not fit i32".to_string())?;
        config.insert(
            (x, 0),
            if index + 1 == length {
                GENERAL
            } else {
                SOLDIER
            },
        );
    }
    Ok(config)
}

const BALZER_RULES: &str = r#"
A A A A
A Q A Q
A A B C
A B B Q
A A C C
A B C Q
A C C M
A - C C
A Q C M
A M X M
A A - -
A C - -
A - - -
A M - M
A Q - Q
A A M B
A B M -
A C M -
A - M C
A M M M
A R M C
A A Q A
A - Q -
A Q Q -
A R Q -
A A R -
A B R Q
A Q R -
B M A M
B B B B
B C B C
B B C B
B C C C
B R C C
B M X M
B M M M
B M Q M
B B R B
B C R C
B R R B
C M A M
C B B B
C C B R
C - B R
C R B R
C B C B
C C C R
C - C R
C R C R
C M X M
C B M C
C C M C
C - M C
C M M M
C M Q M
C B R B
C C R B
C - R B
C R R R
X M A M
X M B M
X - C M
X M C M
X M X F
X - - -
X - M M
X M M F
X M Q M
X M R M
- A A A
- - A A
- Q A Q
- A B C
- B B R
- C B R
- A C C
- B C R
- C C R
- - C C
- Q C M
- R C C
- M X M
- - - -
- M - M
- Q - Q
- A M B
- C M C
- - M C
- M M M
- R M B
- A Q A
- - Q -
- Q Q A
- R Q A
- C R B
- - R A
- M R M
- Q R A
- R R B
M M A M
M A B R
M B B A
M - B Q
M M B M
M Q B R
M R B Q
M A C Q
M B C A
M - C Q
M M C M
M Q C M
M R C Q
M M X F
M R X M
M Q - Q
M C M M
M - M M
M M M F
M Q M M
M R M M
M A Q Q
M M Q M
M Q Q Q
M R Q Q
M B R A
M - R A
M M R M
M Q R Q
M R R Q
Q A A A
Q - A Q
Q Q A Q
Q A B C
Q B B Q
Q C B M
Q Q B M
Q R B Q
Q A C C
Q B C Q
Q C C M
Q - C C
Q M C M
Q Q C M
Q R C Q
Q M X M
Q R X M
Q A - -
Q C - -
Q - - -
Q M - M
Q Q - Q
Q A M B
Q B M C
Q C M M
Q - M C
Q M M M
Q R M C
Q A Q A
Q - Q -
Q Q Q -
Q R Q -
Q A R -
Q B R Q
Q C R M
Q - R Q
Q Q R -
Q R R Q
R B B R
R C B C
R M B M
R Q B R
R R B R
R B C R
R C C C
R M C M
R Q C R
R R C R
R R X R
R C M C
R C R C
R M R M
R R R R
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    #[test]
    fn balzer_squad_first_fires_in_minimal_time() {
        for length in 1..=64 {
            let mut config = firing_line(length).unwrap();
            let fire_at = if length == 1 { 1 } else { 2 * length - 2 };
            for step in 0..fire_at {
                assert!(
                    config.values().all(|state| *state != FIRE),
                    "length {length} fired at step {step}"
                );
                assert!(
                    config.values().all(|state| *state != FIRING_ERROR),
                    "length {length} reached an undefined transition at step {step}"
                );
                config = FiringSquadRule.step(&config).unwrap();
            }
            assert_eq!(config.len(), length);
            assert!(config.values().all(|state| *state == FIRE));
        }
    }

    #[test]
    fn firing_squad_compiles_to_constant_weight_binary_blocks() {
        let logical = firing_line(12).unwrap();
        let machine = BlockMachine::new(FiringSquadRule).unwrap();
        let binary = machine.codec.encode(&logical).unwrap();
        let direct = machine.step_binary_local(&binary).unwrap();
        assert_eq!(direct, machine.step_binary(&binary).unwrap());
        assert_eq!(binary.len(), direct.len());
    }
}
