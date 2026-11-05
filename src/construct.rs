/*
* Constructive population machinery
* =================================
*
* The search tools ask for a binary rule directly.  The constructive route is easier to
* reason about in two layers:
*
*   1. a finite-state logical CA, and
*   2. a binary block code whose ordinary active states have one fixed Hamming weight.
*
* If the logical machine accounts for the population in weighted blocks, every logical state
* change is population-neutral.  Only explicitly scheduled block splits and the bounded tail
* transition change the binary population.  This module implements and verifies that invariant.
*
* BlockCodec is deliberately a macro simulator, not a claim that any host-side prime oracle is a
* CA rule.  Its finite local radius is the witness for the standard block compilation: every
* emitted bit reads only the previous code block and its Moore neighbours.  The scheduler that
* prepares a prime-gap split mask remains a separate proof obligation.
*/

use std::collections::{BTreeMap, BTreeSet};

use crate::sequences::primes_target;

pub type LogicalPoint = (i32, i32);
pub type LogicalConfig = BTreeMap<LogicalPoint, usize>;
pub type BinaryConfig = BTreeSet<LogicalPoint>;

const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// A finite-state, radius-one logical CA.  State zero is quiescence.
pub trait LocalRule {
    fn state_count(&self) -> usize;

    fn next(&self, center: usize, neighbours: [usize; NEIGHBOURS.len()]) -> usize;

    fn step(&self, current: &LogicalConfig) -> Result<LogicalConfig, String> {
        let mut candidates = current.keys().copied().collect::<BTreeSet<_>>();
        for (x, y) in current.keys() {
            for (dx, dy) in NEIGHBOURS {
                let point = (
                    x.checked_add(dx)
                        .ok_or_else(|| "logical neighbourhood exceeds the grid".to_string())?,
                    y.checked_add(dy)
                        .ok_or_else(|| "logical neighbourhood exceeds the grid".to_string())?,
                );
                candidates.insert(point);
            }
        }

        let mut next = LogicalConfig::new();
        for point @ (x, y) in candidates {
            let centre = current.get(&point).copied().unwrap_or(0);
            let mut nearby = [0; NEIGHBOURS.len()];
            for (slot, (dx, dy)) in NEIGHBOURS.iter().enumerate() {
                let neighbour = (
                    x.checked_add(*dx)
                        .ok_or_else(|| "logical neighbourhood exceeds the grid".to_string())?,
                    y.checked_add(*dy)
                        .ok_or_else(|| "logical neighbourhood exceeds the grid".to_string())?,
                );
                nearby[slot] = current.get(&neighbour).copied().unwrap_or(0);
            }
            let state = self.next(centre, nearby);
            if state >= self.state_count() {
                return Err(format!(
                    "logical rule returned state {} but only has {} states",
                    state,
                    self.state_count()
                ));
            }
            if state != 0 {
                next.insert(point, state);
            }
        }
        Ok(next)
    }
}

/// A fixed frame plus one payload bit is a self-synchronising binary code for a logical state.
///
/// The three frame bits have the asymmetric pattern `101 / 010`, followed by a payload row
/// with one bit.  Guards and a two-cell gap separate adjacent blocks, so valid active blocks
/// cannot overlap or be read at another alignment.  Every active state has Hamming weight four.
#[derive(Clone, Debug)]
pub struct BlockCodec {
    state_count: usize,
    ordinary_state_count: usize,
    tail_state_count: usize,
    pub width: i32,
    pub height: i32,
    pub stride_x: i32,
    pub stride_y: i32,
}

impl BlockCodec {
    pub fn new(state_count: usize) -> Result<Self, String> {
        Self::with_tail_states(state_count, 0)
    }

    pub fn with_tail_states(
        ordinary_state_count: usize,
        tail_state_count: usize,
    ) -> Result<Self, String> {
        if tail_state_count > 4 {
            return Err("the weight-four frame has only four tail residue states".to_string());
        }
        let state_count = ordinary_state_count
            .checked_add(tail_state_count)
            .ok_or_else(|| "too many logical states".to_string())?;
        if state_count < 2 {
            return Err(
                "a block codec needs quiescence plus at least one active state".to_string(),
            );
        }
        let width = i32::try_from(
            state_count
                .checked_add(3)
                .ok_or_else(|| "too many logical states".to_string())?,
        )
        .map_err(|_| "too many logical states".to_string())?;
        if width > 100_000 {
            return Err("the requested block width is unreasonably large".to_string());
        }
        Ok(BlockCodec {
            state_count,
            ordinary_state_count,
            tail_state_count,
            width,
            height: 4,
            stride_x: width + 2,
            stride_y: 6,
        })
    }

    pub fn state_count(&self) -> usize {
        self.state_count
    }

    pub fn active_weight(&self) -> usize {
        4
    }

    /// A local binary rule compiling a 3x3 logical neighbourhood cannot need more than this.
    pub fn radius(&self) -> i32 {
        2 * self.stride_x.max(self.stride_y) + self.width.max(self.height)
    }

