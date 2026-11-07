use crate::construct::{LocalRule, LogicalConfig};
use crate::conveyor::{GatedHeadRing, GatedHeadRule};
use crate::firing_squad::{
    FiringSquadRule, FIRE, FIRING_ERROR, FIRING_STATE_COUNT, GENERAL, SOLDIER,
};
use crate::prime_worker::{
    candidate_result_code, recycle_candidate_result, worker_candidate_bit, worker_candidate_value,
    worker_is_msb, worker_rule_state_count, PrimeWorker, RollingPrimeWorkerRule,
};

const SOUTH: usize = 1;
const WEST: usize = 3;
const EAST: usize = 4;
const NORTH: usize = 6;

const RECORD_PAD: usize = 1;
const RECORD_ZERO: usize = 2;
const RECORD_ONE: usize = 3;
const RECORD_WAVE: usize = 4;
const RECORD_RETURN_ZERO: usize = 5;
const RECORD_RETURN_ONE: usize = 6;
const RECORD_DONE_ZERO: usize = 7;
const RECORD_DONE_ONE: usize = 8;
const RECORD_DONE_EMPTY: usize = 9;
const RECORD_RESET: usize = 10;
const RECORD_ACK: usize = 11;
const RECORD_STATE_COUNT: usize = 12;
const SLOT_PAD: usize = 1;
const SLOT_ZERO: usize = 2;
const SLOT_ONE: usize = 3;
const SLOT_EMPTY: usize = 4;
const SLOT_ACK: usize = 5;
const SLOT_STATE_COUNT: usize = 6;

pub struct CandidateRecordRule;

impl CandidateRecordRule {
    fn offset() -> usize {
        worker_rule_state_count()
    }

    fn encode(state: usize) -> usize {
        Self::offset() + state
    }

    fn decode(state: usize) -> Option<usize> {
        let raw = state.checked_sub(Self::offset())?;
        (raw > 0 && raw < RECORD_STATE_COUNT).then_some(raw)
    }

    fn data(bit: bool) -> usize {
        if bit {
            RECORD_ONE
        } else {
            RECORD_ZERO
        }
    }

    fn returning(bit: bool) -> usize {
        if bit {
            RECORD_RETURN_ONE
        } else {
            RECORD_RETURN_ZERO
        }
    }

    fn done(bit: bool) -> usize {
        if bit {
            RECORD_DONE_ONE
        } else {
            RECORD_DONE_ZERO
        }
    }

    fn bit(state: usize) -> Option<bool> {
        match state {
            RECORD_ZERO | RECORD_RETURN_ZERO | RECORD_DONE_ZERO => Some(false),
            RECORD_ONE | RECORD_RETURN_ONE | RECORD_DONE_ONE => Some(true),
            _ => None,
        }
    }

    fn is_returning(state: usize) -> bool {
        matches!(state, RECORD_RETURN_ZERO | RECORD_RETURN_ONE)
    }
}

impl LocalRule for CandidateRecordRule {
    fn state_count(&self) -> usize {
        worker_rule_state_count() + RECORD_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center > 0 && center <= worker_rule_state_count() {
            return center;
        }
        let Some(record) = Self::decode(center) else {
            return 0;
        };
        let next = match record {
            RECORD_PAD => {
                if Self::decode(neighbours[WEST]) == Some(RECORD_RESET) {
                    RECORD_RESET
                } else if Self::decode(neighbours[WEST]) == Some(RECORD_WAVE)
                    && worker_candidate_bit(neighbours[SOUTH]).is_some()
                {
                    RECORD_WAVE
                } else {
                    RECORD_PAD
                }
            }
            RECORD_WAVE => {
                let bit = worker_candidate_bit(neighbours[SOUTH]).unwrap_or(false);
                if worker_is_msb(neighbours[SOUTH]) {
                    Self::returning(bit)
                } else {
                    Self::data(bit)
                }
            }
            RECORD_ZERO | RECORD_ONE => {
                if Self::decode(neighbours[WEST]) == Some(RECORD_RESET) {
                    RECORD_RESET
                } else if Self::decode(neighbours[EAST])
                    .map(Self::is_returning)
                    .unwrap_or(false)
                {
                    Self::returning(Self::bit(record).unwrap())
                } else {
                    record
                }
            }
            RECORD_RETURN_ZERO | RECORD_RETURN_ONE => {
                if worker_candidate_bit(neighbours[SOUTH]).is_some()
                    && Self::decode(neighbours[WEST]).is_none()
                {
                    Self::done(Self::bit(record).unwrap())
                } else {
                    Self::data(Self::bit(record).unwrap())
                }
            }
            RECORD_DONE_ZERO | RECORD_DONE_ONE | RECORD_DONE_EMPTY => record,
            RECORD_RESET => RECORD_PAD,
            RECORD_ACK => RECORD_RESET,
            _ => RECORD_DONE_EMPTY,
        };
        Self::encode(next)
    }
}

