use crate::construct::{BlockCodec, LocalRule, LogicalConfig};
use crate::firing_squad::{
    FiringSquadRule, FIRE, FIRING_ERROR, FIRING_STATE_COUNT, GENERAL, SOLDIER,
};

const SOUTH: usize = 1;
const SOUTH_EAST: usize = 2;
const WEST: usize = 3;
const EAST: usize = 4;
const NORTH_WEST: usize = 5;
const NORTH: usize = 6;
const NORTH_EAST: usize = 7;

const MASK_AVAILABLE: usize = 1;
const MASK_ARMED: usize = 2;
const MASK_COLLISION: usize = 3;
const COUNTER_BASE: usize = 4;
const COUNTER_FLAGS: usize = 1 << 3;
const INPUT_ZERO: usize = COUNTER_BASE + 16 * COUNTER_FLAGS;
const INPUT_ONE: usize = INPUT_ZERO + 1;
const MASK_WRITER_STATE_COUNT: usize = INPUT_ONE + 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Tape,
    DecrementNorth { borrow: bool, nonzero: bool },
    ReturnSouth { zero: bool },
    ShiftOld,
    Shifted,
    ReadySouth,
    Done,
    Load,
    CheckNorth { nonzero: bool },
    CheckReturn { zero: bool },
}

impl Phase {
    fn code(self) -> usize {
        match self {
            Phase::Tape => 0,
            Phase::DecrementNorth {
                borrow: false,
                nonzero: false,
            } => 1,
            Phase::DecrementNorth {
                borrow: false,
                nonzero: true,
            } => 2,
            Phase::DecrementNorth {
                borrow: true,
                nonzero: false,
            } => 3,
            Phase::DecrementNorth {
                borrow: true,
                nonzero: true,
            } => 4,
            Phase::ReturnSouth { zero: false } => 5,
            Phase::ReturnSouth { zero: true } => 6,
            Phase::ShiftOld => 7,
            Phase::Shifted => 8,
            Phase::ReadySouth => 9,
            Phase::Done => 10,
            Phase::Load => 11,
            Phase::CheckNorth { nonzero: false } => 12,
            Phase::CheckNorth { nonzero: true } => 13,
            Phase::CheckReturn { zero: false } => 14,
            Phase::CheckReturn { zero: true } => 15,
        }
    }

    fn decode(code: usize) -> Option<Self> {
        Some(match code {
            0 => Phase::Tape,
            1 => Phase::DecrementNorth {
                borrow: false,
                nonzero: false,
            },
            2 => Phase::DecrementNorth {
                borrow: false,
                nonzero: true,
            },
            3 => Phase::DecrementNorth {
                borrow: true,
                nonzero: false,
            },
            4 => Phase::DecrementNorth {
                borrow: true,
                nonzero: true,
            },
            5 => Phase::ReturnSouth { zero: false },
            6 => Phase::ReturnSouth { zero: true },
            7 => Phase::ShiftOld,
            8 => Phase::Shifted,
            9 => Phase::ReadySouth,
            10 => Phase::Done,
            11 => Phase::Load,
            12 => Phase::CheckNorth { nonzero: false },
            13 => Phase::CheckNorth { nonzero: true },
            14 => Phase::CheckReturn { zero: false },
            15 => Phase::CheckReturn { zero: true },
            _ => return None,
        })
    }

    const COUNT: usize = 16;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CounterCell {
    bit: bool,
    lsb: bool,
    msb: bool,
    phase: Phase,
}

impl CounterCell {
    fn encode(self) -> usize {
        COUNTER_BASE
            + self.phase.code() * COUNTER_FLAGS
            + self.bit as usize
            + 2 * self.lsb as usize
            + 4 * self.msb as usize
    }