    fn codeword(&self, state: usize) -> Result<BTreeSet<LogicalPoint>, String> {
        if state == 0 {
            return Ok(BTreeSet::new());
        }
        if state >= self.state_count {
            return Err(format!(
                "state {} is not in the codec alphabet of {} states",
                state, self.state_count
            ));
        }
        if state < self.ordinary_state_count {
            return Ok(BTreeSet::from([
                (0, 0),
                (2, 0),
                (1, 1),
                (state as i32 + 2, 3),
            ]));
        }
        let residue = state - self.ordinary_state_count;
        if residue >= self.tail_state_count {
            return Err(format!(
                "tail residue {} is outside the codec alphabet",
                residue
            ));
        }
        self.tail_codeword(residue)
    }

    /// A tail state keeps the same self-synchronising frame and adds `residue` payload bits.
    pub fn tail_codeword(&self, residue: usize) -> Result<BTreeSet<LogicalPoint>, String> {
        if residue >= self.active_weight() {
            return Err(format!(
                "tail residue {} does not fit a weight-{} block",
                residue,
                self.active_weight()
            ));
        }
        let mut code = BTreeSet::from([(0, 0), (2, 0), (1, 1), (0, 2)]);
        for x in 0..residue {
            code.insert((x as i32, 3));
        }
        Ok(code)
    }

    pub fn anchor(&self, point: LogicalPoint) -> Result<LogicalPoint, String> {
        let x = point
            .0
            .checked_mul(self.stride_x)
            .ok_or_else(|| format!("logical x coordinate {:?} is too large", point.0))?;
        let y = point
            .1
            .checked_mul(self.stride_y)
            .ok_or_else(|| format!("logical y coordinate {:?} is too large", point.1))?;
        x.checked_add(self.width - 1)
            .and_then(|_| y.checked_add(self.height - 1))
            .ok_or_else(|| format!("code block at logical point {:?} exceeds the grid", point))?;
        Ok((x, y))
    }

    pub fn encode(&self, logical: &LogicalConfig) -> Result<BinaryConfig, String> {
        let mut binary = BinaryConfig::new();
        for (&point, &state) in logical {
            let anchor = self.anchor(point)?;
            for (x, y) in self.codeword(state)? {
                let physical = (
                    anchor
                        .0
                        .checked_add(x)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                    anchor
                        .1
                        .checked_add(y)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                );
                if !binary.insert(physical) {
                    return Err(format!("overlapping block codewords at {:?}", point));
                }
            }
        }
        Ok(binary)
    }

    fn state_at_anchor(
        &self,
        binary: &BinaryConfig,
        anchor: LogicalPoint,
    ) -> Result<Option<usize>, String> {
        let mut occupied = BTreeSet::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let physical = (
                    anchor
                        .0
                        .checked_add(x)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                    anchor
                        .1
                        .checked_add(y)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                );
                if binary.contains(&physical) {
                    occupied.insert((x, y));
                }
            }
        }
        if occupied.is_empty() {
            return Ok(None);
        }
        for state in 1..self.state_count {
            if occupied == self.codeword(state)? {
                return Ok(Some(state));
            }
        }
        Err(format!(
            "invalid binary block at physical anchor {:?}",
            anchor
        ))
    }

    fn matching_state_at_anchor(
        &self,
        binary: &BinaryConfig,
        anchor: LogicalPoint,
    ) -> Result<Option<usize>, String> {
        let mut occupied = BTreeSet::new();
        for y in 0..self.height {
            for x in 0..self.width {
                let physical = (
                    anchor
                        .0
                        .checked_add(x)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                    anchor
                        .1
                        .checked_add(y)
                        .ok_or_else(|| "code block exceeds the physical grid".to_string())?,
                );
                if binary.contains(&physical) {
                    occupied.insert((x, y));
                }
            }
        }
        if occupied.is_empty() {
            return Ok(None);
        }
        for state in 1..self.state_count {
            if occupied == self.codeword(state)? {
                return Ok(Some(state));
            }
        }
        Ok(None)
    }

    fn active_anchors(
        &self,
        binary: &BinaryConfig,
    ) -> Result<BTreeMap<LogicalPoint, usize>, String> {
        let frame = [(0, 0), (2, 0), (1, 1)];
        let mut candidates = BTreeSet::new();
        for &(x, y) in binary {
            for (fx, fy) in frame {
                if let (Some(x), Some(y)) = (x.checked_sub(fx), y.checked_sub(fy)) {
                    candidates.insert((x, y));
                }
            }
        }

        let mut anchors = BTreeMap::new();
        for anchor in candidates {
            if let Some(state) = self.matching_state_at_anchor(binary, anchor)? {
                anchors.insert(anchor, state);
            }
        }
        Ok(anchors)
    }

    pub fn decode(&self, binary: &BinaryConfig) -> Result<LogicalConfig, String> {
        let mut anchors = BTreeSet::new();
        let frame = [(0, 0), (2, 0), (1, 1)];
        for &(x, y) in binary {
            for (fx, fy) in frame {
                if let (Some(x), Some(y)) = (x.checked_sub(fx), y.checked_sub(fy)) {
                    anchors.insert((x, y));
                }
            }
        }

        let mut decoded = LogicalConfig::new();
        for anchor in anchors {
            if anchor.0.rem_euclid(self.stride_x) != 0 || anchor.1.rem_euclid(self.stride_y) != 0 {
                continue;
            }
            if let Some(state) = self.state_at_anchor(binary, anchor)? {
                let point = (anchor.0 / self.stride_x, anchor.1 / self.stride_y);
                if decoded.insert(point, state).is_some() {
                    return Err(format!("block {:?} was decoded more than once", point));
                }
            }
        }

        if self.encode(&decoded)? != *binary {
            return Err(
                "binary configuration contains a bit outside a valid active block".to_string(),
            );
        }
        Ok(decoded)
    }

    pub fn logical_weight(&self, logical: &LogicalConfig) -> Result<usize, String> {
        let mut weight = 0usize;
        for &state in logical.values() {
            if state == 0 || state >= self.state_count {
                return Err(format!("invalid logical state {} in configuration", state));
            }
            weight = weight
                .checked_add(self.codeword(state)?.len())
                .ok_or_else(|| "logical weight is too large to count as usize".to_string())?;
        }
        Ok(weight)
    }
}