pub struct RollingRecordedWorkerRule;

impl LocalRule for RollingRecordedWorkerRule {
    fn state_count(&self) -> usize {
        CandidateRecordRule.state_count()
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 {
            if worker_candidate_bit(neighbours[SOUTH]).is_some()
                && CandidateRecordRule::decode(neighbours[WEST]).is_some()
            {
                return CandidateRecordRule::encode(RECORD_PAD);
            }
            let worker_states = worker_rule_state_count();
            let mut local = [0; 8];
            for slot in 0..8 {
                if neighbours[slot] <= worker_states {
                    local[slot] = neighbours[slot];
                }
            }
            return RollingPrimeWorkerRule.next(0, local);
        }
        if let Some(record) = CandidateRecordRule::decode(center) {
            let result = candidate_result_code(neighbours[SOUTH]);
            if record == RECORD_PAD && CandidateRecordRule::decode(neighbours[WEST]).is_none() {
                return match result {
                    Some(false) => CandidateRecordRule::encode(RECORD_WAVE),
                    Some(true) => CandidateRecordRule::encode(RECORD_DONE_EMPTY),
                    None => CandidateRecordRule.next(center, neighbours),
                };
            }
            if matches!(
                record,
                RECORD_DONE_ZERO | RECORD_DONE_ONE | RECORD_DONE_EMPTY
            ) && result.is_some()
            {
                return CandidateRecordRule::encode(RECORD_RESET);
            }
            return CandidateRecordRule.next(center, neighbours);
        }
        if let Some(composite) = candidate_result_code(center) {
            let record = CandidateRecordRule::decode(neighbours[NORTH]);
            let acknowledged = if composite {
                record == Some(RECORD_DONE_EMPTY)
            } else {
                matches!(record, Some(RECORD_DONE_ZERO | RECORD_DONE_ONE))
            };
            return if acknowledged {
                recycle_candidate_result(center).unwrap()
            } else {
                center
            };
        }

        let worker_states = worker_rule_state_count();
        let center = if center <= worker_states { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            if neighbours[slot] <= worker_states {
                local[slot] = neighbours[slot];
            }
        }
        RollingPrimeWorkerRule.next(center, local)
    }
}

pub struct FamilyStationRule;

impl FamilyStationRule {
    fn head_offset() -> usize {
        CandidateRecordRule.state_count()
    }

    fn encode_head(state: usize) -> usize {
        if state == 0 {
            0
        } else {
            Self::head_offset() + state - 1
        }
    }

    fn decode_head(state: usize) -> Option<usize> {
        if state == 0 {
            Some(0)
        } else {
            state
                .checked_sub(Self::head_offset())
                .map(|state| state + 1)
                .filter(|state| *state < GatedHeadRule.state_count())
        }
    }

    fn sync_offset() -> usize {
        Self::head_offset() + GatedHeadRule.state_count() - 1
    }

    fn encode_sync(state: usize) -> usize {
        Self::sync_offset() + state - 1
    }

    fn decode_sync(state: usize) -> Option<usize> {
        let raw = state.checked_sub(Self::sync_offset())? + 1;
        (raw <= FIRING_STATE_COUNT + 4).then_some(raw)
    }

    fn emit_zero() -> usize {
        FIRING_STATE_COUNT
    }

    fn emit_one() -> usize {
        FIRING_STATE_COUNT + 1
    }

    fn emit_empty() -> usize {
        FIRING_STATE_COUNT + 2
    }

    fn sync_ack() -> usize {
        FIRING_STATE_COUNT + 3
    }

    fn sync_held() -> usize {
        FIRING_STATE_COUNT + 4
    }

    fn slot_offset() -> usize {
        Self::sync_offset() + FIRING_STATE_COUNT + 4
    }

    fn encode_slot(state: usize) -> usize {
        Self::slot_offset() + state - 1
    }

    fn decode_slot(state: usize) -> Option<usize> {
        let raw = state.checked_sub(Self::slot_offset())? + 1;
        (raw > 0 && raw < SLOT_STATE_COUNT).then_some(raw)
    }

