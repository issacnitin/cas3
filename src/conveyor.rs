use crate::construct::{LocalRule, LogicalConfig, LogicalPoint};

const NORTH: usize = 6;
const EAST: usize = 4;
const SOUTH: usize = 1;
const WEST: usize = 3;
const ROLE_COUNT: usize = 4;
const GATE_NONE: usize = 0;
const GATE_NORTH: usize = 1;
const GATE_EAST: usize = 2;
const GATE_SOUTH: usize = 3;
const GATE_WEST: usize = 4;
const GATE_ROLE_COUNT: usize = 5;
const HEAD_TRACK_STATES: usize = ROLE_COUNT * GATE_ROLE_COUNT * 2;
const HEAD_GATE_BASE: usize = 1 + HEAD_TRACK_STATES;
const HEAD_GATE_STATES: usize = ROLE_COUNT * 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pull {
    North,
    East,
    South,
    West,
}

impl Pull {
    fn code(self) -> usize {
        match self {
            Pull::North => 0,
            Pull::East => 1,
            Pull::South => 2,
            Pull::West => 3,
        }
    }

    fn slot(self) -> usize {
        match self {
            Pull::North => NORTH,
            Pull::East => EAST,
            Pull::South => SOUTH,
            Pull::West => WEST,
        }
    }

    fn direction_code(delta: LogicalPoint) -> Result<usize, String> {
        match delta {
            (0, 1) => Ok(GATE_NORTH),
            (1, 0) => Ok(GATE_EAST),
            (0, -1) => Ok(GATE_SOUTH),
            (-1, 0) => Ok(GATE_WEST),
            _ => Err(format!("direction is not cardinal: {:?}", delta)),
        }
    }

    fn direction_slot(direction: usize) -> Option<usize> {
        match direction {
            GATE_NORTH => Some(NORTH),
            GATE_EAST => Some(EAST),
            GATE_SOUTH => Some(SOUTH),
            GATE_WEST => Some(WEST),
            _ => None,
        }
    }
}

pub struct CyclicConveyorRule {
    payload_states: usize,
}

impl CyclicConveyorRule {
    pub fn new(payload_states: usize) -> Result<Self, String> {
        if payload_states == 0 {
            return Err("a cyclic conveyor needs at least one payload state".to_string());
        }
        ROLE_COUNT
            .checked_mul(payload_states)
            .and_then(|states| states.checked_add(1))
            .ok_or_else(|| "cyclic conveyor alphabet is too large".to_string())?;
        Ok(CyclicConveyorRule { payload_states })
    }

    fn encode(&self, pull: Pull, payload: usize) -> Result<usize, String> {
        if payload >= self.payload_states {
            return Err("cyclic conveyor payload is outside its alphabet".to_string());
        }
        Ok(1 + pull.code() * self.payload_states + payload)
    }

    fn decode(&self, state: usize) -> Option<(Pull, usize)> {
        let raw = state.checked_sub(1)?;
        if raw >= ROLE_COUNT * self.payload_states {
            return None;
        }
        let pull = match raw / self.payload_states {
            0 => Pull::North,
            1 => Pull::East,
            2 => Pull::South,
            3 => Pull::West,
            _ => return None,
        };
        Some((pull, raw % self.payload_states))
    }
}

impl LocalRule for CyclicConveyorRule {
    fn state_count(&self) -> usize {
        1 + ROLE_COUNT * self.payload_states
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let Some((pull, _)) = self.decode(center) else {
            return 0;
        };
        let payload = self
            .decode(neighbours[pull.slot()])
            .map(|(_, payload)| payload)
            .unwrap_or(0);
        self.encode(pull, payload).unwrap()
    }
}

pub struct CyclicConveyor {
    pub config: LogicalConfig,
    pub steps: usize,
    path: Vec<LogicalPoint>,
    rule: CyclicConveyorRule,
}

#[derive(Clone, Copy)]
struct HeadTrack {
    push: usize,
    gate: usize,
    head: bool,
}

pub struct GatedHeadRule;

impl GatedHeadRule {
    fn encode_track(track: HeadTrack) -> usize {
        1 + ((track.push - 1) * GATE_ROLE_COUNT + track.gate) * 2 + track.head as usize
    }

    fn decode_track(state: usize) -> Option<HeadTrack> {
        let raw = state.checked_sub(1)?;
        if raw >= HEAD_TRACK_STATES {
            return None;
        }
        let head = raw & 1 != 0;
        let roles = raw / 2;
        Some(HeadTrack {
            push: roles / GATE_ROLE_COUNT + 1,
            gate: roles % GATE_ROLE_COUNT,
            head,
        })
    }

    fn encode_gate(track_direction: usize, ready: bool) -> usize {
        HEAD_GATE_BASE + (track_direction - 1) * 2 + ready as usize
    }

    fn decode_gate(state: usize) -> Option<(usize, bool)> {
        let raw = state.checked_sub(HEAD_GATE_BASE)?;
        if raw >= HEAD_GATE_STATES {
            return None;
        }
        Some((raw / 2 + 1, raw & 1 != 0))
    }