/// A compiled block machine.  The isolated physical seed is a one-bit bootstrap; afterwards the
/// rule always decodes and re-encodes whole blocks.
pub struct BlockMachine<R> {
    pub rule: R,
    pub codec: BlockCodec,
}

/// A population-output tile used after the scheduler has prepared a split mask.
///
/// State 1 is an ordinary block.  State 2 is an ordinary block labelled `split`.  In one local
/// transition the parent returns to state 1 and births one state-1 child immediately to its east.
/// Thus this rule can never consult or invent a prime gap; it only consumes one finite-state label.
pub struct PreparedSplitRule;

pub const ORDINARY_STATE: usize = 1;
pub const SPLIT_STATE: usize = 2;

impl LocalRule for PreparedSplitRule {
    fn state_count(&self) -> usize {
        3
    }

    fn next(&self, center: usize, neighbours: [usize; NEIGHBOURS.len()]) -> usize {
        match center {
            0 if neighbours[3] == SPLIT_STATE => ORDINARY_STATE,
            ORDINARY_STATE | SPLIT_STATE => ORDINARY_STATE,
            _ => 0,
        }
    }
}

/// One generation of the complete weighted output gate.
///
/// Ordinary states 1 and 2 are idle/split.  States 3..6 request one of the four tail residues.
/// States 7..10 combine the same request with a split.  States 11..14 are the four framed tail
/// states.  The scheduler's only job is to prepare this finite label set one generation ahead.
pub struct PreparedPrimeOutputRule;

pub const REQUEST_BASE: usize = 3;
pub const SPLIT_REQUEST_BASE: usize = 7;
pub const TAIL_BASE: usize = 11;
pub const TAIL_RESIDUES: usize = 4;
pub const PREPARED_PRIME_STATE_COUNT: usize = TAIL_BASE + TAIL_RESIDUES;

fn requested_residue(state: usize) -> Option<usize> {
    if (REQUEST_BASE..REQUEST_BASE + TAIL_RESIDUES).contains(&state) {
        Some(state - REQUEST_BASE)
    } else if (SPLIT_REQUEST_BASE..SPLIT_REQUEST_BASE + TAIL_RESIDUES).contains(&state) {
        Some(state - SPLIT_REQUEST_BASE)
    } else {
        None
    }
}

fn is_split_state(state: usize) -> bool {
    state == SPLIT_STATE
        || (SPLIT_REQUEST_BASE..SPLIT_REQUEST_BASE + TAIL_RESIDUES).contains(&state)
}

impl LocalRule for PreparedPrimeOutputRule {
    fn state_count(&self) -> usize {
        PREPARED_PRIME_STATE_COUNT
    }

    fn next(&self, center: usize, neighbours: [usize; NEIGHBOURS.len()]) -> usize {
        match center {
            0 if is_split_state(neighbours[3]) => ORDINARY_STATE,
            ORDINARY_STATE..TAIL_BASE => ORDINARY_STATE,
            TAIL_BASE..PREPARED_PRIME_STATE_COUNT => {
                TAIL_BASE + requested_residue(neighbours[1]).unwrap_or(center - TAIL_BASE)
            }
            _ => 0,
        }
    }
}

fn residue_of_tail_weight(tail_weight: usize, ordinary_weight: usize) -> Result<usize, String> {
    if ordinary_weight != 4 {
        return Err("the framed output gate is currently a weight-four compiler".to_string());
    }
    if !(ordinary_weight..2 * ordinary_weight).contains(&tail_weight) {
        return Err(format!(
            "tail weight {} is outside the framed range {}..{}",
            tail_weight,
            ordinary_weight,
            2 * ordinary_weight - 1
        ));
    }
    Ok(tail_weight - ordinary_weight)
}