    fn slot_ready(state: usize) -> bool {
        matches!(
            Self::decode_slot(state),
            Some(SLOT_ZERO | SLOT_ONE | SLOT_EMPTY)
        )
    }

    fn record_done(state: usize) -> bool {
        matches!(
            CandidateRecordRule::decode(state),
            Some(RECORD_DONE_ZERO | RECORD_DONE_ONE | RECORD_DONE_EMPTY)
        )
    }
}

impl LocalRule for FamilyStationRule {
    fn state_count(&self) -> usize {
        Self::slot_offset() + SLOT_STATE_COUNT - 1
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if let Some(slot) = Self::decode_slot(center) {
            if Self::decode_head(neighbours[NORTH])
                .map(GatedHeadRule::gate_is_acknowledged)
                .unwrap_or(false)
            {
                return Self::encode_slot(SLOT_ACK);
            }
            if slot == SLOT_ACK {
                return Self::encode_slot(SLOT_PAD);
            }
            let loaded = match Self::decode_sync(neighbours[SOUTH]) {
                Some(state) if state == Self::emit_zero() => Some(SLOT_ZERO),
                Some(state) if state == Self::emit_one() => Some(SLOT_ONE),
                Some(state) if state == Self::emit_empty() => Some(SLOT_EMPTY),
                _ => None,
            };
            if let Some(loaded) = loaded {
                return Self::encode_slot(loaded);
            }
            return Self::encode_slot(Self::decode_slot(neighbours[WEST]).unwrap_or(SLOT_PAD));
        }

        if let Some(sync) = Self::decode_sync(center) {
            if Self::decode_slot(neighbours[NORTH]) == Some(SLOT_ACK) {
                return Self::encode_sync(Self::sync_ack());
            }
            if sync == Self::sync_ack() {
                return Self::encode_sync(SOLDIER);
            }
            if sync == Self::sync_held() {
                return center;
            }
            if sync == Self::emit_zero() || sync == Self::emit_one() {
                return Self::encode_sync(SOLDIER);
            }
            if sync == Self::emit_empty() {
                return Self::encode_sync(Self::sync_held());
            }
            if sync == FIRE {
                let bit = CandidateRecordRule::decode(neighbours[SOUTH])
                    .and_then(CandidateRecordRule::bit)
                    .unwrap_or(false);
                return Self::encode_sync(if bit {
                    Self::emit_one()
                } else {
                    Self::emit_zero()
                });
            }
            if sync == SOLDIER
                && CandidateRecordRule::decode(neighbours[SOUTH]) == Some(RECORD_DONE_EMPTY)
            {
                return Self::encode_sync(Self::emit_empty());
            }
            if sync == SOLDIER
                && CandidateRecordRule::decode(neighbours[SOUTH])
                    .map(CandidateRecordRule::is_returning)
                    .unwrap_or(false)
                && Self::decode_sync(neighbours[EAST]).is_none()
            {
                return Self::encode_sync(GENERAL);
            }
            let mut local = [0; 8];
            for slot in 0..8 {
                local[slot] = Self::decode_sync(neighbours[slot])
                    .map(|state| {
                        if state >= FIRING_STATE_COUNT {
                            SOLDIER
                        } else {
                            state
                        }
                    })
                    .unwrap_or(0);
            }
            if sync == SOLDIER
                && matches!(local[WEST], 0 | SOLDIER)
                && matches!(local[EAST], 0 | SOLDIER)
            {
                return center;
            }
            return Self::encode_sync(FiringSquadRule.next(sync, local));
        }

        if center >= Self::head_offset() {
            let raw = Self::decode_head(center).unwrap();
            if GatedHeadRule::gate_is_waiting(raw) {
                if let Some(slot) = GatedHeadRule::gate_station_slot(raw) {
                    if Self::slot_ready(neighbours[slot]) {
                        return Self::encode_head(GatedHeadRule::rearm_gate(raw).unwrap());
                    }
                }
            }
            let mut local = [0; 8];
            for slot in 0..8 {
                local[slot] = Self::decode_head(neighbours[slot]).unwrap_or(0);
            }
            return Self::encode_head(GatedHeadRule.next(raw, local));
        }

        if center == 0 {
            if CandidateRecordRule::decode(neighbours[SOUTH]).is_some()
                && Self::decode_sync(neighbours[WEST]).is_some()
            {
                return Self::encode_sync(SOLDIER);
            }
            let worker_states = worker_rule_state_count();
            if worker_candidate_bit(neighbours[SOUTH]).is_some()
                && CandidateRecordRule::decode(neighbours[WEST]).is_some()
            {
                return CandidateRecordRule::encode(RECORD_PAD);
            }
            let mut local = [0; 8];
            for slot in 0..8 {
                if neighbours[slot] <= worker_states {
                    local[slot] = neighbours[slot];
                }
            }
            return RollingPrimeWorkerRule.next(0, local);
        }

        if let Some(record) = CandidateRecordRule::decode(center) {
            let acknowledged = Self::decode_sync(neighbours[NORTH]) == Some(Self::sync_ack());
            if Self::record_done(center) && acknowledged {
                return CandidateRecordRule::encode(RECORD_ACK);
            }
            if record == RECORD_ACK {
                return CandidateRecordRule::encode(RECORD_RESET);
            }
            let result = candidate_result_code(neighbours[SOUTH]);
            if record == RECORD_PAD && CandidateRecordRule::decode(neighbours[WEST]).is_none() {
                return match result {
                    Some(false) => CandidateRecordRule::encode(RECORD_WAVE),
                    Some(true) => CandidateRecordRule::encode(RECORD_DONE_EMPTY),
                    None => CandidateRecordRule.next(center, neighbours),
                };
            }
            return CandidateRecordRule.next(center, neighbours);
        }

        if candidate_result_code(center).is_some() {
            let acknowledged = CandidateRecordRule::decode(neighbours[NORTH]) == Some(RECORD_ACK);
            return if acknowledged {
                recycle_candidate_result(center).unwrap()
            } else {
                center
            };
        }

        let worker_states = worker_rule_state_count();
        let center = if center <= worker_states { center } else { 0 };
        let mut local = [0; 8];
        for slot in 0..8 {
            if neighbours[slot] <= worker_states {
                local[slot] = neighbours[slot];
            }
        }
        RollingPrimeWorkerRule.next(center, local)
    }
}

