/*
 * Local ordered result queue
 * ==========================
 *
 * Candidate tiles write one finite result token into an adjacent FIFO slot.  The FIFO head
 * occupies the slot it is currently inspecting.  It advances exactly one slot only after the
 * slot is ready, and cannot skip a pending slot.  Every state is active, so movement and release
 * are population-neutral at the logical level.
 */

use crate::construct::{LocalRule, LogicalConfig};
use crate::prime_worker::{
    candidate_result_code, worker_rule_state_count, MeshCandidate, PrimeWorkerRule,
};

pub const SLOT_PENDING: usize = 1;
pub const SLOT_PRIME: usize = 2;
pub const SLOT_COMPOSITE: usize = 3;
pub const HEAD_PENDING: usize = 4;
pub const HEAD_PRIME: usize = 5;
pub const HEAD_COMPOSITE: usize = 6;
pub const RELEASE_PRIME: usize = 7;
pub const RELEASE_COMPOSITE: usize = 8;
pub const SLOT_CONSUMED: usize = 9;
pub const INPUT_PRIME: usize = 10;
pub const INPUT_COMPOSITE: usize = 11;

pub const FIFO_STATE_COUNT: usize = 12;

pub const GAP_BASE: usize = FIFO_STATE_COUNT;
pub const GAP_STATE_COUNT: usize = 8;
pub const TRAVEL_BASE: usize = GAP_BASE + GAP_STATE_COUNT;
pub const TRAVEL_STATE_COUNT: usize = 9;
pub const SUB_BASE: usize = TRAVEL_BASE + TRAVEL_STATE_COUNT;
pub const SUB_PHASES: usize = 4;
pub const SUB_PAYLOAD: usize = 1 << 5;
pub const SUB_STATE_COUNT: usize = 1 + SUB_PHASES * SUB_PAYLOAD;

fn encode_gap_bit(lsb: bool, bit: bool, pulse: bool) -> usize {
    GAP_BASE + (lsb as usize) + 2 * (bit as usize) + 4 * (pulse as usize)
}

fn decode_gap_bit(state: usize) -> Option<(bool, bool, bool)> {
    if !(GAP_BASE..GAP_BASE + GAP_STATE_COUNT).contains(&state) {
        return None;
    }
    let payload = state - GAP_BASE;
    Some((payload & 1 != 0, payload & 2 != 0, payload & 4 != 0))
}

fn encode_travel_bit(lsb: bool, bit: bool, pulse: bool) -> usize {
    TRAVEL_BASE + 1 + (lsb as usize) + 2 * (bit as usize) + 4 * (pulse as usize)
}

fn decode_travel_bit(state: usize) -> Option<(bool, bool, bool)> {
    if !(TRAVEL_BASE + 1..TRAVEL_BASE + TRAVEL_STATE_COUNT).contains(&state) {
        return None;
    }
    let payload = state - TRAVEL_BASE - 1;
    Some((payload & 1 != 0, payload & 2 != 0, payload & 4 != 0))
}

#[derive(Clone, Copy)]
struct SubCell {
    a: bool,
    b: bool,
    result: bool,
    lsb: bool,
    msb: bool,
    phase: usize,
}

impl SubCell {
    fn encode(self) -> usize {
        SUB_BASE
            + 1
            + self.phase * SUB_PAYLOAD
            + (self.a as usize)
            + ((self.b as usize) << 1)
            + ((self.result as usize) << 2)
            + ((self.lsb as usize) << 3)
            + ((self.msb as usize) << 4)
    }

    fn decode(state: usize) -> Option<Self> {
        if !(SUB_BASE + 1..SUB_BASE + SUB_STATE_COUNT).contains(&state) {
            return None;
        }
        let raw = state - SUB_BASE - 1;
        let payload = raw % SUB_PAYLOAD;
        Some(SubCell {
            a: payload & 1 != 0,
            b: payload & 2 != 0,
            result: payload & 4 != 0,
            lsb: payload & 8 != 0,
            msb: payload & 16 != 0,
            phase: raw / SUB_PAYLOAD,
        })
    }
}

