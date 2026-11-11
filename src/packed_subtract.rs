use crate::construct::{LocalRule, LogicalConfig};

const NORTH: usize = 6;
const EAST: usize = 4;
const SOUTH: usize = 1;
const NORTH_EAST: usize = 7;
const SOUTH_EAST: usize = 2;
const PHASE_COUNT: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Wait,
    Subtract(bool),
    Settled,
    Done,
}

impl Phase {
    fn code(self) -> usize {
        match self {
            Phase::Wait => 0,
            Phase::Subtract(false) => 1,
            Phase::Subtract(true) => 2,
            Phase::Settled => 3,
            Phase::Done => 4,
        }
    }

    fn decode(code: usize) -> Option<Self> {
        Some(match code {
            0 => Phase::Wait,
            1 => Phase::Subtract(false),
            2 => Phase::Subtract(true),
            3 => Phase::Settled,
            4 => Phase::Done,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy)]
struct ResultCell {
    payload: usize,
    leftmost: bool,
    phase: Phase,
}

pub struct PackedChunkSubtractRule {
    payload_states: usize,
}

impl PackedChunkSubtractRule {
    pub fn new(payload_bits: usize) -> Result<Self, String> {
        if !(1..=20).contains(&payload_bits) {
            return Err("packed subtraction width must be between 1 and 20 bits".to_string());
        }
        Ok(PackedChunkSubtractRule {
            payload_states: 1usize << payload_bits,
        })
    }

    fn input(&self, payload: usize) -> Result<usize, String> {
        if payload >= self.payload_states {
            return Err("packed subtraction input exceeds its chunk width".to_string());
        }
        Ok(1 + payload)
    }

    fn decode_input(&self, state: usize) -> Option<usize> {
        let payload = state.checked_sub(1)?;
        (payload < self.payload_states).then_some(payload)
    }

    fn result_base(&self) -> usize {
        1 + self.payload_states
    }

    fn result(&self, cell: ResultCell) -> usize {
        self.result_base()
            + ((cell.phase.code() * 2 + cell.leftmost as usize) * self.payload_states)
            + cell.payload
    }

    fn decode_result(&self, state: usize) -> Option<ResultCell> {
        let raw = state.checked_sub(self.result_base())?;
        if raw >= PHASE_COUNT * 2 * self.payload_states {
            return None;
        }
        let payload = raw % self.payload_states;
        let flags = raw / self.payload_states;
        Some(ResultCell {
            payload,
            leftmost: flags & 1 != 0,
            phase: Phase::decode(flags / 2)?,
        })
    }

    fn subtract(&self, current: usize, previous: usize, borrow: bool) -> (usize, bool) {
        let subtrahend = previous + borrow as usize;
        if current >= subtrahend {
            (current - subtrahend, false)
        } else {
            (self.payload_states + current - subtrahend, true)
        }
    }
}

impl LocalRule for PackedChunkSubtractRule {
    fn state_count(&self) -> usize {
        self.result_base() + PHASE_COUNT * 2 * self.payload_states
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if self.decode_input(center).is_some() {
            return center;
        }
        let Some(mut cell) = self.decode_result(center) else {
            return 0;
        };
        if let Phase::Subtract(borrow) = cell.phase {
            let current = self.decode_input(neighbours[SOUTH]).unwrap_or(0);
            let previous = self.decode_input(neighbours[NORTH]).unwrap_or(0);
            let (payload, _) = self.subtract(current, previous, borrow);
            cell.payload = payload;
            cell.phase = if cell.leftmost {
                Phase::Done
            } else {
                Phase::Settled
            };
            return self.result(cell);
        }
        if cell.phase == Phase::Wait {
            if let Some(east) = self.decode_result(neighbours[EAST]) {
                if let Phase::Subtract(borrow) = east.phase {
                    let current = self.decode_input(neighbours[SOUTH_EAST]).unwrap_or(0);
                    let previous = self.decode_input(neighbours[NORTH_EAST]).unwrap_or(0);
                    let (_, borrow) = self.subtract(current, previous, borrow);
                    cell.phase = Phase::Subtract(borrow);
                }
            }
        }
        self.result(cell)
    }
}

pub struct PackedChunkSubtract {
    pub config: LogicalConfig,
    pub steps: usize,
    chunks: usize,
    rule: PackedChunkSubtractRule,
}

impl PackedChunkSubtract {
    pub fn new(current: &[usize], previous: &[usize], payload_bits: usize) -> Result<Self, String> {
        if current.is_empty() || current.len() != previous.len() {
            return Err("packed subtraction records must have equal positive lengths".to_string());
        }
        let rule = PackedChunkSubtractRule::new(payload_bits)?;
        let mut config = LogicalConfig::new();
        for index in 0..current.len() {
            let x = i32::try_from(index)
                .map_err(|_| "packed subtraction width does not fit i32".to_string())?;
            config.insert((x, 0), rule.input(current[index])?);
            config.insert((x, 2), rule.input(previous[index])?);
            config.insert(
                (x, 1),
                rule.result(ResultCell {
                    payload: 0,
                    leftmost: index == 0,
                    phase: if index + 1 == current.len() {
                        Phase::Subtract(false)
                    } else {
                        Phase::Wait
                    },
                }),
            );
        }
        Ok(PackedChunkSubtract {
            config,
            steps: 0,
            chunks: current.len(),
            rule,
        })
    }

    pub fn run_until_done(&mut self) -> Result<(), String> {
        for _ in 0..=self.chunks {
            self.config = self.rule.step(&self.config)?;
            self.steps += 1;
        }
        if self
            .config
            .get(&(0, 1))
            .and_then(|state| self.rule.decode_result(*state))
            .map(|cell| cell.phase == Phase::Done)
            != Some(true)
        {
            return Err("packed subtraction did not finish".to_string());
        }
        Ok(())
    }

    pub fn result(&self) -> Result<Vec<usize>, String> {
        (0..self.chunks)
            .map(|index| {
                self.config
                    .get(&(index as i32, 1))
                    .and_then(|state| self.rule.decode_result(*state))
                    .map(|cell| cell.payload)
                    .ok_or_else(|| format!("packed subtraction lost chunk {}", index))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    fn chunks(value: usize, width: usize, bits: usize) -> Vec<usize> {
        let mask = (1usize << bits) - 1;
        (0..width)
            .rev()
            .map(|chunk| (value >> (chunk * bits)) & mask)
            .collect()
    }

    fn value(chunks: &[usize], bits: usize) -> usize {
        chunks
            .iter()
            .fold(0usize, |value, chunk| (value << bits) | chunk)
    }

    #[test]
    fn packed_prime_records_subtract_locally_in_big_endian_order() {
        for (previous, current) in [(3, 5), (23, 29), (89, 97), (251, 257)] {
            let previous_chunks = chunks(previous, 3, 4);
            let current_chunks = chunks(current, 3, 4);
            let mut subtract =
                PackedChunkSubtract::new(&current_chunks, &previous_chunks, 4).unwrap();
            let active = subtract.config.len();
            subtract.run_until_done().unwrap();
            assert_eq!(subtract.config.len(), active);
            assert_eq!(value(&subtract.result().unwrap(), 4), current - previous);
        }
    }

    #[test]
    fn twenty_bit_chunk_subtractor_compiles_to_binary_blocks() {
        let subtract = PackedChunkSubtract::new(&[1, 29], &[1, 23], 20).unwrap();
        let machine = BlockMachine::new(PackedChunkSubtractRule::new(20).unwrap()).unwrap();
        assert!(machine.codec.width > 10_000_000);
        let binary = machine.codec.encode(&subtract.config).unwrap();
        assert_eq!(machine.codec.decode(&binary).unwrap(), subtract.config);
    }
}