pub struct CandidateRecordWriter {
    pub config: LogicalConfig,
    pub steps: usize,
    width: usize,
}

pub struct RollingRecordedWorker {
    pub config: LogicalConfig,
    pub steps: usize,
    pub records: Vec<(usize, Option<usize>)>,
}

pub struct FamilyStation {
    pub config: LogicalConfig,
    pub steps: usize,
    pub detached: Vec<Option<usize>>,
    candidate_origin: (i32, i32),
    initial_candidate: usize,
    gate_point: (i32, i32),
}

pub struct PackedCandidateRecordRule {
    payload_bits: usize,
    payload_states: usize,
}

impl PackedCandidateRecordRule {
    pub fn new(payload_bits: usize) -> Result<Self, String> {
        if !(1..=20).contains(&payload_bits) {
            return Err("candidate record chunks must contain between 1 and 20 bits".to_string());
        }
        Ok(PackedCandidateRecordRule {
            payload_bits,
            payload_states: 1usize << payload_bits,
        })
    }

    fn offset(&self) -> usize {
        CandidateRecordRule.state_count()
    }

    fn pad(&self) -> usize {
        self.offset() + 1
    }

    fn wait(&self) -> usize {
        self.offset() + 2
    }

    fn head_base(&self) -> usize {
        self.offset() + 3
    }

    fn head(&self, count: usize, payload: usize) -> Result<usize, String> {
        if count == 0 || count >= self.payload_bits || payload >= self.payload_states {
            return Err("invalid candidate record packer head".to_string());
        }
        Ok(self.head_base() + (count - 1) * self.payload_states + payload)
    }

    fn decode_head(&self, state: usize) -> Option<(usize, usize)> {
        let raw = state.checked_sub(self.head_base())?;
        let head_states = (self.payload_bits - 1) * self.payload_states;
        if raw >= head_states {
            return None;
        }
        Some((raw / self.payload_states + 1, raw % self.payload_states))
    }

    fn chunk_base(&self) -> usize {
        self.head_base() + (self.payload_bits - 1) * self.payload_states
    }

    fn chunk(&self, payload: usize) -> Result<usize, String> {
        if payload >= self.payload_states {
            return Err("candidate record chunk payload is too large".to_string());
        }
        Ok(self.chunk_base() + payload)
    }

    fn decode_chunk(&self, state: usize) -> Option<usize> {
        let raw = state.checked_sub(self.chunk_base())?;
        (raw < self.payload_states).then_some(raw)
    }
}