/// Build the finite label pattern consumed by `PreparedPrimeOutputRule`.
///
/// The first ordinary block is the tail controller.  It may also be one of the split parents,
/// which is important when the prime gap is close to the available output-block budget.
pub fn prepared_prime_output_config(
    transition: &MassTransition,
    ordinary_weight: usize,
) -> Result<LogicalConfig, String> {
    if ordinary_weight != 4 {
        return Err("the framed output gate is currently a weight-four compiler".to_string());
    }
    if transition.before.ordinary_blocks == 0 {
        return Err("the finite bootstrap must manufacture the first ordinary block".to_string());
    }
    if transition.split_blocks > transition.before.ordinary_blocks {
        return Err(format!(
            "transition requests {} splits from {} ordinary blocks",
            transition.split_blocks, transition.before.ordinary_blocks
        ));
    }

    let target_residue = residue_of_tail_weight(transition.after.tail_weight, ordinary_weight)?;
    let current_residue = residue_of_tail_weight(transition.before.tail_weight, ordinary_weight)?;
    let mut config = LogicalConfig::new();
    config.insert((0, 0), TAIL_BASE + current_residue);

    let controller = if transition.split_blocks > 0 {
        SPLIT_REQUEST_BASE + target_residue
    } else {
        REQUEST_BASE + target_residue
    };
    config.insert((0, -1), controller);

    let remaining_splits = transition.split_blocks.saturating_sub(1);
    for index in 1..transition.before.ordinary_blocks {
        let state = if index <= remaining_splits {
            SPLIT_STATE
        } else {
            ORDINARY_STATE
        };
        let x = i32::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(2))
            .ok_or_else(|| "ordinary output reservoir exceeds the grid".to_string())?;
        config.insert((x, 2), state);
    }
    Ok(config)
}

impl<R: LocalRule> BlockMachine<R> {
    pub fn new(rule: R) -> Result<Self, String> {
        let codec = BlockCodec::new(rule.state_count())?;
        Self::with_codec(rule, codec)
    }

    pub fn with_codec(rule: R, codec: BlockCodec) -> Result<Self, String> {
        if codec.state_count() != rule.state_count() {
            return Err(format!(
                "codec has {} logical states but the rule has {}",
                codec.state_count(),
                rule.state_count()
            ));
        }
        if rule.next(0, [0; NEIGHBOURS.len()]) != 0 {
            return Err("a compiled CA must keep empty space quiescent".to_string());
        }
        Ok(BlockMachine { rule, codec })
    }

    pub fn initial_binary(&self) -> BinaryConfig {
        BinaryConfig::from([(0, 0)])
    }

    pub fn initial_logical(&self) -> LogicalConfig {
        LogicalConfig::from([((0, 0), 1)])
    }

    pub fn step_binary(&self, current: &BinaryConfig) -> Result<BinaryConfig, String> {
        if current.len() == 1 {
            let seed = *current.iter().next().unwrap();
            let next = self
                .codec
                .encode(&self.rule.step(&self.initial_logical())?)?;
            return next
                .into_iter()
                .map(|(x, y)| {
                    Ok((
                        x.checked_add(seed.0)
                            .ok_or_else(|| "bootstrap exceeds the physical grid".to_string())?,
                        y.checked_add(seed.1)
                            .ok_or_else(|| "bootstrap exceeds the physical grid".to_string())?,
                    ))
                })
                .collect();
        }
        let logical = self.codec.decode(current)?;
        self.codec.encode(&self.rule.step(&logical)?)
    }

    fn state_near(&self, binary: &BinaryConfig, anchor: LogicalPoint) -> Result<usize, String> {
        Ok(self
            .codec
            .matching_state_at_anchor(binary, anchor)?
            .unwrap_or(0))
    }

    fn shifted_anchor(
        &self,
        anchor: LogicalPoint,
        dx: i32,
        dy: i32,
    ) -> Result<LogicalPoint, String> {
        let x = dx
            .checked_mul(self.codec.stride_x)
            .and_then(|offset| anchor.0.checked_add(offset))
            .ok_or_else(|| "logical block exceeds the physical grid".to_string())?;
        let y = dy
            .checked_mul(self.codec.stride_y)
            .and_then(|offset| anchor.1.checked_add(offset))
            .ok_or_else(|| "logical block exceeds the physical grid".to_string())?;
        Ok((x, y))
    }