fn subtract_bit(minuend: bool, subtrahend: bool, borrow: bool) -> (bool, bool) {
    let minuend = minuend as u8;
    let subtrahend = subtrahend as u8 + borrow as u8;
    if minuend >= subtrahend {
        ((minuend - subtrahend) == 1, false)
    } else {
        ((minuend + 2 - subtrahend) == 1, true)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FifoResult {
    Prime,
    Composite,
}

pub struct OrderedFifoRule;

fn is_slot(state: usize) -> bool {
    matches!(state, SLOT_PENDING | SLOT_PRIME | SLOT_COMPOSITE)
}

impl LocalRule for OrderedFifoRule {
    fn state_count(&self) -> usize {
        FIFO_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let west = neighbours[3];
        let east = neighbours[4];
        let south = neighbours[1];

        match center {
            0 => 0,
            SLOT_PENDING => match (west, south) {
                (HEAD_PRIME | HEAD_COMPOSITE, INPUT_PRIME) => HEAD_PRIME,
                (HEAD_PRIME | HEAD_COMPOSITE, INPUT_COMPOSITE) => HEAD_COMPOSITE,
                (HEAD_PRIME | HEAD_COMPOSITE, _) => HEAD_PENDING,
                (_, INPUT_PRIME) => SLOT_PRIME,
                (_, INPUT_COMPOSITE) => SLOT_COMPOSITE,
                _ => SLOT_PENDING,
            },
            SLOT_PRIME => match west {
                HEAD_PRIME | HEAD_COMPOSITE => HEAD_PRIME,
                _ => SLOT_PRIME,
            },
            SLOT_COMPOSITE => match west {
                HEAD_PRIME | HEAD_COMPOSITE => HEAD_COMPOSITE,
                _ => SLOT_COMPOSITE,
            },
            HEAD_PENDING => match south {
                INPUT_PRIME => HEAD_PRIME,
                INPUT_COMPOSITE => HEAD_COMPOSITE,
                _ => HEAD_PENDING,
            },
            HEAD_PRIME if is_slot(east) => RELEASE_PRIME,
            HEAD_COMPOSITE if is_slot(east) => RELEASE_COMPOSITE,
            HEAD_PRIME | HEAD_COMPOSITE => center,
            RELEASE_PRIME | RELEASE_COMPOSITE => SLOT_CONSUMED,
            SLOT_CONSUMED => SLOT_CONSUMED,
            INPUT_PRIME | INPUT_COMPOSITE => SLOT_CONSUMED,
            _ => 0,
        }
    }
}

pub struct OrderedFifo {
    pub config: LogicalConfig,
    pub released: Vec<FifoResult>,
}

/// Product alphabet for the autonomous arithmetic plane and the ordered result plane.
///
/// Worker states retain their original encoding.  FIFO states are tagged above them.  A
/// candidate result directly south of a FIFO slot is seen by that slot as an input token.
pub struct IntegratedSchedulerRule;

pub struct IntegratedCandidate {
    pub config: LogicalConfig,
    pub steps: usize,
    gap_origin: (i32, i32),
    gap_width: usize,
}

impl IntegratedSchedulerRule {
    pub fn encode_fifo(state: usize) -> usize {
        worker_rule_state_count() + state
    }

    pub fn decode_fifo(state: usize) -> Option<usize> {
        let offset = worker_rule_state_count();
        (state > offset && state < offset + FIFO_STATE_COUNT).then(|| state - offset)
    }

    pub fn encode_gap(state: usize) -> usize {
        worker_rule_state_count() + FIFO_STATE_COUNT + state
    }

    pub fn decode_gap(state: usize) -> Option<usize> {
        let offset = worker_rule_state_count() + FIFO_STATE_COUNT;
        (state > offset && state < offset + GAP_BASE + GAP_STATE_COUNT).then(|| state - offset)
    }
}

impl LocalRule for IntegratedSchedulerRule {
    fn state_count(&self) -> usize {
        worker_rule_state_count() + FIFO_STATE_COUNT + GAP_BASE + GAP_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let offset = worker_rule_state_count();
        if let Some(fifo_state) = Self::decode_fifo(center) {
            let mut local = [0; 8];
            for slot in 0..8 {
                let state = neighbours[slot];
                if let Some(tagged) = Self::decode_fifo(state) {
                    local[slot] = tagged;
                } else if slot == 1 {
                    local[slot] = match candidate_result_code(state) {
                        Some(false) => INPUT_PRIME,
                        Some(true) => INPUT_COMPOSITE,
                        None => 0,
                    };
                }
            }
            return offset + OrderedFifoRule.next(fifo_state, local);
        }
        if let Some(gap_state) = Self::decode_gap(center) {
            let mut local = [0; 8];
            for slot in 0..8 {
                let state = neighbours[slot];
                if let Some(tagged) = Self::decode_gap(state) {
                    local[slot] = tagged;
                } else if slot == 1 {
                    local[slot] = match Self::decode_fifo(state) {
                        Some(RELEASE_COMPOSITE) => INPUT_COMPOSITE,
                        Some(RELEASE_PRIME) => INPUT_PRIME,
                        _ => 0,
                    };
                }
            }
            return Self::encode_gap(GapCounterRule.next(gap_state, local));
        }

        let center = if center <= offset { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            if neighbours[slot] <= offset {
                local[slot] = neighbours[slot];
            }
        }
        PrimeWorkerRule.next(center, local)
    }
}

impl IntegratedCandidate {
    pub fn new(candidate: usize) -> Result<Self, String> {
        let mesh = MeshCandidate::new(candidate)?;
        let output = mesh.output;
        let mut config = mesh.config;
        config.insert(
            (output.0, output.1 + 1),
            IntegratedSchedulerRule::encode_fifo(HEAD_PENDING),
        );
        config.insert(
            (output.0 + 1, output.1 + 1),
            IntegratedSchedulerRule::encode_fifo(SLOT_PENDING),
        );
        let gap_origin = (output.0, output.1 + 2);
        let gap_width = 16;
        for bit in 0..gap_width {
            config.insert(
                (gap_origin.0 + bit as i32, gap_origin.1),
                IntegratedSchedulerRule::encode_gap(encode_gap_bit(bit == 0, false, false)),
            );
        }
        Ok(IntegratedCandidate {
            config,
            steps: 0,
            gap_origin,
            gap_width,
        })
    }

    pub fn step(&mut self) -> Result<Option<FifoResult>, String> {
        self.config = IntegratedSchedulerRule.step(&self.config)?;
        self.steps += 1;
        Ok(self.config.values().find_map(|&state| {
            match IntegratedSchedulerRule::decode_fifo(state)? {
                RELEASE_PRIME => Some(FifoResult::Prime),
                RELEASE_COMPOSITE => Some(FifoResult::Composite),
                _ => None,
            }
        }))
    }

    pub fn run_until_release(&mut self, max_steps: usize) -> Result<FifoResult, String> {
        while self.steps < max_steps {
            if let Some(result) = self.step()? {
                return Ok(result);
            }
        }
        Err(format!(
            "integrated candidate did not release in {} local steps",
            max_steps
        ))
    }

    pub fn settle_gap(&mut self) -> Result<(), String> {
        for _ in 0..self.gap_width {
            self.config = IntegratedSchedulerRule.step(&self.config)?;
            self.steps += 1;
        }
        Ok(())
    }

    pub fn gap_value(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 0..self.gap_width {
            let point = (self.gap_origin.0 + bit as i32, self.gap_origin.1);
            let state = self
                .config
                .get(&point)
                .ok_or_else(|| format!("integrated gap counter lost bit {}", bit))?;
            let raw = IntegratedSchedulerRule::decode_gap(*state)
                .ok_or_else(|| "integrated gap counter has an invalid state".to_string())?;
            let (_, set, _) = decode_gap_bit(raw)
                .ok_or_else(|| "integrated gap bit cannot be decoded".to_string())?;
            value |= (set as usize) << bit;
        }
        Ok(value)
    }
}

pub struct GapCounterRule;

impl LocalRule for GapCounterRule {
    fn state_count(&self) -> usize {
        GAP_BASE + GAP_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if let Some((lsb, bit, _pulse)) = decode_gap_bit(center) {
            let carry_in = if lsb {
                matches!(neighbours[1], INPUT_COMPOSITE | INPUT_PRIME)
            } else {
                neighbours[3]
                    .checked_sub(0)
                    .and_then(decode_gap_bit)
                    .map(|(_, _, pulse)| pulse)
                    .unwrap_or(false)
            };
            let next_bit = bit ^ carry_in;
            let carry_out = bit && carry_in;
            return encode_gap_bit(lsb, next_bit, carry_out);
        }
        0
    }
}

pub struct GapCounter {
    pub config: LogicalConfig,
    width: usize,
}

pub struct TravelingCounterRule;

impl LocalRule for TravelingCounterRule {
    fn state_count(&self) -> usize {
        TRAVEL_BASE + TRAVEL_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if decode_travel_bit(center).is_some() {
            return TRAVEL_BASE;
        }
        if center != TRAVEL_BASE {
            return 0;
        }

        let Some((lsb, bit, _)) = decode_travel_bit(neighbours[3]) else {
            return TRAVEL_BASE;
        };
        let carry_in = if lsb {
            matches!(neighbours[0], INPUT_COMPOSITE | INPUT_PRIME)
        } else {
            decode_travel_bit(neighbours[0])
                .map(|(_, _, pulse)| pulse)
                .unwrap_or(false)
        };
        encode_travel_bit(lsb, bit ^ carry_in, bit && carry_in)
    }
}

pub struct BinarySubtractRule;

impl LocalRule for BinarySubtractRule {
    fn state_count(&self) -> usize {
        SUB_BASE + SUB_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let Some(mut cell) = SubCell::decode(center) else {
            return 0;
        };
        if cell.phase == 1 || cell.phase == 2 {
            let (difference, _) = subtract_bit(cell.a, cell.b, cell.phase == 2);
            cell.result = difference;
            cell.phase = if cell.msb { 3 } else { 0 };
            return cell.encode();
        }
        if let Some(west) = SubCell::decode(neighbours[3]) {
            if west.phase == 1 || west.phase == 2 {
                let (_, borrow) = subtract_bit(west.a, west.b, west.phase == 2);
                cell.phase = if borrow { 2 } else { 1 };
                return cell.encode();
            }
        }
        cell.encode()
    }
}

pub struct BinarySubtract {
    pub config: LogicalConfig,
    width: usize,
}

impl BinarySubtract {
    pub fn new(minuend: usize, subtrahend: usize, width: usize) -> Result<Self, String> {
        if minuend < subtrahend {
            return Err("the construction subtractor only handles nonnegative gaps".to_string());
        }
        if width == 0 || width > usize::BITS as usize {
            return Err("subtractor width must be between 1 and usize::BITS".to_string());
        }
        if width < usize::BITS as usize && minuend >= (1usize << width) {
            return Err("subtractor input does not fit its tape".to_string());
        }
        let mut config = LogicalConfig::new();
        for bit in 0..width {
            let x =
                i32::try_from(bit).map_err(|_| "subtractor width does not fit i32".to_string())?;
            config.insert(
                (x, 0),
                SubCell {
                    a: minuend & (1usize << bit) != 0,
                    b: subtrahend & (1usize << bit) != 0,
                    result: false,
                    lsb: bit == 0,
                    msb: bit + 1 == width,
                    phase: if bit == 0 { 1 } else { 0 },
                }
                .encode(),
            );
        }
        Ok(BinarySubtract { config, width })
    }

    pub fn run_until_done(&mut self) -> Result<(), String> {
        for _ in 0..=self.width {
            self.config = BinarySubtractRule.step(&self.config)?;
        }
        if !self.config.values().any(|state| {
            SubCell::decode(*state)
                .map(|cell| cell.msb && cell.phase == 3)
                .unwrap_or(false)
        }) {
            return Err("binary subtractor did not reach its done state".to_string());
        }
        Ok(())
    }

    pub fn value(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 0..self.width {
            let state = self
                .config
                .get(&(bit as i32, 0))
                .ok_or_else(|| format!("subtractor lost bit {}", bit))?;
            let cell =
                SubCell::decode(*state).ok_or_else(|| "invalid subtractor state".to_string())?;
            value |= (cell.result as usize) << bit;
        }
        Ok(value)
    }
}

pub struct TravelingGapCounter {
    pub config: LogicalConfig,
    width: usize,
    capacity: i32,
    head_x: i32,
}

impl TravelingGapCounter {
    pub fn new(width: usize, capacity: usize) -> Result<Self, String> {
        if width == 0 || width > usize::BITS as usize || capacity <= width {
            return Err(
                "a traveling counter needs positive width and capacity larger than its width"
                    .to_string(),
            );
        }
        let mut config = LogicalConfig::new();
        for x in 0..capacity {
            let x = i32::try_from(x)
                .map_err(|_| "traveling counter capacity does not fit i32".to_string())?;
            for y in 0..width {
                let y = i32::try_from(y)
                    .map_err(|_| "traveling counter width does not fit i32".to_string())?;
                config.insert(
                    (x, y),
                    if x == 0 {
                        encode_travel_bit(y == 0, false, false)
                    } else {
                        TRAVEL_BASE
                    },
                );
            }
        }
        Ok(TravelingGapCounter {
            config,
            width,
            capacity: i32::try_from(capacity)
                .map_err(|_| "traveling counter capacity does not fit i32".to_string())?,
            head_x: 0,
        })
    }

    pub fn advance(&mut self, prime: bool) -> Result<(), String> {
        if self.head_x >= self.capacity - 1 {
            return Err("traveling counter reached its preallocated capacity".to_string());
        }
        self.config.insert(
            (self.head_x, -1),
            if prime { INPUT_PRIME } else { INPUT_COMPOSITE },
        );
        self.config = TravelingCounterRule.step(&self.config)?;
        self.head_x += 1;
        Ok(())
    }

    pub fn settle(&mut self) -> Result<(), String> {
        for _ in 0..self.width {
            if self.head_x >= self.capacity - 1 {
                return Err("traveling counter cannot settle beyond its capacity".to_string());
            }
            self.config = TravelingCounterRule.step(&self.config)?;
            self.head_x += 1;
        }
        Ok(())
    }

    pub fn value(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for y in 0..self.width {
            let state = self
                .config
                .get(&(self.head_x, y as i32))
                .ok_or_else(|| format!("traveling counter lost bit {}", y))?;
            let (_, bit, _) = decode_travel_bit(*state)
                .ok_or_else(|| "traveling counter head is not settled".to_string())?;
            value |= (bit as usize) << y;
        }
        Ok(value)
    }

    pub fn active_column_count(&self) -> usize {
        self.config
            .values()
            .filter(|state| decode_travel_bit(**state).is_some())
            .count()
    }
}

impl GapCounter {
    pub fn new(width: usize) -> Result<Self, String> {
        if width == 0 || width > usize::BITS as usize {
            return Err("a gap counter width must be between 1 and usize::BITS".to_string());
        }
        let mut config = LogicalConfig::new();
        for bit in 0..width {
            config.insert(
                (
                    i32::try_from(bit)
                        .map_err(|_| "gap counter width does not fit i32".to_string())?,
                    0,
                ),
                encode_gap_bit(bit == 0, false, false),
            );
        }
        Ok(GapCounter { config, width })
    }

    pub fn increment(&mut self) -> Result<(), String> {
        self.config.insert((0, -1), INPUT_COMPOSITE);
        self.config = GapCounterRule.step(&self.config)?;
        Ok(())
    }

    pub fn settle(&mut self) -> Result<(), String> {
        for _ in 0..self.width {
            self.config = GapCounterRule.step(&self.config)?;
        }
        Ok(())
    }

    pub fn value(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 0..self.width {
            let state = self
                .config
                .get(&(bit as i32, 0))
                .ok_or_else(|| format!("gap counter lost bit {}", bit))?;
            let (_, set, _) = decode_gap_bit(*state)
                .ok_or_else(|| "gap counter has an invalid bit".to_string())?;
            value |= (set as usize) << bit;
        }
        Ok(value)
    }
}

impl OrderedFifo {
    pub fn from_results(results: &[Option<FifoResult>]) -> Result<Self, String> {
        if results.is_empty() {
            return Err("an ordered FIFO needs at least one slot".to_string());
        }
        let mut config = LogicalConfig::new();
        for (index, ready) in results.iter().chain(std::iter::once(&None)).enumerate() {
            let state = match (index == 0, ready) {
                (true, None) => HEAD_PENDING,
                (true, Some(FifoResult::Prime)) => HEAD_PRIME,
                (true, Some(FifoResult::Composite)) => HEAD_COMPOSITE,
                (false, None) => SLOT_PENDING,
                (false, Some(FifoResult::Prime)) => SLOT_PRIME,
                (false, Some(FifoResult::Composite)) => SLOT_COMPOSITE,
            };
            config.insert(
                (
                    i32::try_from(index).map_err(|_| "too many FIFO slots for i32".to_string())?,
                    0,
                ),
                state,
            );
        }
        Ok(OrderedFifo {
            config,
            released: Vec::new(),
        })
    }

    pub fn inject_result(&mut self, index: usize, value: FifoResult) -> Result<(), String> {
        let x = i32::try_from(index).map_err(|_| "FIFO slot index does not fit i32".to_string())?;
        if !matches!(
            self.config.get(&(x, 0)).copied(),
            Some(SLOT_PENDING | HEAD_PENDING)
        ) {
            return Err(format!("FIFO slot {} is not pending", index));
        }
        self.config.insert(
            (x, -1),
            match value {
                FifoResult::Prime => INPUT_PRIME,
                FifoResult::Composite => INPUT_COMPOSITE,
            },
        );
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = OrderedFifoRule.step(&self.config)?;
        let mut events: Vec<_> = self
            .config
            .iter()
            .filter_map(|(point, state)| {
                let event = match *state {
                    RELEASE_PRIME => Some(FifoResult::Prime),
                    RELEASE_COMPOSITE => Some(FifoResult::Composite),
                    _ => None,
                }?;
                Some((point.0, event))
            })
            .collect();
        events.sort_unstable_by_key(|(x, _)| *x);
        self.released
            .extend(events.into_iter().map(|(_, event)| event));
        Ok(())
    }

    pub fn head_index(&self) -> Option<usize> {
        self.config.iter().find_map(|(point, state)| {
            matches!(*state, HEAD_PENDING | HEAD_PRIME | HEAD_COMPOSITE).then_some(point.0 as usize)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_releases_only_the_oldest_ready_slot() {
        let mut fifo = OrderedFifo::from_results(&[
            Some(FifoResult::Prime),
            Some(FifoResult::Composite),
            Some(FifoResult::Prime),
        ])
        .unwrap();
        fifo.step().unwrap();
        assert_eq!(fifo.released, [FifoResult::Prime]);
        assert_eq!(fifo.head_index(), Some(1));
        fifo.step().unwrap();
        fifo.step().unwrap();
        assert_eq!(
            fifo.released,
            [FifoResult::Prime, FifoResult::Composite, FifoResult::Prime]
        );
    }

    #[test]
    fn fifo_head_waits_for_a_pending_candidate() {
        let mut fifo = OrderedFifo::from_results(&[
            None,
            Some(FifoResult::Prime),
            Some(FifoResult::Composite),
        ])
        .unwrap();
        for _ in 0..10 {
            fifo.step().unwrap();
        }
        assert!(fifo.released.is_empty());
        assert_eq!(fifo.head_index(), Some(0));

        fifo.inject_result(0, FifoResult::Composite).unwrap();
        fifo.step().unwrap();
        assert_eq!(fifo.head_index(), Some(0));
        fifo.step().unwrap();
        assert_eq!(fifo.released, [FifoResult::Composite]);
        for _ in 0..4 {
            fifo.step().unwrap();
        }
        assert_eq!(
            fifo.released,
            [
                FifoResult::Composite,
                FifoResult::Prime,
                FifoResult::Composite
            ]
        );
    }

    #[test]
    fn fifo_cannot_inject_into_a_ready_slot() {
        let mut fifo = OrderedFifo::from_results(&[Some(FifoResult::Prime)]).unwrap();
        assert!(fifo.inject_result(0, FifoResult::Composite).is_err());
        assert!(OrderedFifo::from_results(&[]).is_err());
    }

    #[test]
    fn mesh_result_releases_through_the_integrated_ca_fifo() {
        let offset = worker_rule_state_count();
        let worker_cells = MeshCandidate::new(97).unwrap().config.len();
        let mut candidate = IntegratedCandidate::new(97).unwrap();
        assert_eq!(
            candidate.run_until_release(5_000).unwrap(),
            FifoResult::Prime
        );
        candidate.settle_gap().unwrap();
        assert_eq!(candidate.gap_value().unwrap(), 1);
        assert_eq!(
            candidate
                .config
                .values()
                .filter(|&&state| state > 0 && state < offset)
                .count(),
            worker_cells
        );
    }

    #[test]
    fn gap_counter_accepts_one_composite_pulse_per_step() {
        let mut counter = GapCounter::new(16).unwrap();
        for expected in 1..=500 {
            counter.increment().unwrap();
            if expected % 17 == 0 {
                counter.settle().unwrap();
                assert_eq!(counter.value().unwrap(), expected);
            }
        }
        counter.settle().unwrap();
        assert_eq!(counter.value().unwrap(), 500);
    }

    #[test]
    fn traveling_counter_moves_and_counts_without_changing_live_cells() {
        let mut counter = TravelingGapCounter::new(16, 600).unwrap();
        let active = counter.config.len();
        for _ in 0..500 {
            counter.advance(false).unwrap();
            assert_eq!(counter.config.len(), active);
        }
        counter.settle().unwrap();
        assert_eq!(counter.value().unwrap(), 500);
        assert_eq!(counter.active_column_count(), 16);
        assert_eq!(counter.config.len(), active);
    }

    #[test]
    fn traveling_prime_pulse_supplies_the_final_gap_increment() {
        let mut counter = TravelingGapCounter::new(8, 32).unwrap();
        for _ in 0..5 {
            counter.advance(false).unwrap();
        }
        counter.advance(true).unwrap();
        counter.settle().unwrap();
        assert_eq!(counter.value().unwrap(), 6);
    }

    #[test]
    fn traveling_counter_rejects_invalid_widths_and_exhaustion() {
        assert!(GapCounter::new(usize::BITS as usize + 1).is_err());
        assert!(TravelingGapCounter::new(usize::BITS as usize + 1, 1000).is_err());
        let mut counter = TravelingGapCounter::new(1, 2).unwrap();
        counter.advance(false).unwrap();
        assert!(counter.advance(false).is_err());
        assert!(counter.settle().is_err());
    }

    #[test]
    fn local_binary_subtractor_computes_prime_gap_registers() {
        for (previous, current) in [(3, 5), (5, 7), (7, 11), (23, 29), (89, 97)] {
            let mut subtract = BinarySubtract::new(current, previous, 8).unwrap();
            subtract.run_until_done().unwrap();
            assert_eq!(subtract.value().unwrap(), current - previous);
        }
        for minuend in 0..128 {
            for subtrahend in 0..=minuend {
                let mut subtract = BinarySubtract::new(minuend, subtrahend, 8).unwrap();
                subtract.run_until_done().unwrap();
                assert_eq!(subtract.value().unwrap(), minuend - subtrahend);
            }
        }
    }
}