impl LocalRule for PackedCandidateRecordRule {
    fn state_count(&self) -> usize {
        self.chunk_base() + self.payload_states
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center < self.offset() {
            let mut local = [0; 8];
            for slot in 0..8 {
                if neighbours[slot] < self.offset() {
                    local[slot] = neighbours[slot];
                }
            }
            return CandidateRecordRule.next(center, local);
        }
        if center == self.pad() {
            let Some((count, payload)) = self.decode_head(neighbours[WEST]) else {
                return center;
            };
            let bit = CandidateRecordRule::decode(neighbours[SOUTH])
                .and_then(CandidateRecordRule::bit)
                .unwrap_or(false);
            let count = count + 1;
            let payload = payload | ((bit as usize) << (count - 1));
            return if count == self.payload_bits {
                self.chunk(payload).unwrap()
            } else {
                self.head(count, payload).unwrap()
            };
        }
        if center == self.wait() {
            let Some(record) = CandidateRecordRule::decode(neighbours[SOUTH]) else {
                return center;
            };
            if !CandidateRecordRule::is_returning(record) {
                return center;
            }
            let payload = CandidateRecordRule::bit(record).unwrap_or(false) as usize;
            return if self.payload_bits == 1 {
                self.chunk(payload).unwrap()
            } else {
                self.head(1, payload).unwrap()
            };
        }
        if self.decode_head(center).is_some() {
            return self.pad();
        }
        if self.decode_chunk(center).is_some() {
            return center;
        }
        0
    }
}

pub struct PackedCandidateRecordWriter {
    pub config: LogicalConfig,
    pub steps: usize,
    width: usize,
    payload_bits: usize,
    prime: bool,
    rule: PackedCandidateRecordRule,
}