    fn nearby_points<'a>(
        binary: &'a BinaryConfig,
        center: LogicalPoint,
        radius: i32,
    ) -> impl Iterator<Item = LogicalPoint> + 'a {
        let min_x = center.0.saturating_sub(radius);
        let max_x = center.0.saturating_add(radius);
        let min_y = center.1.saturating_sub(radius);
        let max_y = center.1.saturating_add(radius);
        binary
            .range((min_x, i32::MIN)..=(max_x, i32::MAX))
            .copied()
            .filter(move |point| point.1 >= min_y && point.1 <= max_y)
    }

    fn isolated_seed(&self, seed: LogicalPoint, current: &BinaryConfig) -> bool {
        let radius = self.codec.radius();
        current.contains(&seed)
            && Self::nearby_points(current, seed, radius).all(|other| other == seed)
    }

    fn seed_near(
        &self,
        point: LogicalPoint,
        current: &BinaryConfig,
    ) -> Result<Option<LogicalPoint>, String> {
        let radius = self.codec.radius();
        let mut seed = None;
        for candidate in Self::nearby_points(current, point, radius) {
            if !self.isolated_seed(candidate, current) {
                continue;
            }
            if seed.replace(candidate).is_some() {
                return Err(format!(
                    "ambiguous isolated seeds near physical point {:?}",
                    point
                ));
            }
        }
        Ok(seed)
    }

    fn active_anchors_near(
        &self,
        binary: &BinaryConfig,
        point: LogicalPoint,
    ) -> Result<BTreeMap<LogicalPoint, usize>, String> {
        let radius = self.codec.radius();
        let scan_radius = radius.saturating_add(self.codec.width.max(self.codec.height));
        let frame = [(0, 0), (2, 0), (1, 1)];
        let mut candidates = BTreeSet::new();
        for (x, y) in Self::nearby_points(binary, point, scan_radius) {
            for (fx, fy) in frame {
                if let (Some(anchor_x), Some(anchor_y)) = (x.checked_sub(fx), y.checked_sub(fy)) {
                    if point.0.abs_diff(anchor_x) <= radius as u32
                        && point.1.abs_diff(anchor_y) <= radius as u32
                    {
                        candidates.insert((anchor_x, anchor_y));
                    }
                }
            }
        }

        let mut anchors = BTreeMap::new();
        for anchor in candidates {
            if let Some(state) = self.codec.matching_state_at_anchor(binary, anchor)? {
                anchors.insert(anchor, state);
            }
        }
        Ok(anchors)
    }

    /// The actual sparse local predicate.  A physical output bit discovers a valid framed block
    /// in its bounded neighbourhood, adopts that block's grid phase, and reads at most the 3x3
    /// logical neighbourhood needed for the rule.  It never consults an arbitrary global table.
    pub fn next_binary_bit(
        &self,
        point: LogicalPoint,
        current: &BinaryConfig,
    ) -> Result<bool, String> {
        let mut proposal = None;
        if let Some(seed) = self.seed_near(point, current)? {
            let canonical = self
                .codec
                .encode(&self.rule.step(&self.initial_logical())?)?;
            proposal = Some(
                point
                    .0
                    .checked_sub(seed.0)
                    .and_then(|x| {
                        point
                            .1
                            .checked_sub(seed.1)
                            .map(|y| canonical.contains(&(x, y)))
                    })
                    .unwrap_or(false),
            );
        }

        let radius = self.codec.radius();
        let anchors = self.active_anchors_near(current, point)?;
        for (&source_anchor, _) in &anchors {
            if point.0.abs_diff(source_anchor.0) > radius as u32
                || point.1.abs_diff(source_anchor.1) > radius as u32
            {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let target_anchor = self.shifted_anchor(source_anchor, dx, dy)?;
                    let local = (
                        point.0.checked_sub(target_anchor.0).ok_or_else(|| {
                            "target block has no representable offset".to_string()
                        })?,
                        point.1.checked_sub(target_anchor.1).ok_or_else(|| {
                            "target block has no representable offset".to_string()
                        })?,
                    );
                    if local.0 < 0
                        || local.0 >= self.codec.width
                        || local.1 < 0
                        || local.1 >= self.codec.height
                    {
                        continue;
                    }

                    let center = self.state_near(current, target_anchor)?;
                    let mut nearby = [0; NEIGHBOURS.len()];
                    for (slot, (ndx, ndy)) in NEIGHBOURS.iter().enumerate() {
                        nearby[slot] = self
                            .state_near(current, self.shifted_anchor(target_anchor, *ndx, *ndy)?)?;
                    }
                    let next_state = self.rule.next(center, nearby);
                    if next_state >= self.rule.state_count() {
                        return Err(format!(
                            "logical rule returned invalid state {}",
                            next_state
                        ));
                    }
                    let next_bit = self.codec.codeword(next_state)?.contains(&local);
                    if let Some(previous) = proposal {
                        if previous != next_bit {
                            return Err(format!(
                                "conflicting block proposals at physical point {:?}",
                                point
                            ));
                        }
                    } else {
                        proposal = Some(next_bit);
                    }
                }
            }
        }
        Ok(proposal.unwrap_or(false))
    }

    /// Evaluate the compiled predicate independently from the macro block simulator.
    pub fn step_binary_local(&self, current: &BinaryConfig) -> Result<BinaryConfig, String> {
        if current.is_empty() {
            return Ok(BinaryConfig::new());
        }

        let anchors = self.codec.active_anchors(current)?;
        let mut candidates = BTreeSet::new();
        for anchor in anchors.keys() {
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let base = self.shifted_anchor(*anchor, dx, dy)?;
                    for y in 0..self.codec.height {
                        for x in 0..self.codec.width {
                            candidates.insert((
                                base.0.checked_add(x).ok_or_else(|| {
                                    "candidate block exceeds the physical grid".to_string()
                                })?,
                                base.1.checked_add(y).ok_or_else(|| {
                                    "candidate block exceeds the physical grid".to_string()
                                })?,
                            ));
                        }
                    }
                }
            }
        }
        let radius = self.codec.radius();
        for &seed in current {
            if !self.isolated_seed(seed, current) {
                continue;
            }
            for dy in -radius..=radius {
                for dx in -radius..=radius {
                    candidates.insert((
                        seed.0
                            .checked_add(dx)
                            .ok_or_else(|| "seed region exceeds the physical grid".to_string())?,
                        seed.1
                            .checked_add(dy)
                            .ok_or_else(|| "seed region exceeds the physical grid".to_string())?,
                    ));
                }
            }
        }

        candidates
            .into_iter()
            .filter_map(|point| match self.next_binary_bit(point, current) {
                Ok(true) => Some(Ok(point)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub fn states(&self, steps: usize) -> Result<Vec<BinaryConfig>, String> {
        let mut states = vec![self.initial_binary()];
        for _ in 0..steps {
            states.push(self.step_binary(states.last().unwrap())?);
        }
        Ok(states)
    }
}

/// Exact decomposition of a population into ordinary W-weight blocks and a framed tail.
///
/// Once the population reaches W, the tail retains one W-weight frame; its extra payload is the
/// residue modulo W.  Smaller populations form the finite bootstrap before framed blocks exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mass {
    pub population: usize,
    pub ordinary_blocks: usize,
    pub tail_weight: usize,
}

impl Mass {
    pub fn new(population: usize, ordinary_weight: usize) -> Result<Self, String> {
        if ordinary_weight == 0 {
            return Err("ordinary block weight must be positive".to_string());
        }
        let (ordinary_blocks, tail_weight) = if population < ordinary_weight {
            (0, population)
        } else {
            (
                (population - ordinary_weight) / ordinary_weight,
                ordinary_weight + (population - ordinary_weight) % ordinary_weight,
            )
        };
        Ok(Mass {
            population,
            ordinary_blocks,
            tail_weight,
        })
    }

    pub fn encoded_weight(&self, ordinary_weight: usize) -> usize {
        self.ordinary_blocks
            .checked_mul(ordinary_weight)
            .and_then(|n| n.checked_add(self.tail_weight))
            .expect("mass arithmetic must fit in usize")
    }
}

/// The population-neutral/control transition required by the construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MassTransition {
    pub index: usize,
    pub before: Mass,
    pub after: Mass,
    pub prime_gap: usize,
    pub split_blocks: usize,
    pub tail_delta: i64,
}

