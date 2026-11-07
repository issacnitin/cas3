use crate::construct::{LocalRule, LogicalConfig};

const WEST: usize = 3;
const EAST: usize = 4;
const PAD: usize = 1;
const CHUNK_BASE: usize = 2;

/// Stable constant-mass compaction for fixed-height packed record columns.
///
/// Every slot is active: a padding column or a record column containing one packed chunk per row.
/// A record moves one slot west exactly when the whole-column invariant says the slot to its west
/// is padding. Each chunk applies the same radius-one traffic rule, so valid columns move
/// atomically and record order is preserved.
pub struct PackedColumnRule {
    payload_bits: usize,
    payload_states: usize,
}

impl PackedColumnRule {
    pub fn new(payload_bits: usize) -> Result<Self, String> {
        if !(1..=20).contains(&payload_bits) {
            return Err("packed queue payload width must be between 1 and 20".to_string());
        }
        Ok(PackedColumnRule {
            payload_bits,
            payload_states: 1usize << payload_bits,
        })
    }

    pub fn padding(&self) -> usize {
        PAD
    }

    pub fn chunk(&self, payload: usize) -> Result<usize, String> {
        if payload >= self.payload_states {
            return Err(format!(
                "packed payload {} does not fit {} bits",
                payload, self.payload_bits
            ));
        }
        Ok(CHUNK_BASE + payload)
    }

    pub fn payload(&self, state: usize) -> Option<usize> {
        let payload = state.checked_sub(CHUNK_BASE)?;
        (payload < self.payload_states).then_some(payload)
    }

    fn is_chunk(&self, state: usize) -> bool {
        self.payload(state).is_some()
    }
}

impl LocalRule for PackedColumnRule {
    fn state_count(&self) -> usize {
        CHUNK_BASE + self.payload_states
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 {
            return 0;
        }
        if center == PAD {
            return if self.is_chunk(neighbours[EAST]) {
                neighbours[EAST]
            } else {
                PAD
            };
        }
        if self.is_chunk(center) {
            return if neighbours[WEST] == PAD { PAD } else { center };
        }
        0
    }
}

pub struct PackedColumnShiftRule {
    payload_states: usize,
}

impl PackedColumnShiftRule {
    pub fn new(payload_bits: usize) -> Result<Self, String> {
        let packed = PackedColumnRule::new(payload_bits)?;
        Ok(PackedColumnShiftRule {
            payload_states: packed.payload_states,
        })
    }
}

impl LocalRule for PackedColumnShiftRule {
    fn state_count(&self) -> usize {
        CHUNK_BASE + self.payload_states
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        if center == 0 {
            return 0;
        }
        if neighbours[EAST] == 0 {
            PAD
        } else {
            neighbours[EAST]
        }
    }
}

pub struct PackedRecordGrid {
    pub config: LogicalConfig,
    pub steps: usize,
    rows: usize,
    slots: usize,
    chunks: usize,
    rule: PackedColumnRule,
}