    fn decode(state: usize) -> Option<Self> {
        let raw = state.checked_sub(COUNTER_BASE)?;
        if raw >= Phase::COUNT * COUNTER_FLAGS {
            return None;
        }
        let flags = raw % COUNTER_FLAGS;
        Some(CounterCell {
            bit: flags & 1 != 0,
            lsb: flags & 2 != 0,
            msb: flags & 4 != 0,
            phase: Phase::decode(raw / COUNTER_FLAGS)?,
        })
    }
}

fn decrement(bit: bool, borrow: bool) -> (bool, bool) {
    match (bit, borrow) {
        (false, true) => (true, true),
        (true, true) => (false, false),
        _ => (bit, false),
    }
}

fn decrement_output(cell: CounterCell, borrow: bool, nonzero: bool) -> (bool, bool, bool) {
    let (bit, borrow) = decrement(cell.bit, borrow);
    (bit, borrow, nonzero || bit)
}

pub struct SplitMaskWriterRule;

impl LocalRule for SplitMaskWriterRule {
    fn state_count(&self) -> usize {
        MASK_WRITER_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        match center {
            MASK_AVAILABLE => {
                if CounterCell::decode(neighbours[NORTH])
                    .map(|cell| cell.lsb && matches!(cell.phase, Phase::ReturnSouth { .. }))
                    .unwrap_or(false)
                {
                    MASK_ARMED
                } else {
                    MASK_AVAILABLE
                }
            }
            MASK_ARMED => MASK_ARMED,
            MASK_COLLISION => MASK_COLLISION,
            INPUT_ZERO | INPUT_ONE => center,
            0 => {
                let Some(west) = CounterCell::decode(neighbours[WEST]) else {
                    return 0;
                };
                if west.phase != Phase::ShiftOld {
                    return 0;
                }
                CounterCell {
                    phase: Phase::Shifted,
                    ..west
                }
                .encode()
            }
            _ => {
                let Some(mut cell) = CounterCell::decode(center) else {
                    return MASK_COLLISION;
                };
                match cell.phase {
                    Phase::Tape => {
                        if let Some(south) = CounterCell::decode(neighbours[SOUTH]) {
                            if let Phase::CheckNorth { nonzero } = south.phase {
                                cell.phase = Phase::CheckNorth {
                                    nonzero: nonzero || cell.bit,
                                };
                                return cell.encode();
                            }
                        }
                        if let Some(south) = CounterCell::decode(neighbours[SOUTH]) {
                            if let Phase::DecrementNorth { borrow, nonzero } = south.phase {
                                let (_, borrow, nonzero) = decrement_output(south, borrow, nonzero);
                                cell.phase = Phase::DecrementNorth { borrow, nonzero };
                                return cell.encode();
                            }
                        }
                        if let Some(north) = CounterCell::decode(neighbours[NORTH]) {
                            if let Phase::CheckReturn { zero } = north.phase {
                                cell.phase = Phase::CheckReturn { zero };
                                return cell.encode();
                            }
                            if let Phase::ReturnSouth { zero } = north.phase {
                                cell.phase = Phase::ReturnSouth { zero };
                                return cell.encode();
                            }
                            if north.phase == Phase::ReadySouth {
                                cell.phase = Phase::ReadySouth;
                                return cell.encode();
                            }
                        }
                        if CounterCell::decode(neighbours[SOUTH_EAST])
                            .map(|south_east| south_east.phase == Phase::Shifted)
                            .unwrap_or(false)
                        {
                            cell.phase = Phase::ShiftOld;
                        }
                        cell.encode()
                    }
                    Phase::DecrementNorth { borrow, nonzero } => {
                        let (bit, _, nonzero) = decrement_output(cell, borrow, nonzero);
                        cell.bit = bit;
                        cell.phase = if cell.msb {
                            Phase::ReturnSouth { zero: !nonzero }
                        } else {
                            Phase::Tape
                        };
                        cell.encode()
                    }
                    Phase::ReturnSouth { zero } => {
                        if cell.lsb {
                            cell.phase = if zero { Phase::Done } else { Phase::ShiftOld };
                        } else {
                            cell.phase = Phase::Tape;
                        }
                        cell.encode()
                    }
                    Phase::ShiftOld => 0,
                    Phase::Shifted => {
                        cell.phase = if cell.msb {
                            Phase::ReadySouth
                        } else {
                            Phase::Tape
                        };
                        cell.encode()
                    }
                    Phase::ReadySouth => {
                        cell.phase = if cell.lsb {
                            Phase::DecrementNorth {
                                borrow: true,
                                nonzero: false,
                            }
                        } else {
                            Phase::Tape
                        };
                        cell.encode()
                    }
                    Phase::Done => cell.encode(),
                    Phase::Load => {
                        cell.bit = match neighbours[WEST] {
                            INPUT_ZERO => false,
                            INPUT_ONE => true,
                            _ => return MASK_COLLISION,
                        };
                        cell.phase = if cell.lsb {
                            Phase::CheckNorth { nonzero: cell.bit }
                        } else {
                            Phase::Tape
                        };
                        cell.encode()
                    }
                    Phase::CheckNorth { nonzero } => {
                        cell.phase = if cell.msb {
                            Phase::CheckReturn { zero: !nonzero }
                        } else {
                            Phase::Tape
                        };
                        cell.encode()
                    }
                    Phase::CheckReturn { zero } => {
                        if cell.lsb {
                            cell.phase = if zero {
                                Phase::Done
                            } else {
                                Phase::DecrementNorth {
                                    borrow: true,
                                    nonzero: false,
                                }
                            };
                        } else {
                            cell.phase = Phase::Tape;
                        }
                        cell.encode()
                    }
                }
            }
        }
    }
}

pub struct SynchronizedMaskRule;

impl SynchronizedMaskRule {
    fn firing_offset() -> usize {
        SplitMaskWriterRule.state_count()
    }

