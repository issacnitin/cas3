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
    candidate_result_code, recycle_candidate_result, recycle_candidate_result_with_stride,
    worker_candidate_value, worker_candidate_value_at, worker_rule_state_count,
    worker_with_stride_bit, MeshCandidate, PrimeWorker, PrimeWorkerRule, RollingPrimeWorkerRule,
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

pub const PORT_IDLE: usize = 1;
pub const PORT_READY_PRIME: usize = 2;
pub const PORT_READY_COMPOSITE: usize = 3;
pub const PORT_RELEASE_PRIME: usize = 4;
pub const PORT_RELEASE_COMPOSITE: usize = 5;
pub const ROLLING_PORT_STATE_COUNT: usize = 6;
pub const STRIDE_ZERO: usize = 1;
pub const STRIDE_ONE: usize = 2;
pub const STRIDE_STATE_COUNT: usize = 3;
const MERGE_ROLE_COUNT: usize = 4;
const MERGE_PHASE_COUNT: usize = 6;
const MERGE_STATE_COUNT: usize = 1 + MERGE_ROLE_COUNT * MERGE_PHASE_COUNT;
const RESULT_PRIME: usize = 1;
const RESULT_COMPOSITE: usize = 2;
const RESULT_COLLECTOR: usize = 3;
const RESULT_RELEASE_PRIME: usize = 4;
const RESULT_RELEASE_COMPOSITE: usize = 5;
const RESULT_STATE_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MergeRole {
    Top,
    Wire,
    Gate,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MergePhase {
    Idle,
    CheckDown,
    WaitReturn,
    ReleaseUp,
    EmitPrime,
    EmitComposite,
}

#[derive(Clone, Copy)]
struct MergeCell {
    role: MergeRole,
    phase: MergePhase,
}

impl MergeCell {
    fn encode(self) -> usize {
        let role = match self.role {
            MergeRole::Top => 0,
            MergeRole::Wire => 1,
            MergeRole::Gate => 2,
            MergeRole::Bottom => 3,
        };
        let phase = match self.phase {
            MergePhase::Idle => 0,
            MergePhase::CheckDown => 1,
            MergePhase::WaitReturn => 2,
            MergePhase::ReleaseUp => 3,
            MergePhase::EmitPrime => 4,
            MergePhase::EmitComposite => 5,
        };
        1 + phase * MERGE_ROLE_COUNT + role
    }

    fn decode(state: usize) -> Option<Self> {
        if state == 0 || state >= MERGE_STATE_COUNT {
            return None;
        }
        let raw = state - 1;
        let role = match raw % MERGE_ROLE_COUNT {
            0 => MergeRole::Top,
            1 => MergeRole::Wire,
            2 => MergeRole::Gate,
            3 => MergeRole::Bottom,
            _ => unreachable!(),
        };
        let phase = match raw / MERGE_ROLE_COUNT {
            0 => MergePhase::Idle,
            1 => MergePhase::CheckDown,
            2 => MergePhase::WaitReturn,
            3 => MergePhase::ReleaseUp,
            4 => MergePhase::EmitPrime,
            5 => MergePhase::EmitComposite,
            _ => return None,
        };
        Some(MergeCell { role, phase })
    }
}
const ROLLING_GAP_PAYLOAD: usize = 1 << 4;
const ROLLING_GAP_BASE: usize = 1;
const ROLLING_GAP_LIMIT: usize = ROLLING_GAP_BASE + ROLLING_GAP_PAYLOAD;
const ROLLING_RECORD_BASE: usize = ROLLING_GAP_LIMIT;
const ROLLING_RECORD_STATE_COUNT: usize = 2;

fn encode_rolling_gap_bit(lsb: bool, bit: bool, pulse: bool, snapshot: bool) -> usize {
    ROLLING_GAP_BASE
        + (lsb as usize)
        + 2 * (bit as usize)
        + 4 * (pulse as usize)
        + 8 * (snapshot as usize)
}

fn decode_rolling_gap_bit(state: usize) -> Option<(bool, bool, bool, bool)> {
    if !(ROLLING_GAP_BASE..ROLLING_GAP_LIMIT).contains(&state) {
        return None;
    }
    let payload = state - ROLLING_GAP_BASE;
    Some((
        payload & 1 != 0,
        payload & 2 != 0,
        payload & 4 != 0,
        payload & 8 != 0,
    ))
}

fn encode_record_bit(bit: bool) -> usize {
    ROLLING_RECORD_BASE + bit as usize
}

fn decode_record_bit(state: usize) -> Option<bool> {
    (ROLLING_RECORD_BASE..ROLLING_RECORD_BASE + ROLLING_RECORD_STATE_COUNT)
        .contains(&state)
        .then_some(state != ROLLING_RECORD_BASE)
}

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

/// Conservative all-epoch resource certificate for the recurrent batch architecture.
///
/// During the prime interval `[N, 2N]`, one batch is admitted per output generation.  Each batch
/// has `ceil(4 ln N)` consecutive candidates.  A ring family is revisited only after the serial
/// trial-division bound below, so its lanes have completed before their next batch.  The ring
/// footprint includes a factor-four allowance for local builders, ordered merge, and recycling.
///
/// Prime coverage uses Dusart's explicit 2010 bounds:
///
///   pi(x) >= x/ln(x) * (1 + 1/ln(x))        for x >= 599
///   pi(x) <= x/ln(x) * (1 + 1.2762/ln(x))   for x > 1
///
/// The split reserve uses the same paper's prime-in-short-interval theorem:
///
///   [x, x + x/(25 ln^2 x)] contains a prime for x >= 396738.
#[derive(Clone, Debug)]
pub struct PrimeEpochPlan {
    pub power: u32,
    pub lower: f64,
    pub lanes: u64,
    pub word_bits: u32,
    pub worker_latency: f64,
    pub ring_families: f64,
    pub ring_blocks: f64,
    pub gap_buffer_blocks: f64,
    pub scheduler_live_cells: f64,
    pub scheduler_fraction: f64,
    pub candidate_coverage: f64,
    pub maximum_gap_fraction: f64,
    pub mask_latency: f64,
    pub mask_lead_fraction: f64,
    pub queue_latency: f64,
    pub queue_lead_fraction: f64,
}

impl PrimeEpochPlan {
    pub const PACKED_GAP_BITS: f64 = 20.0;

    pub fn for_power(power: u32) -> Result<Self, String> {
        if !(10..=512).contains(&power) {
            return Err("epoch power must be between 10 and 512".to_string());
        }
        let lower = 2f64.powi(power as i32);
        let ln_n = lower.ln();
        let lanes = (4.0 * ln_n).ceil() as u64;
        let word_bits = power + 4;
        let width = f64::from(word_bits);

        // Every trial divisor performs a bounded number of full-tape sweeps.  One hundred
        // width-squared steps per divisor is deliberately loose and covers the merge/reset tail.
        let worker_latency = 100.0 * (2.0 * lower.sqrt() + 1.0) * (width * width + width + 1.0);
        let ring_families = (worker_latency + 8.0 * lanes as f64 + 4.0 * width).ceil();
        let family_blocks = lanes as f64 * (4.0 * width + 16.0);
        let ring_blocks = 4.0 * ring_families * family_blocks;

        // Two variable-length gap buffers cover the current and next dyadic interval.  Concavity
        // bounds their self-delimiting binary records from the interval length and pi(4N).
        let four_n = 4.0 * lower;
        let upper_records = dusart_upper_pi(four_n);
        let average_gap = 2.0 * lower / upper_records.max(1.0);
        let gap_bits = upper_records * ((average_gap + 1.0).log2() + 2.0);
        let gap_buffer_blocks = 2.0 * gap_bits / Self::PACKED_GAP_BITS;

        let scheduler_live_cells = 4.0 * (ring_blocks + gap_buffer_blocks);
        let scheduler_fraction = scheduler_live_cells / lower;
        let output_generations = (dusart_lower_pi(2.0 * lower) - dusart_upper_pi(lower)).max(0.0);
        let candidate_coverage = lanes as f64 * output_generations / (2.0 * lower);
        let maximum_gap_fraction = 1.0 / (25.0 * ln_n * ln_n);
        let maximum_splits = lower * maximum_gap_fraction / 4.0;
        let mask_width = (maximum_splits + 1.0).log2().ceil() + 1.0;
        let mask_latency = (8.0 * mask_width + 2.0) * maximum_splits + 4.0 * mask_width;
        let mask_lead_fraction = mask_latency / output_generations;
        let queue_side = gap_buffer_blocks.sqrt().ceil();
        let packed_chunks = (width / PrimeEpochPlan::PACKED_GAP_BITS).ceil();
        let queue_row_height = packed_chunks * PrimeEpochPlan::PACKED_GAP_BITS + 1.0;
        let queue_latency = queue_side * (1.0 + queue_row_height);
        let queue_lead_fraction = queue_latency / output_generations;

        Ok(PrimeEpochPlan {
            power,
            lower,
            lanes,
            word_bits,
            worker_latency,
            ring_families,
            ring_blocks,
            gap_buffer_blocks,
            scheduler_live_cells,
            scheduler_fraction,
            candidate_coverage,
            maximum_gap_fraction,
            mask_latency,
            mask_lead_fraction,
            queue_latency,
            queue_lead_fraction,
        })
    }

    pub fn certified(&self) -> bool {
        self.lower >= 396_738.0
            && self.candidate_coverage >= 1.0
            && self.scheduler_fraction <= 0.5
            && self.maximum_gap_fraction < 0.5
            && self.mask_lead_fraction < 0.25
            && self.queue_lead_fraction < 0.01
            && self
                .next_epoch_build_fraction()
                .map(|fraction| fraction <= 0.5)
                .unwrap_or(false)
    }

    pub fn next_epoch_build_fraction(&self) -> Result<f64, String> {
        let next = Self::for_power(
            self.power
                .checked_add(1)
                .ok_or_else(|| "next epoch power overflowed".to_string())?,
        )?;
        Ok((next.scheduler_live_cells - self.scheduler_live_cells) / self.lower)
    }

    pub fn next_epoch_reserve_fraction(&self) -> Result<f64, String> {
        let next = Self::for_power(
            self.power
                .checked_add(1)
                .ok_or_else(|| "next epoch power overflowed".to_string())?,
        )?;
        Ok((2.0 * self.lower - next.scheduler_live_cells) / (2.0 * self.lower))
    }

    pub fn first_certified_power() -> Result<u32, String> {
        (10..=512)
            .find_map(|power| {
                Self::for_power(power)
                    .ok()
                    .filter(PrimeEpochPlan::certified)
                    .map(|_| power)
            })
            .ok_or_else(|| "no certified dyadic epoch fits the checked range".to_string())
    }
}

fn dusart_lower_pi(x: f64) -> f64 {
    let logarithm = x.ln();
    x / logarithm * (1.0 + 1.0 / logarithm)
}

fn dusart_upper_pi(x: f64) -> f64 {
    let logarithm = x.ln();
    x / logarithm * (1.0 + 1.2762 / logarithm)
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

/// Recurrent result port for one serial worker.
///
/// Unlike `OrderedFifo`, this port has no consumed terminal state.  Once a result is released it
/// returns to idle, and the worker starts its local increment/reset sweep on the acknowledgement
/// edge.  Results always return to the fixed LSB, so tape-width growth does not need a host-side
/// rebuild or a moving output port.
pub struct RollingPrimeSchedulerRule;

impl RollingPrimeSchedulerRule {
    fn encode_port(state: usize) -> usize {
        worker_rule_state_count() + state
    }

    fn decode_port(state: usize) -> Option<usize> {
        let offset = worker_rule_state_count();
        (state > offset && state < offset + ROLLING_PORT_STATE_COUNT).then(|| state - offset)
    }

    fn port_at_idle(state: usize) -> bool {
        Self::decode_port(state) == Some(PORT_IDLE)
    }

    fn gap_offset() -> usize {
        worker_rule_state_count() + ROLLING_PORT_STATE_COUNT
    }

    fn encode_gap(state: usize) -> usize {
        Self::gap_offset() + state
    }

    fn decode_gap(state: usize) -> Option<usize> {
        let offset = Self::gap_offset();
        let raw = state.checked_sub(offset)?;
        (ROLLING_GAP_BASE..ROLLING_GAP_LIMIT)
            .contains(&raw)
            .then_some(raw)
    }

    fn encode_record(state: usize) -> usize {
        Self::gap_offset() + state
    }

    fn decode_record(state: usize) -> Option<usize> {
        let offset = Self::gap_offset();
        let raw = state.checked_sub(offset)?;
        (ROLLING_RECORD_BASE..ROLLING_RECORD_BASE + ROLLING_RECORD_STATE_COUNT)
            .contains(&raw)
            .then_some(raw)
    }
}

impl LocalRule for RollingPrimeSchedulerRule {
    fn state_count(&self) -> usize {
        Self::gap_offset() + ROLLING_RECORD_BASE + ROLLING_RECORD_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let offset = worker_rule_state_count();
        if let Some(port) = Self::decode_port(center) {
            let composite = candidate_result_code(neighbours[1]);
            return match port {
                PORT_IDLE => match composite {
                    Some(false) => Self::encode_port(PORT_READY_PRIME),
                    Some(true) => Self::encode_port(PORT_READY_COMPOSITE),
                    _ => Self::encode_port(PORT_IDLE),
                },
                PORT_READY_PRIME => Self::encode_port(PORT_RELEASE_PRIME),
                PORT_READY_COMPOSITE => Self::encode_port(PORT_RELEASE_COMPOSITE),
                PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE => Self::encode_port(PORT_IDLE),
                _ => 0,
            };
        }

        if let Some(gap) = Self::decode_gap(center) {
            let (lsb, bit, _, snapshot) = decode_rolling_gap_bit(gap).unwrap();
            let (carry_in, snapshot_in) = if lsb {
                let port = Self::decode_port(neighbours[1]);
                (
                    matches!(port, Some(PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE)),
                    port == Some(PORT_RELEASE_PRIME),
                )
            } else {
                let west = Self::decode_gap(neighbours[3])
                    .and_then(decode_rolling_gap_bit)
                    .unwrap_or((false, false, false, false));
                (west.2, west.3)
            };
            return Self::encode_gap(encode_rolling_gap_bit(
                lsb,
                if snapshot { false } else { bit ^ carry_in },
                bit && carry_in,
                snapshot_in,
            ));
        }

        if let Some(record) = Self::decode_record(center) {
            let mut bit = decode_record_bit(record).unwrap();
            if let Some((_, source, _, true)) =
                Self::decode_gap(neighbours[1]).and_then(decode_rolling_gap_bit)
            {
                bit = source;
            }
            return Self::encode_record(encode_record_bit(bit));
        }

        if candidate_result_code(center).is_some() && Self::port_at_idle(neighbours[6]) {
            return recycle_candidate_result(center).unwrap();
        }

        let center = if center <= offset { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            local[slot] = if neighbours[slot] <= offset {
                neighbours[slot]
            } else {
                0
            };
        }
        RollingPrimeWorkerRule.next(center, local)
    }
}

/// A fixed lane family that recycles each worker by the family stride.
///
/// The stride is stored on a static finite-state tape immediately south of each worker.  When a
/// word grows, the stride tape grows one zero bit at the same east edge.  No candidate value,
/// stride, or result is supplied after the initial CA configuration.
pub struct RollingStrideSchedulerRule;

impl RollingStrideSchedulerRule {
    fn stride_offset() -> usize {
        worker_rule_state_count() + ROLLING_PORT_STATE_COUNT
    }

    pub fn encode_stride(bit: bool) -> usize {
        Self::stride_offset() + if bit { STRIDE_ONE } else { STRIDE_ZERO }
    }

    fn decode_stride(state: usize) -> Option<bool> {
        let offset = Self::stride_offset();
        match state {
            state if state == offset + STRIDE_ZERO => Some(false),
            state if state == offset + STRIDE_ONE => Some(true),
            _ => None,
        }
    }
}

impl LocalRule for RollingStrideSchedulerRule {
    fn state_count(&self) -> usize {
        Self::stride_offset() + STRIDE_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let worker_states = worker_rule_state_count();
        if Self::decode_stride(center).is_some() {
            return center;
        }

        if let Some(port) = RollingPrimeSchedulerRule::decode_port(center) {
            return match port {
                PORT_IDLE => match candidate_result_code(neighbours[1]) {
                    Some(false) => RollingPrimeSchedulerRule::encode_port(PORT_READY_PRIME),
                    Some(true) => RollingPrimeSchedulerRule::encode_port(PORT_READY_COMPOSITE),
                    _ => RollingPrimeSchedulerRule::encode_port(PORT_IDLE),
                },
                PORT_READY_PRIME => RollingPrimeSchedulerRule::encode_port(PORT_RELEASE_PRIME),
                PORT_READY_COMPOSITE => {
                    RollingPrimeSchedulerRule::encode_port(PORT_RELEASE_COMPOSITE)
                }
                PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE => {
                    RollingPrimeSchedulerRule::encode_port(PORT_IDLE)
                }
                _ => 0,
            };
        }

        if center == 0
            && Self::decode_stride(neighbours[3]).is_some()
            && neighbours[6] > 0
            && neighbours[6] <= worker_states
        {
            return Self::encode_stride(false);
        }

        if candidate_result_code(center).is_some()
            && matches!(
                RollingPrimeSchedulerRule::decode_port(neighbours[6]),
                Some(PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE)
            )
        {
            return recycle_candidate_result_with_stride(center).unwrap();
        }

        let center = if center <= worker_states { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            local[slot] = if neighbours[slot] <= worker_states {
                neighbours[slot]
            } else {
                0
            };
        }
        let center =
            worker_with_stride_bit(center, Self::decode_stride(neighbours[1]).unwrap_or(false));
        local[3] = worker_with_stride_bit(
            local[3],
            Self::decode_stride(neighbours[0]).unwrap_or(false),
        );
        local[4] = worker_with_stride_bit(
            local[4],
            Self::decode_stride(neighbours[2]).unwrap_or(false),
        );
        RollingPrimeWorkerRule.next(center, local)
    }
}

pub struct RollingBatchSchedulerRule;

impl RollingBatchSchedulerRule {
    fn merge_offset() -> usize {
        RollingStrideSchedulerRule::stride_offset() + STRIDE_STATE_COUNT
    }

    fn encode_merge(cell: MergeCell) -> usize {
        Self::merge_offset() + cell.encode()
    }

    fn decode_merge(state: usize) -> Option<MergeCell> {
        MergeCell::decode(state.checked_sub(Self::merge_offset())?)
    }

    fn result_offset() -> usize {
        Self::merge_offset() + MERGE_STATE_COUNT
    }

    fn encode_result(state: usize) -> usize {
        Self::result_offset() + state
    }

    fn decode_result(state: usize) -> Option<usize> {
        let raw = state.checked_sub(Self::result_offset())?;
        (raw > 0 && raw < RESULT_STATE_COUNT).then_some(raw)
    }

    fn port_ready(state: usize) -> bool {
        matches!(
            RollingPrimeSchedulerRule::decode_port(state),
            Some(PORT_READY_PRIME | PORT_READY_COMPOSITE)
        )
    }

    fn emit_phase(state: usize) -> Option<MergePhase> {
        match RollingPrimeSchedulerRule::decode_port(state) {
            Some(PORT_READY_PRIME) => Some(MergePhase::EmitPrime),
            Some(PORT_READY_COMPOSITE) => Some(MergePhase::EmitComposite),
            _ => None,
        }
    }
}

impl LocalRule for RollingBatchSchedulerRule {
    fn state_count(&self) -> usize {
        Self::result_offset() + RESULT_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if let Some(mut merge) = Self::decode_merge(center) {
            let south = Self::decode_merge(neighbours[1]);
            let north = Self::decode_merge(neighbours[6]);
            let port_ready = Self::port_ready(neighbours[4]);
            match merge.phase {
                MergePhase::Idle => {
                    if merge.role == MergeRole::Top && port_ready {
                        merge.phase = MergePhase::CheckDown;
                    } else if north
                        .map(|cell| cell.phase == MergePhase::CheckDown)
                        .unwrap_or(false)
                        && (merge.role == MergeRole::Wire || port_ready)
                    {
                        merge.phase = MergePhase::CheckDown;
                    } else if south
                        .map(|cell| cell.phase == MergePhase::ReleaseUp)
                        .unwrap_or(false)
                    {
                        merge.phase = if merge.role == MergeRole::Wire {
                            MergePhase::ReleaseUp
                        } else {
                            Self::emit_phase(neighbours[4]).unwrap_or(MergePhase::Idle)
                        };
                    }
                }
                MergePhase::CheckDown => {
                    if merge.role == MergeRole::Bottom {
                        merge.phase = Self::emit_phase(neighbours[4]).unwrap_or(MergePhase::Idle);
                    } else if south
                        .map(|cell| cell.role == MergeRole::Wire)
                        .unwrap_or(false)
                        || (south.is_some() && Self::port_ready(neighbours[2]))
                    {
                        merge.phase = if merge.role == MergeRole::Top {
                            MergePhase::WaitReturn
                        } else {
                            MergePhase::Idle
                        };
                    }
                }
                MergePhase::WaitReturn => {
                    if south
                        .map(|cell| cell.phase == MergePhase::ReleaseUp)
                        .unwrap_or(false)
                    {
                        merge.phase = Self::emit_phase(neighbours[4]).unwrap_or(MergePhase::Idle);
                    }
                }
                MergePhase::ReleaseUp => {
                    merge.phase = MergePhase::Idle;
                }
                MergePhase::EmitPrime | MergePhase::EmitComposite => {
                    merge.phase = if merge.role == MergeRole::Top {
                        MergePhase::Idle
                    } else {
                        MergePhase::ReleaseUp
                    };
                }
            }
            return Self::encode_merge(merge);
        }

        if let Some(result) = Self::decode_result(center) {
            return match result {
                RESULT_PRIME | RESULT_COMPOSITE => 0,
                RESULT_COLLECTOR => match Self::decode_result(neighbours[6]) {
                    Some(RESULT_PRIME) => Self::encode_result(RESULT_RELEASE_PRIME),
                    Some(RESULT_COMPOSITE) => Self::encode_result(RESULT_RELEASE_COMPOSITE),
                    _ => Self::encode_result(RESULT_COLLECTOR),
                },
                RESULT_RELEASE_PRIME | RESULT_RELEASE_COMPOSITE => {
                    Self::encode_result(RESULT_COLLECTOR)
                }
                _ => 0,
            };
        }

        if RollingStrideSchedulerRule::decode_stride(center).is_some() {
            return center;
        }

        if let Some(port) = RollingPrimeSchedulerRule::decode_port(center) {
            return match port {
                PORT_IDLE => match candidate_result_code(neighbours[1]) {
                    Some(false) => RollingPrimeSchedulerRule::encode_port(PORT_READY_PRIME),
                    Some(true) => RollingPrimeSchedulerRule::encode_port(PORT_READY_COMPOSITE),
                    _ => RollingPrimeSchedulerRule::encode_port(PORT_IDLE),
                },
                PORT_READY_PRIME
                    if Self::decode_merge(neighbours[3])
                        .map(|cell| cell.phase == MergePhase::EmitPrime)
                        .unwrap_or(false) =>
                {
                    RollingPrimeSchedulerRule::encode_port(PORT_RELEASE_PRIME)
                }
                PORT_READY_COMPOSITE
                    if Self::decode_merge(neighbours[3])
                        .map(|cell| cell.phase == MergePhase::EmitComposite)
                        .unwrap_or(false) =>
                {
                    RollingPrimeSchedulerRule::encode_port(PORT_RELEASE_COMPOSITE)
                }
                PORT_READY_PRIME => RollingPrimeSchedulerRule::encode_port(PORT_READY_PRIME),
                PORT_READY_COMPOSITE => {
                    RollingPrimeSchedulerRule::encode_port(PORT_READY_COMPOSITE)
                }
                PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE => {
                    RollingPrimeSchedulerRule::encode_port(PORT_IDLE)
                }
                _ => 0,
            };
        }

        if center == 0 {
            if let Some(merge) = Self::decode_merge(neighbours[4]) {
                match merge.phase {
                    MergePhase::EmitPrime => {
                        return Self::encode_result(RESULT_PRIME);
                    }
                    MergePhase::EmitComposite => {
                        return Self::encode_result(RESULT_COMPOSITE);
                    }
                    _ => {}
                }
            }
            if let Some(result) = Self::decode_result(neighbours[6]) {
                if matches!(result, RESULT_PRIME | RESULT_COMPOSITE) {
                    return Self::encode_result(result);
                }
            }
        }

        if center == 0
            && RollingStrideSchedulerRule::decode_stride(neighbours[3]).is_some()
            && neighbours[6] > 0
            && neighbours[6] <= worker_rule_state_count()
        {
            return RollingStrideSchedulerRule::encode_stride(false);
        }

        if candidate_result_code(center).is_some()
            && matches!(
                RollingPrimeSchedulerRule::decode_port(neighbours[6]),
                Some(PORT_RELEASE_PRIME | PORT_RELEASE_COMPOSITE)
            )
        {
            return recycle_candidate_result_with_stride(center).unwrap();
        }

        let worker_states = worker_rule_state_count();
        let center = if center <= worker_states { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            local[slot] = if neighbours[slot] <= worker_states {
                neighbours[slot]
            } else {
                0
            };
        }
        let center = worker_with_stride_bit(
            center,
            RollingStrideSchedulerRule::decode_stride(neighbours[1]).unwrap_or(false),
        );
        local[3] = worker_with_stride_bit(
            local[3],
            RollingStrideSchedulerRule::decode_stride(neighbours[0]).unwrap_or(false),
        );
        local[4] = worker_with_stride_bit(
            local[4],
            RollingStrideSchedulerRule::decode_stride(neighbours[2]).unwrap_or(false),
        );
        RollingPrimeWorkerRule.next(center, local)
    }
}

pub struct RollingCandidate {
    pub config: LogicalConfig,
    pub steps: usize,
    pub released: Vec<FifoResult>,
    pub classified: Vec<usize>,
    pub gaps: Vec<usize>,
    pending_classified: Option<usize>,
    gap_width: usize,
}

pub struct RollingCandidateField {
    pub config: LogicalConfig,
    pub steps: usize,
    pub results: Vec<Vec<(usize, FifoResult)>>,
    pending: Vec<Option<usize>>,
    rows: Vec<i32>,
}

pub struct RollingBatchField {
    pub config: LogicalConfig,
    pub steps: usize,
    pub batches: Vec<Vec<(usize, FifoResult)>>,
    pub stream: Vec<FifoResult>,
    pending: Vec<Option<usize>>,
    rows: Vec<i32>,
    releasing: Vec<(usize, FifoResult)>,
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

impl RollingCandidate {
    pub fn new(candidate: usize) -> Result<Self, String> {
        let mut config = PrimeWorker::new(candidate)?.config;
        config.insert((0, 1), RollingPrimeSchedulerRule::encode_port(PORT_IDLE));
        let gap_width = 16;
        for bit in 0..gap_width {
            config.insert(
                (bit as i32, 2),
                RollingPrimeSchedulerRule::encode_gap(encode_rolling_gap_bit(
                    bit == 0,
                    false,
                    false,
                    false,
                )),
            );
            config.insert(
                (bit as i32, 3),
                RollingPrimeSchedulerRule::encode_record(encode_record_bit(false)),
            );
        }
        Ok(RollingCandidate {
            config,
            steps: 0,
            released: Vec::new(),
            classified: Vec::new(),
            gaps: Vec::new(),
            pending_classified: None,
            gap_width,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        let snapshot_complete = self
            .config
            .get(&((self.gap_width - 1) as i32, 2))
            .and_then(|state| RollingPrimeSchedulerRule::decode_gap(*state))
            .and_then(decode_rolling_gap_bit)
            .map(|(_, _, _, snapshot)| snapshot)
            .unwrap_or(false);
        if self
            .config
            .values()
            .any(|state| candidate_result_code(*state).is_some())
        {
            self.pending_classified = worker_candidate_value(&self.config);
        }
        self.config = RollingPrimeSchedulerRule.step(&self.config)?;
        self.steps += 1;
        if let Some(result) = self.config.values().find_map(|state| {
            let port = RollingPrimeSchedulerRule::decode_port(*state)?;
            match port {
                PORT_RELEASE_PRIME => Some(FifoResult::Prime),
                PORT_RELEASE_COMPOSITE => Some(FifoResult::Composite),
                _ => None,
            }
        }) {
            self.released.push(result);
            self.classified.push(
                self.pending_classified
                    .take()
                    .ok_or_else(|| "rolling candidate result lost its tape value")?,
            );
        }
        if snapshot_complete {
            self.gaps.push(self.gap_value()?);
        }
        Ok(())
    }

    pub fn run_until_releases(
        &mut self,
        count: usize,
        max_steps: usize,
    ) -> Result<Vec<FifoResult>, String> {
        while self.released.len() < count && self.steps < max_steps {
            self.step()?;
        }
        if self.released.len() != count {
            return Err(format!(
                "rolling worker released {} of {} candidates in {} local steps",
                self.released.len(),
                count,
                max_steps
            ));
        }
        Ok(self.released.clone())
    }

    pub fn settle_gap(&mut self) -> Result<(), String> {
        for _ in 0..self.gap_width {
            self.step()?;
        }
        Ok(())
    }

    pub fn release_count(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 0..self.gap_width {
            let state = self
                .config
                .get(&(bit as i32, 2))
                .ok_or_else(|| format!("rolling release counter lost bit {}", bit))?;
            let raw = RollingPrimeSchedulerRule::decode_gap(*state)
                .ok_or_else(|| "rolling release counter has an invalid state".to_string())?;
            let (_, set, _, _) = decode_rolling_gap_bit(raw)
                .ok_or_else(|| "rolling release counter bit cannot be decoded".to_string())?;
            value |= (set as usize) << bit;
        }
        Ok(value)
    }

    pub fn gap_value(&self) -> Result<usize, String> {
        let mut value = 0usize;
        for bit in 0..self.gap_width {
            let state = self
                .config
                .get(&(bit as i32, 3))
                .ok_or_else(|| format!("rolling gap record lost bit {}", bit))?;
            let raw = RollingPrimeSchedulerRule::decode_record(*state)
                .ok_or_else(|| "rolling gap record has an invalid state".to_string())?;
            let set = decode_record_bit(raw)
                .ok_or_else(|| "rolling gap record bit cannot be decoded".to_string())?;
            value |= (set as usize) << bit;
        }
        Ok(value)
    }
}

impl RollingCandidateField {
    pub fn new(start: usize, lanes: usize) -> Result<Self, String> {
        Self::new_with_stride(start, lanes, lanes)
    }

    pub fn new_with_stride(start: usize, lanes: usize, stride: usize) -> Result<Self, String> {
        if lanes == 0 {
            return Err("a rolling candidate field needs at least one lane".to_string());
        }
        if stride == 0 {
            return Err("a rolling candidate field needs a positive stride".to_string());
        }
        if start < 2 {
            return Err("a rolling candidate field starts at an integer of at least 2".to_string());
        }
        let last = start
            .checked_add(lanes - 1)
            .ok_or_else(|| "rolling candidate field exceeds usize".to_string())?;
        let width_value = last.max(stride);
        let width = (usize::BITS - width_value.leading_zeros()) as usize + 1;

        let mut config = LogicalConfig::new();
        let mut rows = Vec::with_capacity(lanes);
        for lane in 0..lanes {
            let candidate = start
                .checked_add(lane)
                .ok_or_else(|| "rolling lane candidate exceeds usize".to_string())?;
            let row = i32::try_from(lane)
                .ok()
                .and_then(|lane| lane.checked_mul(4))
                .ok_or_else(|| "rolling lane geometry exceeds i32".to_string())?;
            let worker = PrimeWorker::with_width(candidate, width)?;
            for ((x, _), state) in worker.config {
                config.insert((x, row), state);
            }
            config.insert(
                (0, row + 1),
                RollingPrimeSchedulerRule::encode_port(PORT_IDLE),
            );
            for x in 0..width as i32 {
                let bit = x < usize::BITS as i32 && stride & (1usize << x as usize) != 0;
                config.insert((x, row - 1), RollingStrideSchedulerRule::encode_stride(bit));
            }
            rows.push(row);
        }
        Ok(RollingCandidateField {
            config,
            steps: 0,
            results: vec![Vec::new(); lanes],
            pending: vec![None; lanes],
            rows,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        for (lane, row) in self.rows.iter().copied().enumerate() {
            if self
                .config
                .get(&(0, row))
                .and_then(|state| candidate_result_code(*state))
                .is_some()
            {
                self.pending[lane] = worker_candidate_value_at(&self.config, row);
            }
        }
        self.config = RollingStrideSchedulerRule.step(&self.config)?;
        self.steps += 1;
        for (lane, row) in self.rows.iter().copied().enumerate() {
            let Some(state) = self.config.get(&(0, row + 1)) else {
                return Err(format!("rolling lane {} lost its result port", lane));
            };
            let result = match RollingPrimeSchedulerRule::decode_port(*state) {
                Some(PORT_RELEASE_PRIME) => Some(FifoResult::Prime),
                Some(PORT_RELEASE_COMPOSITE) => Some(FifoResult::Composite),
                _ => None,
            };
            if let Some(result) = result {
                let candidate = self.pending[lane]
                    .take()
                    .ok_or_else(|| format!("rolling lane {} lost its candidate", lane))?;
                self.results[lane].push((candidate, result));
            }
        }
        Ok(())
    }

    pub fn run_until_rounds(&mut self, rounds: usize, max_steps: usize) -> Result<(), String> {
        while self.steps < max_steps && self.results.iter().any(|results| results.len() < rounds) {
            self.step()?;
        }
        if self.results.iter().any(|results| results.len() < rounds) {
            return Err(format!(
                "rolling field did not complete {} rounds in {} local steps",
                rounds, max_steps
            ));
        }
        Ok(())
    }
}

impl RollingBatchField {
    pub fn new(start: usize, lanes: usize) -> Result<Self, String> {
        Self::new_with_stride(start, lanes, lanes)
    }

    pub fn new_with_stride(start: usize, lanes: usize, stride: usize) -> Result<Self, String> {
        if lanes < 2 {
            return Err("a rolling batch field needs at least two lanes".to_string());
        }
        let field = RollingCandidateField::new_with_stride(start, lanes, stride)?;
        let mut config = field.config;
        let rows = field.rows;
        let top = rows[0] + 1;
        let bottom = rows[lanes - 1] + 1;
        for y in top..=bottom {
            let role = if y == top {
                MergeRole::Bottom
            } else if y == bottom {
                MergeRole::Top
            } else if rows.iter().any(|row| *row + 1 == y) {
                MergeRole::Gate
            } else {
                MergeRole::Wire
            };
            config.insert(
                (-1, y),
                RollingBatchSchedulerRule::encode_merge(MergeCell {
                    role,
                    phase: MergePhase::Idle,
                }),
            );
        }
        config.insert(
            (-2, 0),
            RollingBatchSchedulerRule::encode_result(RESULT_COLLECTOR),
        );
        Ok(RollingBatchField {
            config,
            steps: 0,
            batches: Vec::new(),
            stream: Vec::new(),
            pending: vec![None; lanes],
            rows,
            releasing: Vec::with_capacity(lanes),
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        for (lane, row) in self.rows.iter().copied().enumerate() {
            if self
                .config
                .get(&(0, row))
                .and_then(|state| candidate_result_code(*state))
                .is_some()
            {
                self.pending[lane] = worker_candidate_value_at(&self.config, row);
            }
        }
        self.config = RollingBatchSchedulerRule.step(&self.config)?;
        self.steps += 1;
        if let Some(state) = self.config.get(&(-2, 0)) {
            match RollingBatchSchedulerRule::decode_result(*state) {
                Some(RESULT_RELEASE_PRIME) => self.stream.push(FifoResult::Prime),
                Some(RESULT_RELEASE_COMPOSITE) => self.stream.push(FifoResult::Composite),
                _ => {}
            }
        }

        for (lane, row) in self.rows.iter().copied().enumerate() {
            let state = self
                .config
                .get(&(0, row + 1))
                .ok_or_else(|| format!("rolling batch lane {} lost its port", lane))?;
            let result = match RollingPrimeSchedulerRule::decode_port(*state) {
                Some(PORT_RELEASE_PRIME) => Some(FifoResult::Prime),
                Some(PORT_RELEASE_COMPOSITE) => Some(FifoResult::Composite),
                _ => None,
            };
            if let Some(result) = result {
                if lane != self.releasing.len() {
                    return Err(format!(
                        "rolling merge released lane {} while waiting for lane {}",
                        lane,
                        self.releasing.len()
                    ));
                }
                let candidate = self.pending[lane]
                    .take()
                    .ok_or_else(|| format!("rolling batch lane {} lost its candidate", lane))?;
                self.releasing.push((candidate, result));
            }
        }
        if self.releasing.len() == self.rows.len() {
            self.batches.push(std::mem::take(&mut self.releasing));
            self.releasing = Vec::with_capacity(self.rows.len());
        }
        Ok(())
    }

    pub fn run_until_batches(&mut self, count: usize, max_steps: usize) -> Result<(), String> {
        while (self.batches.len() < count
            || self.stream.len() < count.saturating_mul(self.rows.len()))
            && self.steps < max_steps
        {
            self.step()?;
        }
        if self.batches.len() != count || self.stream.len() != count.saturating_mul(self.rows.len())
        {
            return Err(format!(
                "rolling batch field released {} batches and {} stream results; expected {} and {} in {} local steps",
                self.batches.len(),
                self.stream.len(),
                count,
                count.saturating_mul(self.rows.len()),
                max_steps
            ));
        }
        Ok(())
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
    use crate::construct::BlockMachine;

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
    fn rolling_worker_recycles_without_host_reinitialisation() {
        let first = 15usize;
        let count = 26usize;
        let mut candidate = RollingCandidate::new(first).unwrap();
        let actual = candidate
            .run_until_releases(count, 2_000_000)
            .unwrap_or_else(|error| {
                panic!("rolling worker after {} steps: {}", candidate.steps, error)
            });
        let expected = (first..first + count)
            .map(|value| {
                if (2..=value / 2).all(|divisor| value % divisor != 0) {
                    FifoResult::Prime
                } else {
                    FifoResult::Composite
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            candidate.classified,
            (first..first + count).collect::<Vec<_>>()
        );
        assert_eq!(actual, expected);
        candidate.settle_gap().unwrap();
        assert_eq!(candidate.gap_value().unwrap(), 6);
        assert_eq!(candidate.release_count().unwrap(), 3);
        assert_eq!(candidate.gaps, [3, 2, 4, 6, 2, 6]);
        assert_eq!(
            candidate
                .config
                .values()
                .filter(|state| RollingPrimeSchedulerRule::decode_port(**state).is_some())
                .count(),
            1
        );
    }

    #[test]
    fn rolling_scheduler_round_trips_through_large_binary_alphabet() {
        let logical = RollingCandidate::new(15).unwrap().config;
        let machine = BlockMachine::new(RollingPrimeSchedulerRule).unwrap();
        assert!(machine.codec.width > 500_000);
        let binary = machine.codec.encode(&logical).unwrap();
        assert_eq!(machine.codec.decode(&binary).unwrap(), logical);
        let next = machine.step_binary(&binary).unwrap();
        let decoded = machine.codec.decode(&next).unwrap();
        assert_eq!(decoded, RollingPrimeSchedulerRule.step(&logical).unwrap());
        assert_eq!(next.len(), machine.codec.logical_weight(&decoded).unwrap());
    }

    #[test]
    fn rolling_field_advances_all_lanes_by_its_local_stride() {
        let start = 28usize;
        let lanes = 4usize;
        let rounds = 3usize;
        let mut field = RollingCandidateField::new(start, lanes).unwrap();
        field.run_until_rounds(rounds, 2_000_000).unwrap();
        for lane in 0..lanes {
            let actual: Vec<_> = field.results[lane]
                .iter()
                .take(rounds)
                .map(|record| record.0)
                .collect();
            assert_eq!(
                actual,
                (0..rounds)
                    .map(|round| start + lane + round * lanes)
                    .collect::<Vec<_>>()
            );
            for &(candidate, result) in &field.results[lane] {
                let prime =
                    candidate >= 2 && (2..=candidate / 2).all(|divisor| candidate % divisor != 0);
                assert_eq!(result == FifoResult::Prime, prime, "candidate {candidate}");
            }
        }
        assert!(field.config.contains_key(&(6, 0)));
        assert!(field
            .config
            .values()
            .any(|state| RollingStrideSchedulerRule::decode_stride(*state) == Some(false)));
    }

    #[test]
    fn rolling_batch_barrier_releases_consecutive_candidates_together() {
        let start = 28usize;
        let lanes = 4usize;
        let stride = 12usize;
        let rounds = 3usize;
        let mut field = RollingBatchField::new_with_stride(start, lanes, stride).unwrap();
        field.run_until_batches(rounds, 2_000_000).unwrap();
        for (round, batch) in field.batches.iter().enumerate() {
            let candidates: Vec<_> = batch.iter().map(|record| record.0).collect();
            assert_eq!(
                candidates,
                (start + round * stride..start + round * stride + lanes).collect::<Vec<_>>()
            );
            for &(candidate, result) in batch {
                let prime =
                    candidate >= 2 && (2..=candidate / 2).all(|divisor| candidate % divisor != 0);
                assert_eq!(result == FifoResult::Prime, prime, "candidate {candidate}");
            }
        }
        assert_eq!(
            field.stream,
            field
                .batches
                .iter()
                .flat_map(|batch| batch.iter().map(|record| record.1))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn dyadic_epoch_certificate_closes_coverage_and_population_budget() {
        assert_eq!(PrimeEpochPlan::first_certified_power().unwrap(), 86);
        let before = PrimeEpochPlan::for_power(85).unwrap();
        assert!(before.candidate_coverage > 1.9);
        assert!(before.scheduler_fraction > 0.5);

        let plan = PrimeEpochPlan::for_power(86).unwrap();
        assert!(plan.certified());
        assert!(plan.candidate_coverage > 1.9);
        assert!(plan.scheduler_fraction < 0.46);
        assert!(plan.maximum_gap_fraction < 0.00002);
        assert!(plan.mask_lead_fraction < 0.095);
        assert!(plan.queue_lead_fraction < 0.0000000002);
        assert!(plan.next_epoch_build_fraction().unwrap() < 0.32);
        assert!(plan.next_epoch_reserve_fraction().unwrap() > 0.61);
        assert_eq!(plan.lanes, 239);
        assert_eq!(plan.word_bits, 90);
        assert!(plan.ring_blocks > plan.gap_buffer_blocks);
    }

    #[test]
    fn implemented_dusart_bounds_bracket_actual_prime_counts() {
        let limit = 1usize << 19;
        let mut prime = vec![true; limit + 1];
        prime[0] = false;
        prime[1] = false;
        for divisor in 2..=integer_square_root(limit) {
            if !prime[divisor] {
                continue;
            }
            for multiple in (divisor * divisor..=limit).step_by(divisor) {
                prime[multiple] = false;
            }
        }
        let mut count = 0usize;
        for value in 2..=limit {
            count += prime[value] as usize;
            if value.is_power_of_two() && value >= 1024 {
                assert!(dusart_lower_pi(value as f64) <= count as f64);
                assert!(count as f64 <= dusart_upper_pi(value as f64));
            }
        }
    }

    fn integer_square_root(value: usize) -> usize {
        let mut root = (value as f64).sqrt() as usize;
        while (root + 1) <= value / (root + 1) {
            root += 1;
        }
        while root > value / root {
            root -= 1;
        }
        root
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