    fn gate_ready(track: HeadTrack, neighbours: [usize; 8]) -> bool {
        track.gate == GATE_NONE
            || Pull::direction_slot(track.gate)
                .and_then(|slot| Self::decode_gate(neighbours[slot]))
                .map(|(_, ready)| ready)
                .unwrap_or(false)
    }

    fn predecessor_gate_slot(predecessor_slot: usize, gate_direction: usize) -> Option<usize> {
        match (predecessor_slot, gate_direction) {
            (WEST, GATE_NORTH) | (NORTH, GATE_WEST) => Some(5),
            (WEST, GATE_SOUTH) | (SOUTH, GATE_WEST) => Some(0),
            (EAST, GATE_NORTH) | (NORTH, GATE_EAST) => Some(7),
            (EAST, GATE_SOUTH) | (SOUTH, GATE_EAST) => Some(2),
            _ => None,
        }
    }

    fn points_to_center(predecessor_slot: usize, push: usize) -> bool {
        matches!(
            (predecessor_slot, push),
            (WEST, GATE_EAST) | (EAST, GATE_WEST) | (SOUTH, GATE_NORTH) | (NORTH, GATE_SOUTH)
        )
    }
}

impl LocalRule for GatedHeadRule {
    fn state_count(&self) -> usize {
        HEAD_GATE_BASE + HEAD_GATE_STATES
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if let Some((track_direction, ready)) = Self::decode_gate(center) {
            if !ready {
                return center;
            }
            let track_slot = Pull::direction_slot(track_direction).unwrap();
            return if Self::decode_track(neighbours[track_slot])
                .map(|track| track.head)
                .unwrap_or(false)
            {
                Self::encode_gate(track_direction, false)
            } else {
                center
            };
        }

        let Some(mut track) = Self::decode_track(center) else {
            return 0;
        };
        if track.head {
            if Self::gate_ready(track, neighbours) {
                track.head = false;
            }
            return Self::encode_track(track);
        }

        for predecessor_slot in [WEST, EAST, SOUTH, NORTH] {
            let Some(predecessor) = Self::decode_track(neighbours[predecessor_slot]) else {
                continue;
            };
            if !predecessor.head || !Self::points_to_center(predecessor_slot, predecessor.push) {
                continue;
            }
            let ready = if predecessor.gate == GATE_NONE {
                true
            } else {
                Self::predecessor_gate_slot(predecessor_slot, predecessor.gate)
                    .and_then(|slot| Self::decode_gate(neighbours[slot]))
                    .map(|(_, ready)| ready)
                    .unwrap_or(false)
            };
            if ready {
                track.head = true;
                break;
            }
        }
        Self::encode_track(track)
    }
}

pub struct GatedHeadRing {
    pub config: LogicalConfig,
    pub steps: usize,
    path: Vec<LogicalPoint>,
}