    fn encode_firing(state: usize) -> usize {
        Self::firing_offset() + state
    }

    fn decode_firing(state: usize) -> Option<usize> {
        let state = state.checked_sub(Self::firing_offset())?;
        (state > 0 && state < FIRING_STATE_COUNT).then_some(state)
    }

    fn counter_done(state: usize) -> bool {
        CounterCell::decode(state)
            .map(|cell| cell.lsb && cell.phase == Phase::Done)
            .unwrap_or(false)
    }

    fn writer_neighbour(state: usize) -> usize {
        if Self::decode_firing(state).is_some() {
            MASK_ARMED
        } else {
            state
        }
    }
}

impl LocalRule for SynchronizedMaskRule {
    fn state_count(&self) -> usize {
        Self::firing_offset() + FIRING_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == MASK_AVAILABLE {
            if CounterCell::decode(neighbours[NORTH])
                .map(|cell| cell.lsb && matches!(cell.phase, Phase::ReturnSouth { .. }))
                .unwrap_or(false)
            {
                return Self::encode_firing(SOLDIER);
            }
            return MASK_AVAILABLE;
        }
        if center == MASK_ARMED {
            return Self::encode_firing(SOLDIER);
        }
        if let Some(firing) = Self::decode_firing(center) {
            if firing == FIRING_ERROR || firing == FIRE {
                return center;
            }
            if firing == SOLDIER && Self::counter_done(neighbours[NORTH]) {
                return Self::encode_firing(GENERAL);
            }
            if firing == SOLDIER
                && Self::decode_firing(neighbours[EAST]).is_none()
                && Self::counter_done(neighbours[NORTH_EAST])
            {
                return Self::encode_firing(GENERAL);
            }

            let mut local = [0; 8];
            for slot in 0..8 {
                local[slot] = Self::decode_firing(neighbours[slot]).unwrap_or(0);
            }
            if firing == SOLDIER
                && matches!(local[WEST], 0 | SOLDIER)
                && matches!(local[EAST], 0 | SOLDIER)
            {
                return Self::encode_firing(SOLDIER);
            }
            return Self::encode_firing(FiringSquadRule.next(firing, local));
        }

        let mut local = [0; 8];
        for slot in 0..8 {
            local[slot] = Self::writer_neighbour(neighbours[slot]);
        }
        SplitMaskWriterRule.next(center, local)
    }
}

/// One-shot exact split consumer for a synchronized mask.
///
/// Every firing parent returns to an ordinary reservoir state and births one ordinary child
/// immediately to its south. All other writer, counter, and firing-squad states remain delegated
/// to `SynchronizedMaskRule`.
pub struct SynchronizedSplitRule;

impl LocalRule for SynchronizedSplitRule {
    fn state_count(&self) -> usize {
        SynchronizedMaskRule.state_count()
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 && SynchronizedMaskRule::decode_firing(neighbours[NORTH]) == Some(FIRE) {
            return MASK_AVAILABLE;
        }
        if SynchronizedMaskRule::decode_firing(center) == Some(FIRE) {
            return MASK_AVAILABLE;
        }
        SynchronizedMaskRule.next(center, neighbours)
    }
}

pub struct SynchronizedPrimeOutputRule {
    request_base: usize,
    tail_base: usize,
}

impl SynchronizedPrimeOutputRule {
    pub fn new() -> Self {
        let request_base = SynchronizedMaskRule.state_count();
        SynchronizedPrimeOutputRule {
            request_base,
            tail_base: request_base + 4,
        }
    }