impl CandidateRecordWriter {
    pub fn new(candidate: usize, prime: bool) -> Result<Self, String> {
        let worker = PrimeWorker::new(candidate)?;
        let width = worker.config.len();
        let mut config = worker.config;
        for x in 0..width {
            let state = if x == 0 {
                if prime {
                    RECORD_WAVE
                } else {
                    RECORD_DONE_EMPTY
                }
            } else {
                RECORD_PAD
            };
            config.insert((x as i32, 1), CandidateRecordRule::encode(state));
        }
        Ok(CandidateRecordWriter {
            config,
            steps: 0,
            width,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = CandidateRecordRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn done(&self) -> bool {
        matches!(
            self.config
                .get(&(0, 1))
                .and_then(|state| CandidateRecordRule::decode(*state)),
            Some(RECORD_DONE_ZERO | RECORD_DONE_ONE | RECORD_DONE_EMPTY)
        )
    }

    pub fn run_until_done(&mut self, max_steps: usize) -> Result<(), String> {
        while !self.done() && self.steps < max_steps {
            self.step()?;
        }
        if !self.done() {
            return Err(format!(
                "candidate record writer did not finish in {} steps",
                max_steps
            ));
        }
        Ok(())
    }

    pub fn value(&self) -> Result<Option<usize>, String> {
        if self
            .config
            .get(&(0, 1))
            .and_then(|state| CandidateRecordRule::decode(*state))
            == Some(RECORD_DONE_EMPTY)
        {
            return Ok(None);
        }
        let mut value = 0usize;
        for bit in 0..self.width {
            let state = self
                .config
                .get(&(bit as i32, 1))
                .and_then(|state| CandidateRecordRule::decode(*state))
                .ok_or_else(|| format!("candidate record lost bit {}", bit))?;
            value |= (CandidateRecordRule::bit(state)
                .ok_or_else(|| format!("candidate record bit {} is not settled", bit))?
                as usize)
                << bit;
        }
        Ok(Some(value))
    }
}

impl RollingRecordedWorker {
    pub fn new(candidate: usize) -> Result<Self, String> {
        let width = PrimeWorker::new(candidate)?.config.len();
        Self::with_width(candidate, width)
    }

    pub fn with_width(candidate: usize, width: usize) -> Result<Self, String> {
        let worker = PrimeWorker::with_width(candidate, width)?;
        let width = worker.config.len();
        let mut config = worker.config;
        for x in 0..width {
            config.insert((x as i32, 1), CandidateRecordRule::encode(RECORD_PAD));
        }
        Ok(RollingRecordedWorker {
            config,
            steps: 0,
            records: Vec::new(),
        })
    }

    fn settled_record(&self) -> Result<Option<Option<usize>>, String> {
        let Some(record) = self
            .config
            .get(&(0, 1))
            .and_then(|state| CandidateRecordRule::decode(*state))
        else {
            return Ok(None);
        };
        if record == RECORD_DONE_EMPTY {
            return Ok(Some(None));
        }
        if !matches!(record, RECORD_DONE_ZERO | RECORD_DONE_ONE) {
            return Ok(None);
        }
        let mut value = 0usize;
        for ((x, y), state) in &self.config {
            if *y != 1 || *x < 0 || *x >= usize::BITS as i32 {
                continue;
            }
            let record = CandidateRecordRule::decode(*state)
                .ok_or_else(|| "rolling record contains an invalid state".to_string())?;
            let bit = CandidateRecordRule::bit(record)
                .ok_or_else(|| "rolling prime record is not settled".to_string())?;
            value |= (bit as usize) << *x;
        }
        Ok(Some(Some(value)))
    }

    pub fn step(&mut self) -> Result<(), String> {
        if let Some(record) = self.settled_record()? {
            let candidate = worker_candidate_value(&self.config)
                .ok_or_else(|| "rolling record worker lost its candidate".to_string())?;
            self.records.push((candidate, record));
        }
        self.config = RollingRecordedWorkerRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn run_until_records(&mut self, count: usize, max_steps: usize) -> Result<(), String> {
        while self.records.len() < count && self.steps < max_steps {
            self.step()?;
        }
        if self.records.len() != count {
            return Err(format!(
                "rolling record worker emitted {} of {} records in {} steps; candidate {:?}, result {:?}, record {:?}",
                self.records.len(),
                count,
                max_steps,
                worker_candidate_value(&self.config),
                self.config
                    .get(&(0, 0))
                    .and_then(|state| candidate_result_code(*state)),
                self.config
                    .get(&(0, 1))
                    .and_then(|state| CandidateRecordRule::decode(*state))
            ));
        }
        Ok(())
    }
}

impl FamilyStation {
    pub fn new(candidate: usize) -> Result<Self, String> {
        let width = PrimeWorker::new(candidate)?.config.len();
        Self::with_width(candidate, width)
    }

    pub fn with_width(candidate: usize, width: usize) -> Result<Self, String> {
        let mut ring = GatedHeadRing::new_with_stations(4, 4, &[0])?;
        ring.set_all_gates_waiting();
        let (gate_point, station) = ring.station_site(0)?;
        let candidate_origin = (station.0, station.1 - 3);
        let worker = RollingRecordedWorker::with_width(candidate, width)?;
        let width = worker.config.keys().filter(|(_, y)| *y == 0).count();
        let mut config = LogicalConfig::new();
        for (point, state) in ring.config {
            config.insert(point, FamilyStationRule::encode_head(state));
        }
        for ((x, y), state) in worker.config {
            let point = (candidate_origin.0 + x, candidate_origin.1 + y);
            if config.insert(point, state).is_some() {
                return Err(format!("family station geometry overlaps at {:?}", point));
            }
        }
        for x in 0..width {
            config.insert(
                (candidate_origin.0 + x as i32, candidate_origin.1 + 2),
                FamilyStationRule::encode_sync(SOLDIER),
            );
        }
        for x in 0..width + 128 {
            config.insert(
                (candidate_origin.0 + x as i32, candidate_origin.1 + 3),
                FamilyStationRule::encode_slot(SLOT_PAD),
            );
        }
        Ok(FamilyStation {
            config,
            steps: 0,
            detached: Vec::new(),
            candidate_origin,
            initial_candidate: candidate,
            gate_point,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        let slot_origin = (self.candidate_origin.0, self.candidate_origin.1 + 3);
        match self
            .config
            .get(&slot_origin)
            .and_then(|state| FamilyStationRule::decode_slot(*state))
        {
            Some(SLOT_EMPTY) => self.detached.push(None),
            Some(SLOT_ZERO | SLOT_ONE) => {
                let mut value = 0usize;
                for bit in 0..usize::BITS as usize {
                    let Some(slot) = self
                        .config
                        .get(&(slot_origin.0 + bit as i32, slot_origin.1))
                        .and_then(|state| FamilyStationRule::decode_slot(*state))
                    else {
                        break;
                    };
                    match slot {
                        SLOT_ZERO => {}
                        SLOT_ONE => value |= 1usize << bit,
                        _ => break,
                    }
                }
                self.detached.push(Some(value));
            }
            _ => {}
        }
        self.config = FamilyStationRule.step(&self.config)?;
        self.steps += 1;
        if let Some(point) = self.config.iter().find_map(|(point, state)| {
            (FamilyStationRule::decode_sync(*state) == Some(FIRING_ERROR)).then_some(*point)
        }) {
            return Err(format!(
                "family station synchronizer failed at {:?} on step {}",
                point, self.steps
            ));
        }
        Ok(())
    }

    pub fn candidate(&self) -> Option<usize> {
        let mut value = 0usize;
        let mut occupied = false;
        for ((x, y), state) in &self.config {
            if *y != self.candidate_origin.1 || *x < self.candidate_origin.0 {
                continue;
            }
            let bit = worker_candidate_bit(*state)?;
            let offset = usize::try_from(*x - self.candidate_origin.0).ok()?;
            if offset >= usize::BITS as usize {
                return None;
            }
            value |= (bit as usize) << offset;
            occupied = true;
        }
        occupied.then_some(value)
    }

    pub fn run_until_recycled(&mut self, max_steps: usize) -> Result<(), String> {
        let target = self
            .initial_candidate
            .checked_add(1)
            .ok_or_else(|| "family station candidate overflowed".to_string())?;
        while self.candidate() != Some(target) && self.steps < max_steps {
            self.step()?;
        }
        if self.candidate() != Some(target) {
            return Err(format!(
                "family station did not recycle candidate {} in {} steps; current {:?}",
                self.initial_candidate,
                max_steps,
                self.candidate()
            ));
        }
        Ok(())
    }

    pub fn run_until_candidate(&mut self, target: usize, max_steps: usize) -> Result<(), String> {
        while self.candidate() != Some(target) && self.steps < max_steps {
            self.step()?;
        }
        if self.candidate() != Some(target) {
            return Err(format!(
                "family station did not reach candidate {} in {} steps; current {:?}, result {:?}, record {:?}, sync {:?}, slot {:?}, gate {:?}",
                target,
                max_steps,
                self.candidate(),
                self.config
                    .get(&self.candidate_origin)
                    .and_then(|state| candidate_result_code(*state)),
                self.config
                    .get(&(self.candidate_origin.0, self.candidate_origin.1 + 1))
                    .and_then(|state| CandidateRecordRule::decode(*state)),
                self.config
                    .get(&(self.candidate_origin.0, self.candidate_origin.1 + 2))
                    .and_then(|state| FamilyStationRule::decode_sync(*state)),
                self.config
                    .get(&(self.candidate_origin.0, self.candidate_origin.1 + 3))
                    .and_then(|state| FamilyStationRule::decode_slot(*state)),
                self.config
                    .get(&self.gate_point)
                    .and_then(|state| FamilyStationRule::decode_head(*state))
            ));
        }
        Ok(())
    }
}

impl PackedCandidateRecordWriter {
    pub fn new(candidate: usize, prime: bool, payload_bits: usize) -> Result<Self, String> {
        let rule = PackedCandidateRecordRule::new(payload_bits)?;
        let base_width = PrimeWorker::new(candidate)?.config.len();
        let width = base_width.div_ceil(payload_bits) * payload_bits;
        let worker = PrimeWorker::with_width(candidate, width)?;
        let mut config = worker.config;
        for x in 0..width {
            let record = if x == 0 {
                if prime {
                    RECORD_WAVE
                } else {
                    RECORD_DONE_EMPTY
                }
            } else {
                RECORD_PAD
            };
            config.insert((x as i32, 1), CandidateRecordRule::encode(record));
            config.insert(
                (x as i32, 2),
                if x % payload_bits == 0 {
                    rule.wait()
                } else {
                    rule.pad()
                },
            );
        }
        Ok(PackedCandidateRecordWriter {
            config,
            steps: 0,
            width,
            payload_bits,
            prime,
            rule,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = self.rule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn done(&self) -> bool {
        if !self.prime {
            return self
                .config
                .get(&(0, 1))
                .and_then(|state| CandidateRecordRule::decode(*state))
                == Some(RECORD_DONE_EMPTY);
        }
        (0..self.width / self.payload_bits).all(|chunk| {
            let x = (chunk + 1) * self.payload_bits - 1;
            self.config
                .get(&(x as i32, 2))
                .and_then(|state| self.rule.decode_chunk(*state))
                .is_some()
        })
    }

    pub fn run_until_done(&mut self, max_steps: usize) -> Result<(), String> {
        while !self.done() && self.steps < max_steps {
            self.step()?;
        }
        if !self.done() {
            return Err(format!(
                "packed candidate record writer did not finish in {} steps",
                max_steps
            ));
        }
        Ok(())
    }

    pub fn chunks(&self) -> Result<Option<Vec<usize>>, String> {
        if !self.prime {
            return Ok(None);
        }
        (0..self.width / self.payload_bits)
            .map(|chunk| {
                let x = (chunk + 1) * self.payload_bits - 1;
                self.config
                    .get(&(x as i32, 2))
                    .and_then(|state| self.rule.decode_chunk(*state))
                    .ok_or_else(|| format!("packed candidate record lost chunk {}", chunk))
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    fn is_prime(candidate: usize) -> bool {
        candidate >= 2
            && (2..)
                .take_while(|divisor| divisor * divisor <= candidate)
                .all(|divisor| candidate % divisor != 0)
    }

    #[test]
    fn prime_candidate_tapes_copy_into_ordered_record_rows() {
        for candidate in 2..=128 {
            let prime = is_prime(candidate);
            let mut writer = CandidateRecordWriter::new(candidate, prime).unwrap();
            let active = writer.config.len();
            writer.run_until_done(1_000).unwrap();
            assert_eq!(writer.config.len(), active);
            assert_eq!(writer.value().unwrap(), prime.then_some(candidate));
        }
    }

    #[test]
    fn record_copy_rule_compiles_to_constant_weight_binary_blocks() {
        let writer = CandidateRecordWriter::new(29, true).unwrap();
        let machine = BlockMachine::new(CandidateRecordRule).unwrap();
        let binary = machine.codec.encode(&writer.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&writer.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn settled_candidate_records_pack_into_queue_chunks_locally() {
        for candidate in 2..=128 {
            let prime = is_prime(candidate);
            let mut writer = PackedCandidateRecordWriter::new(candidate, prime, 4).unwrap();
            let active = writer.config.len();
            writer.run_until_done(2_000).unwrap();
            assert_eq!(writer.config.len(), active);
            let chunks = writer.chunks().unwrap();
            if prime {
                let value = chunks
                    .unwrap()
                    .into_iter()
                    .enumerate()
                    .fold(0usize, |value, (chunk, payload)| {
                        value | (payload << (4 * chunk))
                    });
                assert_eq!(value, candidate);
            } else {
                assert_eq!(chunks, None);
            }
        }
    }

    #[test]
    fn twenty_bit_record_packer_alphabet_compiles() {
        let writer = PackedCandidateRecordWriter::new(29, true, 20).unwrap();
        let machine = BlockMachine::new(PackedCandidateRecordRule::new(20).unwrap()).unwrap();
        assert!(machine.codec.width > 20_000_000);
        let binary = machine.codec.encode(&writer.config).unwrap();
        assert_eq!(machine.codec.decode(&binary).unwrap(), writer.config);
    }

    #[test]
    fn rolling_worker_classifies_copies_and_recycles_under_one_rule() {
        let start = 15usize;
        let count = 26usize;
        let mut worker = RollingRecordedWorker::new(start).unwrap();
        worker.run_until_records(count, 2_000_000).unwrap();
        assert_eq!(
            worker
                .records
                .iter()
                .map(|record| record.0)
                .collect::<Vec<_>>(),
            (start..start + count).collect::<Vec<_>>()
        );
        for &(candidate, record) in &worker.records {
            assert_eq!(record, is_prime(candidate).then_some(candidate));
        }
        let worker_width = worker.config.keys().filter(|(_, y)| *y == 0).count();
        let record_width = worker.config.keys().filter(|(_, y)| *y == 1).count();
        assert_eq!(worker_width, record_width);
    }

    #[test]
    fn rolling_record_product_compiles_to_binary_blocks() {
        let worker = RollingRecordedWorker::new(29).unwrap();
        let machine = BlockMachine::new(RollingRecordedWorkerRule).unwrap();
        let binary = machine.codec.encode(&worker.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&worker.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn gated_ring_acknowledges_record_before_worker_recycle() {
        let mut station = FamilyStation::with_width(29, 8).unwrap();
        let active = station.config.len();
        while station.candidate() != Some(34) {
            station.step().unwrap();
            assert_eq!(station.config.len(), active);
            assert!(station.steps < 100_000);
        }
        assert_eq!(station.candidate(), Some(34));
        assert_eq!(station.detached, vec![Some(29), None, Some(31), None, None]);
    }

    #[test]
    fn family_station_product_compiles_to_binary_blocks() {
        let station = FamilyStation::new(29).unwrap();
        let machine = BlockMachine::new(FamilyStationRule).unwrap();
        let binary = machine.codec.encode(&station.config).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&station.config).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }
}
