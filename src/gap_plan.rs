use crate::construct::{LocalRule, LogicalConfig};

const WEST: usize = 3;
const FLAGS: usize = 1 << 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Position {
    Low,
    High,
    Split,
}

impl Position {
    fn code(self) -> usize {
        match self {
            Position::Low => 0,
            Position::High => 1,
            Position::Split => 2,
        }
    }

    fn next(self) -> Self {
        match self {
            Position::Low => Position::High,
            Position::High | Position::Split => Position::Split,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Tape,
    Add {
        carry: bool,
        residue: u8,
        position: Position,
    },
    Done,
}

impl Phase {
    fn code(self) -> usize {
        match self {
            Phase::Tape => 0,
            Phase::Done => 1,
            Phase::Add {
                carry,
                residue,
                position,
            } => 2 + position.code() * 8 + residue as usize * 2 + carry as usize,
        }
    }

    fn decode(code: usize) -> Option<Self> {
        if code == 0 {
            return Some(Phase::Tape);
        }
        if code == 1 {
            return Some(Phase::Done);
        }
        let raw = code - 2;
        if raw >= 24 {
            return None;
        }
        let position = match raw / 8 {
            0 => Position::Low,
            1 => Position::High,
            2 => Position::Split,
            _ => return None,
        };
        Some(Phase::Add {
            carry: raw & 1 != 0,
            residue: ((raw % 8) / 2) as u8,
            position,
        })
    }

    const COUNT: usize = 26;
}

#[derive(Clone, Copy)]
struct PlanCell {
    gap: bool,
    sum: bool,
    lsb: bool,
    msb: bool,
    phase: Phase,
}

impl PlanCell {
    fn encode(self) -> usize {
        1 + self.phase.code() * FLAGS
            + self.gap as usize
            + 2 * self.sum as usize
            + 4 * self.lsb as usize
            + 8 * self.msb as usize
    }

    fn decode(state: usize) -> Option<Self> {
        let raw = state.checked_sub(1)?;
        if raw >= Phase::COUNT * FLAGS {
            return None;
        }
        let flags = raw % FLAGS;
        Some(PlanCell {
            gap: flags & 1 != 0,
            sum: flags & 2 != 0,
            lsb: flags & 4 != 0,
            msb: flags & 8 != 0,
            phase: Phase::decode(raw / FLAGS)?,
        })
    }
}

fn addition(cell: PlanCell, carry: bool, residue: u8, position: Position) -> (bool, bool) {
    let residue_bit = match position {
        Position::Low => residue & 1,
        Position::High => (residue >> 1) & 1,
        Position::Split => 0,
    };
    let sum = cell.gap as u8 + residue_bit + carry as u8;
    (sum & 1 != 0, sum > 1)
}

pub struct GapPlanRule;

impl LocalRule for GapPlanRule {
    fn state_count(&self) -> usize {
        1 + Phase::COUNT * FLAGS
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let Some(mut cell) = PlanCell::decode(center) else {
            return 0;
        };
        if let Phase::Add {
            carry,
            residue,
            position,
        } = cell.phase
        {
            let (sum, _) = addition(cell, carry, residue, position);
            cell.sum = sum;
            cell.phase = if cell.msb { Phase::Done } else { Phase::Tape };
            return cell.encode();
        }
        if cell.phase == Phase::Tape {
            if let Some(west) = PlanCell::decode(neighbours[WEST]) {
                if let Phase::Add {
                    carry,
                    residue,
                    position,
                } = west.phase
                {
                    let (_, carry) = addition(west, carry, residue, position);
                    cell.phase = Phase::Add {
                        carry,
                        residue,
                        position: position.next(),
                    };
                }
            }
        }
        cell.encode()
    }
}

pub struct GapPlan {
    pub config: LogicalConfig,
    pub steps: usize,
    width: usize,
}

impl GapPlan {
    pub fn new(gap: usize, residue: usize, width: usize) -> Result<Self, String> {
        if residue >= 4 {
            return Err("tail residue must be between 0 and 3".to_string());
        }
        if width < 3 || width > usize::BITS as usize {
            return Err("gap-plan width must be between 3 and usize::BITS".to_string());
        }
        if width < usize::BITS as usize && gap >= 1usize << (width - 1) {
            return Err("gap-plan tape needs one carry guard bit".to_string());
        }
        let mut config = LogicalConfig::new();
        for bit in 0..width {
            config.insert(
                (bit as i32, 0),
                PlanCell {
                    gap: gap & (1usize << bit) != 0,
                    sum: false,
                    lsb: bit == 0,
                    msb: bit + 1 == width,
                    phase: if bit == 0 {
                        Phase::Add {
                            carry: false,
                            residue: residue as u8,
                            position: Position::Low,
                        }
                    } else {
                        Phase::Tape
                    },
                }
                .encode(),
            );
        }
        Ok(GapPlan {
            config,
            steps: 0,
            width,
        })
    }

    pub fn run_until_done(&mut self) -> Result<(), String> {
        for _ in 0..=self.width {
            self.config = GapPlanRule.step(&self.config)?;
            self.steps += 1;
        }
        if !self.config.values().any(|state| {
            PlanCell::decode(*state)
                .map(|cell| cell.msb && cell.phase == Phase::Done)
                .unwrap_or(false)
        }) {
            return Err("gap-plan arithmetic did not finish".to_string());
        }
        Ok(())
    }

    pub fn next_residue(&self) -> Result<usize, String> {
        let low = PlanCell::decode(
            *self
                .config
                .get(&(0, 0))
                .ok_or_else(|| "gap plan lost its low residue bit".to_string())?,
        )
        .ok_or_else(|| "gap plan low residue state is invalid".to_string())?
        .sum;
        let high = PlanCell::decode(
            *self
                .config
                .get(&(1, 0))
                .ok_or_else(|| "gap plan lost its high residue bit".to_string())?,
        )
        .ok_or_else(|| "gap plan high residue state is invalid".to_string())?
        .sum;
        Ok(low as usize + 2 * high as usize)
    }

    pub fn split_count(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 2..self.width {
            let cell = PlanCell::decode(
                *self
                    .config
                    .get(&(bit as i32, 0))
                    .ok_or_else(|| format!("gap plan lost split bit {}", bit - 2))?,
            )
            .ok_or_else(|| "gap plan split state is invalid".to_string())?;
            value |= (cell.sum as usize) << (bit - 2);
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    #[test]
    fn local_gap_plan_matches_weight_four_mass_decomposition() {
        for gap in 0..=500 {
            for residue in 0..4 {
                let mut plan = GapPlan::new(gap, residue, 16).unwrap();
                let active = plan.config.len();
                plan.run_until_done().unwrap();
                assert_eq!(plan.config.len(), active);
                assert_eq!(plan.next_residue().unwrap(), (gap + residue) % 4);
                assert_eq!(plan.split_count().unwrap(), (gap + residue) / 4);
            }
        }
    }

    #[test]
    fn gap_plan_compiles_to_constant_weight_binary_blocks() {
        let plan = GapPlan::new(30, 1, 8).unwrap();
        let machine = BlockMachine::new(GapPlanRule).unwrap();
        let binary = machine.codec.encode(&plan.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&plan.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }
}