    pub fn ordinary_state_count(&self) -> usize {
        self.tail_base
    }

    pub fn request(&self, residue: usize) -> Result<usize, String> {
        if residue >= 4 {
            return Err("synchronized tail request is outside the residue range".to_string());
        }
        Ok(self.request_base + residue)
    }

    pub fn tail(&self, residue: usize) -> Result<usize, String> {
        if residue >= 4 {
            return Err("synchronized tail state is outside the residue range".to_string());
        }
        Ok(self.tail_base + residue)
    }

    fn request_residue(&self, state: usize) -> Option<usize> {
        (self.request_base..self.tail_base)
            .contains(&state)
            .then_some(state - self.request_base)
    }

    fn tail_residue(&self, state: usize) -> Option<usize> {
        (self.tail_base..self.tail_base + 4)
            .contains(&state)
            .then_some(state - self.tail_base)
    }
}

impl Default for SynchronizedPrimeOutputRule {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalRule for SynchronizedPrimeOutputRule {
    fn state_count(&self) -> usize {
        self.tail_base + 4
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 {
            if SynchronizedMaskRule::decode_firing(neighbours[NORTH]) == Some(FIRE)
                && self.tail_residue(neighbours[NORTH_WEST]).is_none()
            {
                return MASK_AVAILABLE;
            }
            let mut local = [0; 8];
            for slot in 0..8 {
                if neighbours[slot] < self.request_base {
                    local[slot] = neighbours[slot];
                }
            }
            return SynchronizedMaskRule.next(0, local);
        }
        if center < self.request_base {
            if SynchronizedMaskRule::decode_firing(center) == Some(FIRE) {
                return MASK_AVAILABLE;
            }
            let mut local = [0; 8];
            for slot in 0..8 {
                if neighbours[slot] < self.request_base {
                    local[slot] = neighbours[slot];
                }
            }
            return SynchronizedMaskRule.next(center, local);
        }
        if self.request_residue(center).is_some() {
            return if SynchronizedMaskRule::decode_firing(neighbours[NORTH_EAST]) == Some(FIRE) {
                MASK_AVAILABLE
            } else {
                center
            };
        }
        if let Some(current) = self.tail_residue(center) {
            if SynchronizedMaskRule::decode_firing(neighbours[EAST]) == Some(FIRE) {
                if let Some(requested) = self.request_residue(neighbours[SOUTH]) {
                    return self.tail_base + requested;
                }
            }
            return self.tail_base + current;
        }
        0
    }
}

pub struct SplitMaskWriter {
    pub config: LogicalConfig,
    pub steps: usize,
    width: usize,
    capacity: usize,
}

pub struct SynchronizedPrimeOutput {
    pub config: LogicalConfig,
    pub steps: usize,
    rule: SynchronizedPrimeOutputRule,
}

impl SplitMaskWriter {
    pub fn new(value: usize, width: usize, capacity: usize) -> Result<Self, String> {
        if width == 0 || width > usize::BITS as usize {
            return Err("mask writer width must be between 1 and usize::BITS".to_string());
        }
        if capacity < value {
            return Err("mask writer capacity is smaller than its requested mask".to_string());
        }
        if width < usize::BITS as usize && value >= 1usize << width {
            return Err("mask writer value does not fit its counter".to_string());
        }
        let mut config = LogicalConfig::new();
        for x in 0..capacity {
            let x = i32::try_from(x)
                .map_err(|_| "mask writer capacity does not fit i32".to_string())?;
            config.insert((x, -1), MASK_AVAILABLE);
        }
        for bit in 0..width {
            let y =
                i32::try_from(bit).map_err(|_| "mask writer width does not fit i32".to_string())?;
            config.insert(
                (0, y),
                CounterCell {
                    bit: value & (1usize << bit) != 0,
                    lsb: bit == 0,
                    msb: bit + 1 == width,
                    phase: if bit == 0 {
                        if value == 0 {
                            Phase::Done
                        } else {
                            Phase::DecrementNorth {
                                borrow: true,
                                nonzero: false,
                            }
                        }
                    } else {
                        Phase::Tape
                    },
                }
                .encode(),
            );
        }
        Ok(SplitMaskWriter {
            config,
            steps: 0,
            width,
            capacity,
        })
    }