impl MassTransition {
    fn between(
        index: usize,
        before: usize,
        after: usize,
        ordinary_weight: usize,
    ) -> Result<Self, String> {
        if ordinary_weight == 0 {
            return Err("ordinary block weight must be positive".to_string());
        }
        if after < before {
            return Err(format!(
                "the monotone construction cannot reduce the prime population {} to {}",
                before, after
            ));
        }
        let before = Mass::new(before, ordinary_weight)?;
        let after = Mass::new(after, ordinary_weight)?;
        if after.ordinary_blocks < before.ordinary_blocks {
            return Err("ordinary block count unexpectedly decreased".to_string());
        }
        let split_blocks = after.ordinary_blocks - before.ordinary_blocks;
        let tail_delta = after.tail_weight as i64 - before.tail_weight as i64;
        let scheduled = split_blocks as i64 * ordinary_weight as i64 + tail_delta;
        let prime_gap = after.population - before.population;
        if scheduled != prime_gap as i64 {
            return Err(format!(
                "mass transition {} accounts for {} births, expected {}",
                index, scheduled, prime_gap
            ));
        }
        Ok(MassTransition {
            index,
            before,
            after,
            prime_gap,
            split_blocks,
            tail_delta,
        })
    }
}

/// Verify the exact weighted accounting for `steps` population transitions.
///
/// This is a construction certificate, not an implementation of the prime scheduler.  In
/// particular, the block compiler above and this recurrence can be checked independently.
pub fn prime_mass_trace(
    steps: usize,
    ordinary_weight: usize,
) -> Result<Vec<MassTransition>, String> {
    let target = primes_target(
        steps
            .checked_add(1)
            .ok_or_else(|| "too many construction steps".to_string())?,
    );
    let mut transitions = Vec::with_capacity(steps);
    for (index, pair) in target.windows(2).enumerate() {
        transitions.push(MassTransition::between(
            index,
            pair[0],
            pair[1],
            ordinary_weight,
        )?);
    }
    Ok(transitions)
}

#[cfg(test)]
mod test {
    use super::*;

    struct GrowingLine;

    impl LocalRule for GrowingLine {
        fn state_count(&self) -> usize {
            3
        }

        fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
            if center != 0 {
                return center;
            }
            // The west neighbour is slot 3 in NEIGHBOURS.
            if neighbours[3] != 0 {
                2
            } else {
                0
            }
        }
    }

    struct TailCycle;

    impl LocalRule for TailCycle {
        fn state_count(&self) -> usize {
            6
        }

        fn next(&self, center: usize, _: [usize; 8]) -> usize {
            match center {
                ORDINARY_STATE => 2,
                2..=5 => 2 + (center - 1) % 4,
                _ => 0,
            }
        }
    }

    #[test]
    fn framed_blocks_round_trip_without_changing_logical_weight() {
        let codec = BlockCodec::new(4).unwrap();
        let logical = LogicalConfig::from([((0, 0), 1), ((1, 0), 2), ((3, -2), 3)]);
        let binary = codec.encode(&logical).unwrap();
        assert_eq!(binary.len(), 12);
        assert_eq!(codec.logical_weight(&logical).unwrap(), binary.len());
        assert_eq!(codec.decode(&binary).unwrap(), logical);
        assert_eq!(
            codec
                .active_anchors(&binary)
                .unwrap()
                .keys()
                .copied()
                .collect::<BTreeSet<_>>(),
            logical
                .keys()
                .map(|&point| codec.anchor(point).unwrap())
                .collect()
        );
        assert!(codec.radius() >= 2 * codec.stride_x);
        assert!(codec.decode(&BinaryConfig::from([(5, 0)])).is_err());
        for residue in 0..codec.active_weight() {
            assert_eq!(
                codec.tail_codeword(residue).unwrap().len(),
                codec.active_weight() + residue
            );
        }
        assert!(codec.tail_codeword(codec.active_weight()).is_err());
    }

    #[test]
    fn every_ordinary_and_tail_frame_has_a_unique_alignment() {
        let codec = BlockCodec::with_tail_states(TAIL_BASE, TAIL_RESIDUES).unwrap();
        for first in 1..codec.state_count {
            for second in 1..codec.state_count {
                for delta in [(1, 0), (0, 1), (1, -1)] {
                    let logical = LogicalConfig::from([((0, 0), first), (delta, second)]);
                    let binary = codec.encode(&logical).unwrap();
                    assert_eq!(codec.decode(&binary).unwrap(), logical);
                    assert_eq!(
                        codec.active_anchors(&binary).unwrap().len(),
                        logical.len(),
                        "states {} and {} at {:?}",
                        first,
                        second,
                        delta
                    );
                }
            }
        }
    }

    #[test]
    fn binary_block_machine_uses_a_one_bit_seed_then_framed_states() {
        let machine = BlockMachine::new(GrowingLine).unwrap();
        let states = machine.states(4).unwrap();
        assert_eq!(states[0], BinaryConfig::from([(0, 0)]));
        assert_eq!(
            states.iter().map(BinaryConfig::len).collect::<Vec<_>>(),
            vec![1, 8, 12, 16, 20]
        );
        for state in &states[1..] {
            assert_eq!(
                machine.codec.decode(state).unwrap().len() * machine.codec.active_weight(),
                state.len()
            );
        }
        for pair in states.windows(2) {
            assert_eq!(machine.step_binary_local(&pair[0]).unwrap(), pair[1]);
        }

        let translated_seed: BinaryConfig = BinaryConfig::from([(9, -7)]);
        let translated_next = machine.step_binary_local(&translated_seed).unwrap();
        let shifted_first = states[1]
            .iter()
            .map(|&(x, y)| (x + 9, y - 7))
            .collect::<BinaryConfig>();
        assert_eq!(translated_next, shifted_first);

        let translated_frame: BinaryConfig =
            shifted_first.iter().map(|&(x, y)| (x + 2, y + 5)).collect();
        let shifted_logical_next = states[2]
            .iter()
            .map(|&(x, y)| (x + 11, y - 2))
            .collect::<BinaryConfig>();
        assert_eq!(
            machine.step_binary_local(&translated_frame).unwrap(),
            shifted_logical_next
        );
    }

    #[test]
    fn framed_tail_changes_weight_through_the_same_local_compiler() {
        let codec = BlockCodec::with_tail_states(2, 4).unwrap();
        let machine = BlockMachine::with_codec(TailCycle, codec).unwrap();
        let states = machine.states(5).unwrap();
        assert_eq!(
            states.iter().map(BinaryConfig::len).collect::<Vec<_>>(),
            vec![1, 4, 5, 6, 7, 4]
        );
        for state in &states[1..] {
            let decoded = machine.codec.decode(state).unwrap();
            assert_eq!(machine.codec.logical_weight(&decoded).unwrap(), state.len());
        }
        for pair in states.windows(2) {
            assert_eq!(machine.step_binary_local(&pair[0]).unwrap(), pair[1]);
        }
    }

    #[test]
    fn local_predicate_is_unchanged_by_faraway_patterns() {
        let machine = BlockMachine::new(GrowingLine).unwrap();
        let current = machine
            .codec
            .encode(&machine.rule.step(&machine.initial_logical()).unwrap())
            .unwrap();
        let near = machine.step_binary_local(&current).unwrap();
        let far_offset = 10 * machine.codec.radius() + 100;
        let mut separated = current.clone();
        separated.extend(
            current
                .iter()
                .map(|&(x, y)| (x + far_offset, y - far_offset)),
        );
        for point in near {
            assert_eq!(
                machine.next_binary_bit(point, &current).unwrap(),
                machine.next_binary_bit(point, &separated).unwrap()
            );
        }
    }

    #[test]
    fn prime_mass_recurrence_is_exact_for_a_long_prefix() {
        for weight in [2, 4, 8, 16] {
            let trace = prime_mass_trace(5_000, weight).unwrap();
            let first = trace.first().unwrap();
            let last = trace.last().unwrap();
            assert_eq!(first.before.population, 1);
            assert_eq!(last.after.population, primes_target(5_001)[5_000]);
            for step in &trace {
                assert_eq!(step.before.encoded_weight(weight), step.before.population);
                assert_eq!(step.after.encoded_weight(weight), step.after.population);
                if step.after.population >= weight {
                    assert!(
                        step.after.tail_weight >= weight && step.after.tail_weight < 2 * weight
                    );
                }
                let births = step.split_blocks as i64 * weight as i64 + step.tail_delta;
                assert_eq!(births, step.prime_gap as i64);
                assert!(step.split_blocks * weight <= step.prime_gap + weight - 1);
            }
        }
    }

    #[test]
    fn invalid_construction_inputs_are_rejected() {
        assert!(BlockCodec::new(1).is_err());
        assert!(BlockCodec::with_tail_states(2, 5).is_err());
        assert!(BlockMachine::with_codec(TailCycle, BlockCodec::new(5).unwrap()).is_err());
        assert!(prime_mass_trace(4, 0).is_err());
        assert!(BlockMachine::new(Unquiescent).is_err());
        let trace = prime_mass_trace(10, 4).unwrap();
        assert!(prepared_prime_output_config(&trace[0], 8).is_err());
        let codec = BlockCodec::new(2).unwrap();
        assert!(codec
            .encode(&LogicalConfig::from([((i32::MAX, 0), ORDINARY_STATE)]))
            .is_err());
        assert!(GrowingLine
            .step(&LogicalConfig::from([((i32::MAX, 0), ORDINARY_STATE)]))
            .is_err());
    }

    #[test]
    fn prepared_split_labels_birth_exactly_one_weighted_child_each() {
        let source = LogicalConfig::from([
            ((0, 0), SPLIT_STATE),
            ((2, 0), ORDINARY_STATE),
            ((4, 0), SPLIT_STATE),
        ]);
        let next = PreparedSplitRule.step(&source).unwrap();
        assert_eq!(next.len(), source.len() + 2);
        assert!(next.iter().all(|(_, state)| *state == ORDINARY_STATE));
        assert_eq!(
            next.keys().copied().collect::<Vec<_>>(),
            vec![(0, 0), (1, 0), (2, 0), (4, 0), (5, 0)]
        );

        let machine = BlockMachine::new(PreparedSplitRule).unwrap();
        let binary_before = machine.codec.encode(&source).unwrap();
        let binary_after = machine.step_binary_local(&binary_before).unwrap();
        assert_eq!(
            binary_after.len() - binary_before.len(),
            2 * machine.codec.active_weight()
        );
        assert_eq!(machine.step_binary(&binary_before).unwrap(), binary_after);
    }

    #[test]
    fn prepared_prime_output_gate_accounts_for_every_weight_and_birth() {
        let codec = BlockCodec::with_tail_states(TAIL_BASE, TAIL_RESIDUES).unwrap();
        let machine = BlockMachine::with_codec(PreparedPrimeOutputRule, codec).unwrap();
        let trace = prime_mass_trace(1_000, machine.codec.active_weight()).unwrap();
        let mut checked = 0;
        for transition in trace {
            if transition.before.ordinary_blocks == 0 {
                continue;
            }
            let current =
                prepared_prime_output_config(&transition, machine.codec.active_weight()).unwrap();
            let next = machine.rule.step(&current).unwrap();
            assert_eq!(
                machine.codec.logical_weight(&next).unwrap(),
                transition.after.population
            );
            assert_eq!(next.len(), current.len() + transition.split_blocks);
            if checked < 12 {
                let binary = machine.codec.encode(&current).unwrap();
                assert_eq!(binary.len(), transition.before.population);
                let direct = machine.step_binary_local(&binary).unwrap();
                let macro_step = machine.step_binary(&binary).unwrap();
                assert_eq!(direct, macro_step);
                assert_eq!(direct.len(), transition.after.population);
            }
            checked += 1;
        }
        assert!(checked > 900);
    }

    #[test]
    fn prime_splits_fit_the_available_weighted_blocks_after_bootstrap() {
        let trace = prime_mass_trace(2_000, 4).unwrap();
        let mut checked = 0;
        for transition in trace {
            if transition.split_blocks == 0 {
                continue;
            }
            if transition.before.ordinary_blocks < transition.split_blocks {
                // Only the finite transition from the bootstrap tail can manufacture the
                // first ordinary block.
                assert!(transition.before.population < 8);
                continue;
            }
            let source: LogicalConfig = (0..transition.before.ordinary_blocks)
                .map(|index| {
                    let state = if index < transition.split_blocks {
                        SPLIT_STATE
                    } else {
                        ORDINARY_STATE
                    };
                    ((2 * index as i32, 0), state)
                })
                .collect();
            let next = PreparedSplitRule.step(&source).unwrap();
            assert_eq!(next.len() - source.len(), transition.split_blocks);
            checked += 1;
        }
        assert!(checked > 1_000);
    }

    struct Unquiescent;

    impl LocalRule for Unquiescent {
        fn state_count(&self) -> usize {
            2
        }

        fn next(&self, _: usize, _: [usize; 8]) -> usize {
            1
        }
    }
}