impl PackedRecordGrid {
    pub fn new(payload_bits: usize, records: &[Vec<Option<Vec<usize>>>]) -> Result<Self, String> {
        if records.is_empty() || records[0].is_empty() {
            return Err("packed record grid needs at least one row and slot".to_string());
        }
        let slots = records[0].len();
        if records.iter().any(|row| row.len() != slots) {
            return Err("packed record rows must have equal slot counts".to_string());
        }
        let chunks = records
            .iter()
            .flat_map(|row| row.iter())
            .find_map(|record| record.as_ref().map(Vec::len))
            .ok_or_else(|| "packed record grid needs at least one record".to_string())?;
        if chunks == 0 {
            return Err("packed records need at least one chunk".to_string());
        }
        let rule = PackedColumnRule::new(payload_bits)?;
        let mut config = LogicalConfig::new();
        for (row, slots_data) in records.iter().enumerate() {
            let base_y = row
                .checked_mul(chunks + 1)
                .ok_or_else(|| "packed record grid height overflowed".to_string())?;
            for (slot, record) in slots_data.iter().enumerate() {
                if record.as_ref().is_some_and(|record| record.len() != chunks) {
                    return Err("packed records must have equal chunk counts".to_string());
                }
                for chunk in 0..chunks {
                    let state = match record {
                        Some(record) => rule.chunk(record[chunk])?,
                        None => rule.padding(),
                    };
                    config.insert(
                        (
                            i32::try_from(slot)
                                .map_err(|_| "packed queue width does not fit i32".to_string())?,
                            i32::try_from(base_y + chunk)
                                .map_err(|_| "packed queue height does not fit i32".to_string())?,
                        ),
                        state,
                    );
                }
            }
        }
        Ok(PackedRecordGrid {
            config,
            steps: 0,
            rows: records.len(),
            slots,
            chunks,
            rule,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = self.rule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn compact(&mut self) -> Result<(), String> {
        for _ in 0..self.slots {
            self.step()?;
        }
        Ok(())
    }

    pub fn shift(&mut self) -> Result<(), String> {
        let rule = PackedColumnShiftRule::new(self.rule.payload_bits)?;
        self.config = rule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn front(&self, row: usize) -> Result<Option<Vec<usize>>, String> {
        if row >= self.rows {
            return Err("packed queue row is outside the grid".to_string());
        }
        let base_y = row * (self.chunks + 1);
        let mut payload = Vec::with_capacity(self.chunks);
        for chunk in 0..self.chunks {
            let state = self
                .config
                .get(&(0, (base_y + chunk) as i32))
                .ok_or_else(|| "packed queue lost its front slot".to_string())?;
            if *state == PAD {
                return Ok(None);
            }
            payload.push(
                self.rule
                    .payload(*state)
                    .ok_or_else(|| "packed queue front has an invalid chunk".to_string())?,
            );
        }
        Ok(Some(payload))
    }

    pub fn records(&self) -> Result<Vec<Vec<Option<Vec<usize>>>>, String> {
        let mut records = vec![vec![None; self.slots]; self.rows];
        for row in 0..self.rows {
            let base_y = row * (self.chunks + 1);
            for slot in 0..self.slots {
                let mut payload = Vec::with_capacity(self.chunks);
                let mut padding = true;
                for chunk in 0..self.chunks {
                    let state = self
                        .config
                        .get(&(slot as i32, (base_y + chunk) as i32))
                        .ok_or_else(|| "packed queue lost an active slot".to_string())?;
                    if *state == PAD {
                        if !padding {
                            return Err("packed queue contains a partial record column".to_string());
                        }
                    } else {
                        padding = false;
                        payload.push(
                            self.rule
                                .payload(*state)
                                .ok_or_else(|| "packed queue has an invalid chunk".to_string())?,
                        );
                    }
                }
                if !padding {
                    if payload.len() != self.chunks {
                        return Err("packed queue contains a partial record column".to_string());
                    }
                    records[row][slot] = Some(payload);
                }
            }
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construct::BlockMachine;

    #[test]
    fn packed_columns_compact_stably_in_parallel_rows() {
        let source = vec![
            vec![
                Some(vec![1, 2, 3]),
                None,
                Some(vec![4, 5, 6]),
                None,
                None,
                Some(vec![7, 8, 9]),
            ],
            vec![
                None,
                Some(vec![10, 11, 12]),
                None,
                Some(vec![13, 14, 15]),
                None,
                None,
            ],
        ];
        let mut grid = PackedRecordGrid::new(4, &source).unwrap();
        let active = grid.config.len();
        grid.compact().unwrap();
        assert_eq!(grid.config.len(), active);
        assert_eq!(
            grid.records().unwrap(),
            vec![
                vec![
                    Some(vec![1, 2, 3]),
                    Some(vec![4, 5, 6]),
                    Some(vec![7, 8, 9]),
                    None,
                    None,
                    None,
                ],
                vec![
                    Some(vec![10, 11, 12]),
                    Some(vec![13, 14, 15]),
                    None,
                    None,
                    None,
                    None,
                ],
            ]
        );
    }

    #[test]
    fn packed_column_rule_compiles_with_twenty_bit_payload_states() {
        let rule = PackedColumnRule::new(20).unwrap();
        let logical = LogicalConfig::from([
            ((0, 0), rule.padding()),
            ((1, 0), rule.chunk((1 << 20) - 1).unwrap()),
        ]);
        let machine = BlockMachine::new(rule).unwrap();
        assert!(machine.codec.width > 1_000_000);
        let binary = machine.codec.encode(&logical).unwrap();
        let next = machine.step_binary(&binary).unwrap();
        assert_eq!(
            machine.codec.decode(&next).unwrap(),
            machine.rule.step(&logical).unwrap()
        );
        assert_eq!(binary.len(), next.len());
    }

    #[test]
    fn compacted_row_exposes_one_complete_record_per_generation() {
        let source = vec![vec![
            None,
            Some(vec![1, 2, 3]),
            None,
            Some(vec![4, 5, 6]),
            Some(vec![7, 8, 9]),
            None,
        ]];
        let mut grid = PackedRecordGrid::new(4, &source).unwrap();
        let active = grid.config.len();
        grid.compact().unwrap();
        let mut output = Vec::new();
        for _ in 0..3 {
            output.push(grid.front(0).unwrap().unwrap());
            grid.shift().unwrap();
            assert_eq!(grid.config.len(), active);
        }
        assert_eq!(output, vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]]);
        assert_eq!(grid.front(0).unwrap(), None);
    }

    #[test]
    fn packed_grid_rejects_malformed_records() {
        assert!(PackedRecordGrid::new(0, &[vec![Some(vec![1])]]).is_err());
        assert!(PackedRecordGrid::new(4, &[]).is_err());
        assert!(PackedRecordGrid::new(4, &[vec![None]]).is_err());
        assert!(PackedRecordGrid::new(4, &[vec![Some(vec![1, 2]), Some(vec![3])]]).is_err());
    }
}