    pub fn from_record(value: usize, width: usize, capacity: usize) -> Result<Self, String> {
        if width == 0 || width > usize::BITS as usize {
            return Err("mask writer width must be between 1 and usize::BITS".to_string());
        }
        if capacity < value {
            return Err("mask writer capacity is smaller than its requested mask".to_string());
        }
        if width < usize::BITS as usize && value >= 1usize << width {
            return Err("mask writer value does not fit its input record".to_string());
        }
        let mut config = LogicalConfig::new();
        for x in 0..capacity {
            let x = i32::try_from(x)
                .map_err(|_| "mask writer capacity does not fit i32".to_string())?;
            config.insert((x, -1), MASK_AVAILABLE);
        }
        for bit in 0..width {
            let y =
                i32::try_from(bit).map_err(|_| "mask writer width does not fit i32".to_string())?;
            config.insert(
                (-1, y),
                if value & (1usize << bit) != 0 {
                    INPUT_ONE
                } else {
                    INPUT_ZERO
                },
            );
            config.insert(
                (0, y),
                CounterCell {
                    bit: false,
                    lsb: bit == 0,
                    msb: bit + 1 == width,
                    phase: Phase::Load,
                }
                .encode(),
            );
        }
        Ok(SplitMaskWriter {
            config,
            steps: 0,
            width,
            capacity,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = SplitMaskWriterRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn done(&self) -> bool {
        self.config.values().any(|state| {
            CounterCell::decode(*state)
                .map(|cell| cell.lsb && cell.phase == Phase::Done)
                .unwrap_or(false)
        })
    }

    pub fn run_until_done(&mut self, max_steps: usize) -> Result<(), String> {
        while !self.done() && self.steps < max_steps {
            self.step()?;
        }
        if !self.done() {
            return Err(format!(
                "mask writer did not finish in {} local steps",
                max_steps
            ));
        }
        Ok(())
    }

    pub fn armed(&self) -> Vec<usize> {
        (0..self.capacity)
            .filter(|x| self.config.get(&(*x as i32, -1)) == Some(&MASK_ARMED))
            .collect()
    }

    pub fn counter_cells(&self) -> usize {
        self.config
            .values()
            .filter(|state| CounterCell::decode(**state).is_some())
            .count()
    }

    pub fn width(&self) -> usize {
        self.width
    }
}

impl SynchronizedPrimeOutput {
    pub fn new(
        splits: usize,
        width: usize,
        capacity: usize,
        old_residue: usize,
        new_residue: usize,
    ) -> Result<Self, String> {
        let rule = SynchronizedPrimeOutputRule::new();
        let mut config = SplitMaskWriter::from_record(splits, width, capacity)?.config;
        config.insert((-1, -1), SynchronizedMaskRule::encode_firing(SOLDIER));
        config.insert((-2, -1), rule.tail(old_residue)?);
        config.insert((-2, -2), rule.request(new_residue)?);
        Ok(SynchronizedPrimeOutput {
            config,
            steps: 0,
            rule,
        })
    }

    pub fn ready(&self) -> bool {
        self.config
            .values()
            .any(|state| SynchronizedMaskRule::decode_firing(*state) == Some(FIRE))
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = self.rule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn run_until_ready(&mut self, max_steps: usize) -> Result<(), String> {
        while !self.ready() && self.steps < max_steps {
            self.step()?;
        }
        if !self.ready() {
            return Err(format!(
                "synchronized prime output did not become ready in {} steps",
                max_steps
            ));
        }
        Ok(())
    }

    pub fn binary_weight(&self) -> Result<usize, String> {
        let codec = BlockCodec::with_tail_states(self.rule.ordinary_state_count(), 4)?;
        codec.logical_weight(&self.config)
    }

    pub fn commit(&mut self) -> Result<(), String> {
        if !self.ready() {
            return Err("synchronized prime output is not ready to commit".to_string());
        }
        self.step()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    #[test]
    fn moving_counter_arms_exact_prefix_without_changing_population() {
        for value in 0..=64 {
            let width = 8;
            let mut writer = SplitMaskWriter::new(value, width, 70).unwrap();
            let active = writer.config.len();
            while !writer.done() {
                writer.step().unwrap();
                assert_eq!(writer.config.len(), active, "value {value}");
                assert_eq!(writer.counter_cells(), width, "value {value}");
            }
            assert_eq!(writer.armed(), (0..value).collect::<Vec<_>>());
            assert!(
                writer.steps <= value * 8 * width + 1,
                "value {value} took {} steps",
                writer.steps
            );
        }
    }

    #[test]
    fn binary_record_loads_locally_before_mask_generation() {
        for value in 0..=64 {
            let mut writer = SplitMaskWriter::from_record(value, 8, 70).unwrap();
            let active = writer.config.len();
            writer
                .run_until_done(value * 10 * writer.width() + 40)
                .unwrap();
            assert_eq!(writer.config.len(), active);
            assert_eq!(writer.armed(), (0..value).collect::<Vec<_>>());
        }
    }

    #[test]
    fn moving_counter_rule_compiles_to_constant_weight_binary_blocks() {
        let writer = SplitMaskWriter::new(7, 4, 10).unwrap();
        let machine = BlockMachine::new(SplitMaskWriterRule).unwrap();
        let binary = machine.codec.encode(&writer.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            SplitMaskWriterRule.step(&writer.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn written_prefix_fires_simultaneously_after_local_countdown() {
        for value in 1..=64 {
            let width = 8;
            let writer = SplitMaskWriter::new(value, width, 70).unwrap();
            let active = writer.config.len();
            let mut config = writer.config;
            let mut fired = false;
            for step in 0..value * 10 * width + 2 * value + 20 {
                let firing = config
                    .iter()
                    .filter_map(|(point, state)| {
                        (SynchronizedMaskRule::decode_firing(*state) == Some(FIRE))
                            .then_some(point.0)
                    })
                    .collect::<Vec<_>>();
                assert!(
                    config.values().all(|state| {
                        SynchronizedMaskRule::decode_firing(*state) != Some(FIRING_ERROR)
                    }),
                    "value {value} reached an undefined firing transition at {step}"
                );
                if !firing.is_empty() {
                    assert_eq!(firing, (0..value as i32).collect::<Vec<_>>());
                    fired = true;
                    break;
                }
                config = SynchronizedMaskRule.step(&config).unwrap();
                assert_eq!(config.len(), active, "value {value}");
            }
            assert!(fired, "value {value} never fired");
        }
    }

    #[test]
    fn synchronized_mask_births_exactly_one_child_per_firing_parent() {
        for value in 1..=32 {
            let writer = SplitMaskWriter::new(value, 8, 40).unwrap();
            let mut config = writer.config;
            while config
                .values()
                .all(|state| SynchronizedMaskRule::decode_firing(*state) != Some(FIRE))
            {
                config = SynchronizedSplitRule.step(&config).unwrap();
            }
            let before = config.len();
            let next = SynchronizedSplitRule.step(&config).unwrap();
            assert_eq!(next.len() - before, value);
            for x in 0..value as i32 {
                assert_eq!(next.get(&(x, -1)), Some(&MASK_AVAILABLE));
                assert_eq!(next.get(&(x, -2)), Some(&MASK_AVAILABLE));
            }
        }

        let writer = SplitMaskWriter::new(7, 4, 10).unwrap();
        let mut logical = writer.config;
        while logical
            .values()
            .all(|state| SynchronizedMaskRule::decode_firing(*state) != Some(FIRE))
        {
            logical = SynchronizedSplitRule.step(&logical).unwrap();
        }
        let machine = BlockMachine::new(SynchronizedSplitRule).unwrap();
        let binary = machine.codec.encode(&logical).unwrap();
        let direct = machine.step_binary_local(&binary).unwrap();
        assert_eq!(direct, machine.step_binary(&binary).unwrap());
        assert_eq!(
            direct.len() - binary.len(),
            7 * machine.codec.active_weight()
        );
    }

    #[test]
    fn synchronized_prime_output_applies_splits_and_tail_delta_together() {
        for splits in 0..=16 {
            let old_residue = splits % 4;
            let new_residue = (3 * splits + 1) % 4;
            let rule = SynchronizedPrimeOutputRule::new();
            let mut logical = SplitMaskWriter::from_record(splits, 6, 20).unwrap().config;
            logical.insert((-1, -1), SynchronizedMaskRule::encode_firing(SOLDIER));
            logical.insert((-2, -1), rule.tail(old_residue).unwrap());
            logical.insert((-2, -2), rule.request(new_residue).unwrap());
            let active = logical.len();
            while logical
                .values()
                .all(|state| SynchronizedMaskRule::decode_firing(*state) != Some(FIRE))
            {
                assert_eq!(
                    logical.get(&(-2, -1)),
                    Some(&rule.tail(old_residue).unwrap())
                );
                logical = rule.step(&logical).unwrap();
                assert_eq!(logical.len(), active);
            }

            let codec =
                crate::construct::BlockCodec::with_tail_states(rule.ordinary_state_count(), 4)
                    .unwrap();
            let machine = BlockMachine::with_codec(rule, codec).unwrap();
            let before = machine.codec.logical_weight(&logical).unwrap();
            let binary = machine.codec.encode(&logical).unwrap();
            let next = machine.rule.step(&logical).unwrap();
            let after = machine.codec.logical_weight(&next).unwrap();
            assert_eq!(next.len() - logical.len(), splits);
            assert_eq!(
                after as isize - before as isize,
                (4 * splits) as isize + new_residue as isize - old_residue as isize
            );
            assert_eq!(
                next.get(&(-2, -1)),
                Some(&machine.rule.tail(new_residue).unwrap())
            );
            assert!(!next.contains_key(&(-1, -2)));
            for x in 0..splits as i32 {
                assert_eq!(next.get(&(x, -2)), Some(&MASK_AVAILABLE));
            }
            let direct = machine.step_binary_local(&binary).unwrap();
            assert_eq!(direct, machine.step_binary(&binary).unwrap());
            assert_eq!(direct.len(), after);
        }
    }

    #[test]
    fn mask_writer_rejects_invalid_capacity_and_width() {
        assert!(SplitMaskWriter::new(8, 3, 8).is_err());
        assert!(SplitMaskWriter::new(8, 4, 7).is_err());
        assert!(SplitMaskWriter::new(1, 0, 1).is_err());
    }
}