impl GatedHeadRing {
    pub fn new(width: usize, height: usize) -> Result<Self, String> {
        if width < 3 || height < 3 {
            return Err("a gated head ring must be at least 3 by 3".to_string());
        }
        let path = rectangle_path(width, height)?;
        let width_i32 =
            i32::try_from(width).map_err(|_| "head ring width does not fit i32".to_string())?;
        let height_i32 =
            i32::try_from(height).map_err(|_| "head ring height does not fit i32".to_string())?;
        let mut config = LogicalConfig::new();
        for (index, point) in path.iter().copied().enumerate() {
            let successor = path[(index + 1) % path.len()];
            let push = Pull::direction_code((successor.0 - point.0, successor.1 - point.1))?;
            let gate = if point.1 == 0 && push == GATE_EAST {
                GATE_SOUTH
            } else if point.0 == width_i32 - 1 && push == GATE_NORTH {
                GATE_EAST
            } else if point.1 == height_i32 - 1 && push == GATE_WEST {
                GATE_NORTH
            } else {
                GATE_WEST
            };
            config.insert(
                point,
                GatedHeadRule::encode_track(HeadTrack {
                    push,
                    gate,
                    head: index == 0,
                }),
            );
            let gate_point = match gate {
                GATE_NORTH => (point.0, point.1 + 1),
                GATE_EAST => (point.0 + 1, point.1),
                GATE_SOUTH => (point.0, point.1 - 1),
                GATE_WEST => (point.0 - 1, point.1),
                _ => unreachable!(),
            };
            if config
                .insert(
                    gate_point,
                    GatedHeadRule::encode_gate(
                        match gate {
                            GATE_NORTH => GATE_SOUTH,
                            GATE_EAST => GATE_WEST,
                            GATE_SOUTH => GATE_NORTH,
                            GATE_WEST => GATE_EAST,
                            _ => unreachable!(),
                        },
                        true,
                    ),
                )
                .is_some()
            {
                return Err(format!("gated head ring gate overlap at {:?}", gate_point));
            }
        }
        Ok(GatedHeadRing {
            config,
            steps: 0,
            path,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = GatedHeadRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn head_index(&self) -> Option<usize> {
        self.path.iter().position(|point| {
            self.config
                .get(point)
                .and_then(|state| GatedHeadRule::decode_track(*state))
                .map(|track| track.head)
                .unwrap_or(false)
        })
    }
}

impl CyclicConveyor {
    pub fn new(width: usize, height: usize, payloads: &[usize]) -> Result<Self, String> {
        if width < 2 || height < 2 {
            return Err("a cyclic conveyor rectangle must be at least 2 by 2".to_string());
        }
        let path = rectangle_path(width, height)?;
        if payloads.len() != path.len() {
            return Err(format!(
                "cyclic conveyor needs {} payloads, received {}",
                path.len(),
                payloads.len()
            ));
        }
        let payload_states = payloads
            .iter()
            .copied()
            .max()
            .and_then(|value| value.checked_add(1))
            .unwrap_or(1);
        let rule = CyclicConveyorRule::new(payload_states)?;
        let mut config = LogicalConfig::new();
        for (index, point) in path.iter().copied().enumerate() {
            let predecessor = path[(index + path.len() - 1) % path.len()];
            let pull = pull_from(point, predecessor)?;
            config.insert(point, rule.encode(pull, payloads[index])?);
        }
        Ok(CyclicConveyor {
            config,
            steps: 0,
            path,
            rule,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = self.rule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn payloads(&self) -> Result<Vec<usize>, String> {
        self.path
            .iter()
            .map(|point| {
                self.config
                    .get(point)
                    .and_then(|state| self.rule.decode(*state))
                    .map(|(_, payload)| payload)
                    .ok_or_else(|| format!("cyclic conveyor lost track cell {:?}", point))
            })
            .collect()
    }
}

fn rectangle_path(width: usize, height: usize) -> Result<Vec<LogicalPoint>, String> {
    let width =
        i32::try_from(width).map_err(|_| "cyclic conveyor width does not fit i32".to_string())?;
    let height =
        i32::try_from(height).map_err(|_| "cyclic conveyor height does not fit i32".to_string())?;
    let mut path = Vec::new();
    for x in 0..width {
        path.push((x, 0));
    }
    for y in 1..height {
        path.push((width - 1, y));
    }
    for x in (0..width - 1).rev() {
        path.push((x, height - 1));
    }
    for y in (1..height - 1).rev() {
        path.push((0, y));
    }
    Ok(path)
}

fn pull_from(center: LogicalPoint, predecessor: LogicalPoint) -> Result<Pull, String> {
    match (predecessor.0 - center.0, predecessor.1 - center.1) {
        (0, 1) => Ok(Pull::North),
        (1, 0) => Ok(Pull::East),
        (0, -1) => Ok(Pull::South),
        (-1, 0) => Ok(Pull::West),
        delta => Err(format!("conveyor predecessor is not adjacent: {:?}", delta)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    #[test]
    fn rectangular_conveyor_rotates_payloads_without_reordering() {
        let payloads = (0..12).collect::<Vec<_>>();
        let mut conveyor = CyclicConveyor::new(4, 4, &payloads).unwrap();
        let active = conveyor.config.len();
        for rotation in 1..=24 {
            conveyor.step().unwrap();
            assert_eq!(conveyor.config.len(), active);
            let expected = (0..payloads.len())
                .map(|index| {
                    payloads[(index + payloads.len() - rotation % payloads.len()) % payloads.len()]
                })
                .collect::<Vec<_>>();
            assert_eq!(conveyor.payloads().unwrap(), expected);
        }
    }

    #[test]
    fn conveyor_rule_compiles_to_constant_weight_binary_blocks() {
        let conveyor = CyclicConveyor::new(3, 3, &[0, 1, 2, 3, 4, 5, 6, 7]).unwrap();
        let machine = BlockMachine::new(CyclicConveyorRule::new(8).unwrap()).unwrap();
        let binary = machine.codec.encode(&conveyor.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&conveyor.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn gated_head_visits_ready_family_slots_in_ring_order() {
        let mut ring = GatedHeadRing::new(5, 4).unwrap();
        let active = ring.config.len();
        let length = ring.path.len();
        assert_eq!(ring.head_index(), Some(0));
        for expected in 1..=length {
            ring.step().unwrap();
            assert_eq!(ring.config.len(), active);
            assert_eq!(ring.head_index(), Some(expected % length));
        }
        for _ in 0..10 {
            ring.step().unwrap();
            assert_eq!(ring.head_index(), Some(0));
        }
    }

    #[test]
    fn gated_head_ring_compiles_to_binary_blocks() {
        let ring = GatedHeadRing::new(4, 4).unwrap();
        let machine = BlockMachine::new(GatedHeadRule).unwrap();
        let binary = machine.codec.encode(&ring.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&ring.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn conveyor_rejects_invalid_geometry_and_payload_count() {
        assert!(CyclicConveyor::new(1, 4, &[0]).is_err());
        assert!(CyclicConveyor::new(4, 4, &[0]).is_err());
    }
}
