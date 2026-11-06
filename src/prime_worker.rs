/*
 * Local binary trial-division worker
 * ==================================
 *
 * One worker is a one-cell-high tape whose least significant register bit is at x=0.
 * Every tape cell contains four bits (candidate n, divisor d, divisor square s=d^2, and
 * remainder r), plus finite position/cursor/head fields.  The rule therefore never stores an
 * unbounded integer in a cell state.
 *
 * The finite head performs the following algorithm entirely through neighbour-local moves:
 *
 *   d := 2; s := 4
 *   while s <= n:
 *       r := 0
 *       for bit in n, most-significant to least-significant:
 *           r := 2*r + bit
 *           if r >= d: r := r - d
 *       if r == 0: composite
 *       d := d + 1
 *       s := s + 2*d(old) + 1
 *   prime
 *
 * This module is the autonomous CA scheduler primitive.  It has no host-side primality branch:
 * the only input is a binary tape initialisation, and the only transitions are the local rule.
 */

use std::collections::BTreeMap;

use crate::construct::LocalRule;
use crate::construct::{LogicalConfig, LogicalPoint};

const WEST: usize = 3;
const EAST: usize = 4;
const DATA_BITS: usize = 4;
const BOUNDARY_BITS: usize = 2;
const CURSOR_BIT: usize = DATA_BITS + BOUNDARY_BITS;
const SINGLE_DIVISOR_BIT: usize = CURSOR_BIT + 1;
const BOTTOM_BIT: usize = SINGLE_DIVISOR_BIT + 1;
const TOP_BIT: usize = BOTTOM_BIT + 1;
const WIRE_ROLE_SHIFT: usize = TOP_BIT + 1;
const WIRE_ROLE_BITS: usize = 3;
const CELL_PAYLOAD: usize = 1 << (WIRE_ROLE_SHIFT + WIRE_ROLE_BITS);
const LAUNCHER_GUARD_BITS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    Equal,
    Less,
    Greater,
}

impl Relation {
    fn update(self, left: bool, right: bool) -> Self {
        if self != Relation::Equal {
            return self;
        }
        match left.cmp(&right) {
            std::cmp::Ordering::Less => Relation::Less,
            std::cmp::Ordering::Equal => Relation::Equal,
            std::cmp::Ordering::Greater => Relation::Greater,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Tape,
    CompareSquare(Relation),
    ClearRemainderEast,
    SeekCursorEast,
    SeekCursorWest,
    GoToLsb(bool),
    ShiftRemainderEast(bool),
    CompareRemainderWest(Relation),
    SubtractDivisorEast(bool),
    SeekCursorToAdvanceEast,
    SeekCursorToAdvanceWest,
    PlaceCursor(bool),
    CheckRemainderEast(bool),
    GoToLsbForUpdate,
    UpdateDivisorEast {
        divisor_carry: bool,
        square_carry: bool,
        previous_divisor_bit: bool,
    },
    Prime,
    Composite,
    ClearRemainderWest,
    PassPrime,
    PassComposite,
    CandidatePrime,
    CandidateComposite,
    CounterIncrementEast,
    CounterReturnWest,
    CounterBirthEast,
    CounterAddEast(bool),
    CounterStrideReturnWest,
    CounterStrideBirthEast,
    RowWire,
    RowPrimeEast,
    RowCompositeEast,
    RowPrimeReady,
    RowCompositeReady,
    ColumnWire,
    ColumnPrimeNorth,
    ColumnCompositeNorth,
}

impl Phase {
    fn code(self) -> usize {
        use Phase::*;
        match self {
            Tape => 0,
            CompareSquare(Relation::Equal) => 1,
            CompareSquare(Relation::Less) => 2,
            CompareSquare(Relation::Greater) => 3,
            ClearRemainderEast => 4,
            SeekCursorEast => 5,
            SeekCursorWest => 6,
            GoToLsb(false) => 7,
            GoToLsb(true) => 8,
            ShiftRemainderEast(false) => 9,
            ShiftRemainderEast(true) => 10,
            CompareRemainderWest(Relation::Equal) => 11,
            CompareRemainderWest(Relation::Less) => 12,
            CompareRemainderWest(Relation::Greater) => 13,
            SubtractDivisorEast(false) => 14,
            SubtractDivisorEast(true) => 15,
            SeekCursorToAdvanceEast => 16,
            SeekCursorToAdvanceWest => 17,
            PlaceCursor(false) => 18,
            PlaceCursor(true) => 19,
            CheckRemainderEast(false) => 20,
            CheckRemainderEast(true) => 21,
            GoToLsbForUpdate => 22,
            UpdateDivisorEast {
                divisor_carry: false,
                square_carry: false,
                previous_divisor_bit: false,
            } => 23,
            UpdateDivisorEast {
                divisor_carry: false,
                square_carry: false,
                previous_divisor_bit: true,
            } => 24,
            UpdateDivisorEast {
                divisor_carry: false,
                square_carry: true,
                previous_divisor_bit: false,
            } => 25,
            UpdateDivisorEast {
                divisor_carry: false,
                square_carry: true,
                previous_divisor_bit: true,
            } => 26,
            UpdateDivisorEast {
                divisor_carry: true,
                square_carry: false,
                previous_divisor_bit: false,
            } => 27,
            UpdateDivisorEast {
                divisor_carry: true,
                square_carry: false,
                previous_divisor_bit: true,
            } => 28,
            UpdateDivisorEast {
                divisor_carry: true,
                square_carry: true,
                previous_divisor_bit: false,
            } => 29,
            UpdateDivisorEast {
                divisor_carry: true,
                square_carry: true,
                previous_divisor_bit: true,
            } => 30,
            Prime => 31,
            Composite => 32,
            ClearRemainderWest => 33,
            PassPrime => 34,
            PassComposite => 35,
            CandidatePrime => 36,
            CandidateComposite => 37,
            CounterIncrementEast => 38,
            CounterReturnWest => 39,
            CounterBirthEast => 40,
            CounterAddEast(false) => 41,
            CounterAddEast(true) => 42,
            CounterStrideReturnWest => 43,
            CounterStrideBirthEast => 44,
            RowWire => 45,
            RowPrimeEast => 46,
            RowCompositeEast => 47,
            RowPrimeReady => 48,
            RowCompositeReady => 49,
            ColumnWire => 50,
            ColumnPrimeNorth => 51,
            ColumnCompositeNorth => 52,
        }
    }

    fn decode(code: usize) -> Option<Self> {
        use Phase::*;
        Some(match code {
            0 => Tape,
            1 => CompareSquare(Relation::Equal),
            2 => CompareSquare(Relation::Less),
            3 => CompareSquare(Relation::Greater),
            4 => ClearRemainderEast,
            5 => SeekCursorEast,
            6 => SeekCursorWest,
            7 => GoToLsb(false),
            8 => GoToLsb(true),
            9 => ShiftRemainderEast(false),
            10 => ShiftRemainderEast(true),
            11 => CompareRemainderWest(Relation::Equal),
            12 => CompareRemainderWest(Relation::Less),
            13 => CompareRemainderWest(Relation::Greater),
            14 => SubtractDivisorEast(false),
            15 => SubtractDivisorEast(true),
            16 => SeekCursorToAdvanceEast,
            17 => SeekCursorToAdvanceWest,
            18 => PlaceCursor(false),
            19 => PlaceCursor(true),
            20 => CheckRemainderEast(false),
            21 => CheckRemainderEast(true),
            22 => GoToLsbForUpdate,
            23..=30 => {
                let flags = code - 23;
                UpdateDivisorEast {
                    divisor_carry: flags & 4 != 0,
                    square_carry: flags & 2 != 0,
                    previous_divisor_bit: flags & 1 != 0,
                }
            }
            31 => Prime,
            32 => Composite,
            33 => ClearRemainderWest,
            34 => PassPrime,
            35 => PassComposite,
            36 => CandidatePrime,
            37 => CandidateComposite,
            38 => CounterIncrementEast,
            39 => CounterReturnWest,
            40 => CounterBirthEast,
            41 => CounterAddEast(false),
            42 => CounterAddEast(true),
            43 => CounterStrideReturnWest,
            44 => CounterStrideBirthEast,
            45 => RowWire,
            46 => RowPrimeEast,
            47 => RowCompositeEast,
            48 => RowPrimeReady,
            49 => RowCompositeReady,
            50 => ColumnWire,
            51 => ColumnPrimeNorth,
            52 => ColumnCompositeNorth,
            _ => return None,
        })
    }

    const COUNT: usize = 53;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WireRole {
    None,
    Row,
    Port,
    RowEnd,
    Column,
}

impl WireRole {
    fn code(self) -> usize {
        match self {
            WireRole::None => 0,
            WireRole::Row => 1,
            WireRole::Port => 2,
            WireRole::RowEnd => 3,
            WireRole::Column => 4,
        }
    }

    fn decode(code: usize) -> Option<Self> {
        Some(match code {
            0 => WireRole::None,
            1 => WireRole::Row,
            2 => WireRole::Port,
            3 => WireRole::RowEnd,
            4 => WireRole::Column,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkerCell {
    n: bool,
    divisor: bool,
    square: bool,
    remainder: bool,
    lsb: bool,
    msb: bool,
    cursor: bool,
    single_divisor: bool,
    bottom: bool,
    top: bool,
    wire_role: WireRole,
    phase: Phase,
}

impl WorkerCell {
    fn tape(n: bool, divisor: bool, square: bool, remainder: bool, lsb: bool, msb: bool) -> Self {
        WorkerCell {
            n,
            divisor,
            square,
            remainder,
            lsb,
            msb,
            cursor: false,
            single_divisor: false,
            bottom: false,
            top: false,
            wire_role: WireRole::None,
            phase: Phase::Tape,
        }
    }

    fn encode(self) -> usize {
        let payload = (self.n as usize)
            | ((self.divisor as usize) << 1)
            | ((self.square as usize) << 2)
            | ((self.remainder as usize) << 3)
            | ((self.lsb as usize) << DATA_BITS)
            | ((self.msb as usize) << (DATA_BITS + 1))
            | ((self.cursor as usize) << CURSOR_BIT);
        let payload = payload
            | ((self.single_divisor as usize) << SINGLE_DIVISOR_BIT)
            | ((self.bottom as usize) << BOTTOM_BIT)
            | ((self.top as usize) << TOP_BIT)
            | (self.wire_role.code() << WIRE_ROLE_SHIFT);
        1 + self.phase.code() * CELL_PAYLOAD + payload
    }

    fn decode(state: usize) -> Option<Self> {
        if state == 0 || state > Phase::COUNT * CELL_PAYLOAD {
            return None;
        }
        let state = state - 1;
        let phase = Phase::decode(state / CELL_PAYLOAD)?;
        let payload = state % CELL_PAYLOAD;
        Some(WorkerCell {
            n: payload & 1 != 0,
            divisor: payload & 2 != 0,
            square: payload & 4 != 0,
            remainder: payload & 8 != 0,
            lsb: payload & (1 << DATA_BITS) != 0,
            msb: payload & (1 << (DATA_BITS + 1)) != 0,
            cursor: payload & (1 << CURSOR_BIT) != 0,
            single_divisor: payload & (1 << SINGLE_DIVISOR_BIT) != 0,
            bottom: payload & (1 << BOTTOM_BIT) != 0,
            top: payload & (1 << TOP_BIT) != 0,
            wire_role: WireRole::decode(
                (payload >> WIRE_ROLE_SHIFT) & ((1 << WIRE_ROLE_BITS) - 1),
            )?,
            phase,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Move {
    Stay,
    West,
    East,
}

struct Transition {
    source: WorkerCell,
    target_phase: Option<(Move, Phase)>,
}

fn transition_at(mut cell: WorkerCell) -> Transition {
    use Phase::*;

    let emit = |cell: WorkerCell, movement: Move, phase: Phase| Transition {
        source: WorkerCell {
            phase: Phase::Tape,
            ..cell
        },
        target_phase: Some((movement, phase)),
    };
    let stay = |cell: WorkerCell, phase: Phase| Transition {
        source: WorkerCell { phase, ..cell },
        target_phase: Some((Move::Stay, phase)),
    };

    match cell.phase {
        Tape => Transition {
            source: cell,
            target_phase: None,
        },
        Prime
        | Composite
        | PassPrime
        | PassComposite
        | CandidatePrime
        | CandidateComposite
        | CounterBirthEast
        | CounterStrideBirthEast
        | RowWire
        | RowPrimeEast
        | RowCompositeEast
        | RowPrimeReady
        | RowCompositeReady
        | ColumnWire
        | ColumnPrimeNorth
        | ColumnCompositeNorth => Transition {
            source: cell,
            target_phase: None,
        },
        CounterIncrementEast => {
            if cell.n {
                cell.n = false;
                if cell.msb {
                    cell.msb = false;
                    return emit(cell, Move::East, CounterBirthEast);
                }
                return emit(cell, Move::East, CounterIncrementEast);
            }
            cell.n = true;
            if cell.lsb {
                stay(cell, CounterIncrementEast)
            } else {
                emit(cell, Move::West, CounterReturnWest)
            }
        }
        CounterReturnWest => {
            if cell.lsb {
                stay(cell, CounterIncrementEast)
            } else {
                emit(cell, Move::West, CounterReturnWest)
            }
        }
        CounterAddEast(carry) => {
            let sum = cell.n as u8 + cell.divisor as u8 + carry as u8;
            cell.n = sum & 1 != 0;
            let next_carry = sum > 1;
            if cell.msb {
                if next_carry {
                    cell.msb = false;
                    return emit(cell, Move::East, CounterStrideBirthEast);
                }
                return emit(cell, Move::West, CounterStrideReturnWest);
            }
            emit(cell, Move::East, CounterAddEast(next_carry))
        }
        CounterStrideReturnWest => {
            if cell.lsb {
                stay(cell, CounterAddEast(false))
            } else {
                emit(cell, Move::West, CounterStrideReturnWest)
            }
        }
        ClearRemainderWest => {
            cell.remainder = false;
            if cell.lsb {
                return emit(cell, Move::East, SeekCursorEast);
            }
            emit(cell, Move::West, ClearRemainderWest)
        }
        CompareSquare(relation) => {
            let relation = relation.update(cell.square, cell.n);
            if cell.lsb {
                if relation == Relation::Greater {
                    return stay(cell, Prime);
                }
                if cell.msb {
                    cell.remainder = false;
                    return stay(cell, ClearRemainderEast);
                }
                cell.remainder = false;
                return emit(cell, Move::East, ClearRemainderEast);
            }
            emit(cell, Move::West, CompareSquare(relation))
        }
        ClearRemainderEast => {
            cell.remainder = false;
            if cell.msb {
                cell.cursor = true;
                return emit(cell, Move::West, GoToLsb(cell.n));
            }
            emit(cell, Move::East, ClearRemainderEast)
        }
        SeekCursorEast => {
            if cell.cursor {
                return emit(cell, Move::West, GoToLsb(cell.n));
            }
            emit(cell, Move::East, SeekCursorEast)
        }
        SeekCursorWest => {
            if cell.cursor {
                return emit(cell, Move::West, GoToLsb(cell.n));
            }
            if cell.lsb {
                return stay(cell, Composite);
            }
            emit(cell, Move::West, SeekCursorWest)
        }
        GoToLsb(candidate_bit) => {
            if cell.lsb {
                let previous = cell.remainder;
                cell.remainder = candidate_bit;
                emit(cell, Move::East, ShiftRemainderEast(previous))
            } else {
                emit(cell, Move::West, GoToLsb(candidate_bit))
            }
        }
        ShiftRemainderEast(carry) => {
            let previous = cell.remainder;
            cell.remainder = carry;
            if cell.msb {
                return stay(cell, CompareRemainderWest(Relation::Equal));
            }
            emit(cell, Move::East, ShiftRemainderEast(previous))
        }
        CompareRemainderWest(relation) => {
            let relation = relation.update(cell.remainder, cell.divisor);
            if cell.lsb {
                let phase = if relation == Relation::Less {
                    SeekCursorToAdvanceEast
                } else {
                    SubtractDivisorEast(false)
                };
                return stay(cell, phase);
            }
            emit(cell, Move::West, CompareRemainderWest(relation))
        }
        SubtractDivisorEast(borrow) => {
            let minuend = cell.remainder as u8;
            let subtrahend = cell.divisor as u8 + borrow as u8;
            let difference = if minuend >= subtrahend {
                minuend - subtrahend
            } else {
                minuend + 2 - subtrahend
            };
            cell.remainder = difference == 1;
            let next_borrow = minuend < subtrahend;
            if cell.msb {
                return stay(cell, SeekCursorToAdvanceWest);
            }
            emit(cell, Move::East, SubtractDivisorEast(next_borrow))
        }
        SeekCursorToAdvanceEast => {
            if cell.cursor {
                cell.cursor = false;
                if cell.lsb {
                    let nonzero = cell.remainder;
                    return emit(cell, Move::East, CheckRemainderEast(nonzero));
                }
                return emit(cell, Move::West, PlaceCursor(cell.n));
            }
            if cell.msb {
                return stay(cell, Composite);
            }
            emit(cell, Move::East, SeekCursorToAdvanceEast)
        }
        SeekCursorToAdvanceWest => {
            if cell.cursor {
                cell.cursor = false;
                if cell.lsb {
                    let nonzero = cell.remainder;
                    return emit(cell, Move::East, CheckRemainderEast(nonzero));
                }
                return emit(cell, Move::West, PlaceCursor(cell.n));
            }
            if cell.lsb {
                return stay(cell, Composite);
            }
            emit(cell, Move::West, SeekCursorToAdvanceWest)
        }
        PlaceCursor(_) => {
            cell.cursor = true;
            if cell.lsb {
                let previous = cell.remainder;
                cell.remainder = cell.n;
                emit(cell, Move::East, ShiftRemainderEast(previous))
            } else {
                emit(cell, Move::West, GoToLsb(cell.n))
            }
        }
        CheckRemainderEast(nonzero) => {
            let nonzero = nonzero || cell.remainder;
            if cell.msb {
                if nonzero {
                    if cell.single_divisor {
                        return stay(cell, Prime);
                    }
                    return emit(cell, Move::West, GoToLsbForUpdate);
                }
                return stay(cell, Composite);
            }
            emit(cell, Move::East, CheckRemainderEast(nonzero))
        }
        GoToLsbForUpdate => {
            if cell.lsb {
                return stay(
                    cell,
                    UpdateDivisorEast {
                        divisor_carry: true,
                        square_carry: true,
                        previous_divisor_bit: false,
                    },
                );
            }
            emit(cell, Move::West, GoToLsbForUpdate)
        }
        UpdateDivisorEast {
            divisor_carry,
            square_carry,
            previous_divisor_bit,
        } => {
            let old_divisor = cell.divisor;
            let divisor_sum = old_divisor as u8 + divisor_carry as u8;
            cell.divisor = divisor_sum & 1 != 0;
            let next_divisor_carry = divisor_sum >> 1 != 0;

            let square_sum = cell.square as u8 + previous_divisor_bit as u8 + square_carry as u8;
            cell.square = square_sum & 1 != 0;
            let next_square_carry = square_sum >> 1 != 0;

            if cell.msb {
                return stay(cell, CompareSquare(Relation::Equal));
            }
            emit(
                cell,
                Move::East,
                UpdateDivisorEast {
                    divisor_carry: next_divisor_carry,
                    square_carry: next_square_carry,
                    previous_divisor_bit: old_divisor,
                },
            )
        }
    }
}

fn row_token_is_composite(phase: Phase) -> Option<bool> {
    match phase {
        Phase::RowPrimeEast => Some(false),
        Phase::RowCompositeEast => Some(true),
        _ => None,
    }
}

fn column_token_is_composite(phase: Phase) -> Option<bool> {
    match phase {
        Phase::ColumnPrimeNorth => Some(false),
        Phase::ColumnCompositeNorth => Some(true),
        _ => None,
    }
}

fn worker_result_is_composite(phase: Phase) -> Option<bool> {
    match phase {
        Phase::Prime => Some(false),
        Phase::Composite => Some(true),
        _ => None,
    }
}

fn final_result_phase(composite: bool) -> Phase {
    if composite {
        Phase::CandidateComposite
    } else {
        Phase::CandidatePrime
    }
}

fn grid_reducer_next(mut center: WorkerCell, neighbours: [usize; 8]) -> Option<WorkerCell> {
    if center.wire_role == WireRole::None {
        return None;
    }
    let west = WorkerCell::decode(neighbours[WEST]);
    let east = WorkerCell::decode(neighbours[EAST]);
    let south = WorkerCell::decode(neighbours[1]);
    let north = WorkerCell::decode(neighbours[6]);
    let south_east = WorkerCell::decode(neighbours[2]);

    match center.phase {
        Phase::RowPrimeEast | Phase::RowCompositeEast => {
            let composite = row_token_is_composite(center.phase)?;
            let can_enter = east
                .filter(|cell| {
                    matches!(
                        cell.wire_role,
                        WireRole::Row | WireRole::Port | WireRole::RowEnd
                    )
                })
                .map(|target| {
                    if composite || target.wire_role != WireRole::Port {
                        true
                    } else {
                        south_east
                            .and_then(|cell| worker_result_is_composite(cell.phase))
                            .is_some()
                    }
                })
                .unwrap_or(false);
            if can_enter {
                center.phase = Phase::RowWire;
            }
            Some(center)
        }
        Phase::RowWire => {
            let Some(incoming) = west.and_then(|cell| row_token_is_composite(cell.phase)) else {
                return Some(center);
            };
            let mut composite = incoming;
            if center.wire_role == WireRole::Port {
                if let Some(worker_composite) =
                    south.and_then(|cell| worker_result_is_composite(cell.phase))
                {
                    composite |= worker_composite;
                } else if !incoming {
                    return Some(center);
                }
            }
            center.phase = if center.wire_role == WireRole::RowEnd {
                if composite {
                    Phase::RowCompositeReady
                } else {
                    Phase::RowPrimeReady
                }
            } else if composite {
                Phase::RowCompositeEast
            } else {
                Phase::RowPrimeEast
            };
            Some(center)
        }
        Phase::RowPrimeReady | Phase::RowCompositeReady => {
            let row_composite = center.phase == Phase::RowCompositeReady;
            if center.bottom {
                center.phase = if center.top {
                    final_result_phase(row_composite)
                } else if row_composite {
                    Phase::ColumnCompositeNorth
                } else {
                    Phase::ColumnPrimeNorth
                };
                return Some(center);
            }
            let Some(incoming) = south.and_then(|cell| column_token_is_composite(cell.phase))
            else {
                return Some(center);
            };
            let composite = row_composite || incoming;
            center.phase = if center.top {
                final_result_phase(composite)
            } else if composite {
                Phase::ColumnCompositeNorth
            } else {
                Phase::ColumnPrimeNorth
            };
            Some(center)
        }
        Phase::ColumnPrimeNorth | Phase::ColumnCompositeNorth => {
            let target_ready = north.map(|cell| {
                matches!(cell.phase, Phase::RowPrimeReady | Phase::RowCompositeReady)
                    || matches!(cell.wire_role, WireRole::Column)
            });
            if target_ready == Some(true) {
                center.phase = Phase::ColumnWire;
            }
            Some(center)
        }
        Phase::ColumnWire => {
            if let Some(composite) = south.and_then(|cell| column_token_is_composite(cell.phase)) {
                center.phase = if composite {
                    Phase::ColumnCompositeNorth
                } else {
                    Phase::ColumnPrimeNorth
                };
            }
            Some(center)
        }
        Phase::CandidatePrime | Phase::CandidateComposite => Some(center),
        _ => None,
    }
}

/// Autonomous finite-state CA rule for one binary trial-division worker.
pub struct PrimeWorkerRule;

impl LocalRule for PrimeWorkerRule {
    fn state_count(&self) -> usize {
        1 + Phase::COUNT * CELL_PAYLOAD
    }

    fn next(&self, center: usize, neighbours: [usize; 8]) -> usize {
        let Some(center) = WorkerCell::decode(center) else {
            let Some(west) = WorkerCell::decode(neighbours[WEST]) else {
                return 0;
            };
            let incoming = transition_at(west);
            if incoming.target_phase == Some((Move::East, Phase::CounterBirthEast)) {
                return WorkerCell {
                    n: true,
                    msb: true,
                    phase: Phase::CounterReturnWest,
                    ..WorkerCell::tape(false, false, false, false, false, false)
                }
                .encode();
            }
            if incoming.target_phase == Some((Move::East, Phase::CounterStrideBirthEast)) {
                return WorkerCell {
                    n: true,
                    msb: true,
                    phase: Phase::CounterStrideReturnWest,
                    ..WorkerCell::tape(false, false, false, false, false, false)
                }
                .encode();
            }
            return 0;
        };
        if let Some(reduced) = grid_reducer_next(center, neighbours) {
            return reduced.encode();
        }
        let own = transition_at(center);
        let mut next = own.source;
        if own.target_phase == Some((Move::Stay, next.phase)) {
            next.phase = own.source.phase;
        }

        for (slot, movement) in [(WEST, Move::East), (EAST, Move::West)] {
            let Some(neighbour) = neighbours[slot].checked_sub(0).and_then(WorkerCell::decode)
            else {
                continue;
            };
            if neighbour.phase == Phase::Tape
                || neighbour.phase == Phase::Prime
                || neighbour.phase == Phase::Composite
            {
                continue;
            }
            let incoming = transition_at(neighbour);
            if let Some((incoming_move, phase)) = incoming.target_phase {
                if incoming_move == movement {
                    if next.phase == Phase::Tape {
                        next.phase = phase;
                    }
                }
            }
        }

        if next.single_divisor && next.msb {
            let incoming = neighbours[1].checked_sub(0).and_then(WorkerCell::decode);
            if next.bottom {
                next.phase = match next.phase {
                    Phase::Prime if next.top => Phase::CandidatePrime,
                    Phase::Prime => Phase::PassPrime,
                    Phase::Composite if next.top => Phase::CandidateComposite,
                    Phase::Composite => Phase::PassComposite,
                    phase => phase,
                };
            } else if let Some(south) = incoming {
                match (south.phase, next.phase) {
                    (
                        Phase::PassComposite,
                        Phase::Tape
                        | Phase::Prime
                        | Phase::Composite
                        | Phase::CompareSquare(_)
                        | Phase::ClearRemainderEast
                        | Phase::SeekCursorEast
                        | Phase::SeekCursorWest
                        | Phase::GoToLsb(_)
                        | Phase::ShiftRemainderEast(_)
                        | Phase::CompareRemainderWest(_)
                        | Phase::SubtractDivisorEast(_)
                        | Phase::SeekCursorToAdvanceEast
                        | Phase::SeekCursorToAdvanceWest
                        | Phase::PlaceCursor(_)
                        | Phase::CheckRemainderEast(_)
                        | Phase::GoToLsbForUpdate
                        | Phase::UpdateDivisorEast { .. }
                        | Phase::ClearRemainderWest,
                    ) => {
                        next.phase = if next.top {
                            Phase::CandidateComposite
                        } else {
                            Phase::PassComposite
                        };
                    }
                    (Phase::PassPrime, Phase::Prime) => {
                        next.phase = if next.top {
                            Phase::CandidatePrime
                        } else {
                            Phase::PassPrime
                        };
                    }
                    (Phase::PassPrime, Phase::Composite) => {
                        next.phase = if next.top {
                            Phase::CandidateComposite
                        } else {
                            Phase::PassComposite
                        };
                    }
                    _ => {}
                }
            }
        }
        next.encode()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkerResult {
    Prime,
    Composite,
}

pub struct PrimeWorker {
    pub config: LogicalConfig,
    pub steps: usize,
}

pub struct DivisorWorker {
    pub config: LogicalConfig,
    pub steps: usize,
}

pub struct ParallelCandidate {
    pub config: LogicalConfig,
    pub steps: usize,
}

/// A candidate tile whose \(\Theta(\sqrt n)\) divisor workers occupy an
/// \(n^{1/4}\times n^{1/4}\) mesh.  Row and column accumulator waves have linear-in-diameter
/// latency; unlike the one-dimensional `ParallelCandidate`, their in-flight footprint is o(n).
pub struct MeshCandidate {
    pub config: LogicalConfig,
    pub steps: usize,
    pub columns: usize,
    pub rows: usize,
    pub output: LogicalPoint,
}

pub fn worker_rule_state_count() -> usize {
    PrimeWorkerRule.state_count()
}

pub fn candidate_result_code(state: usize) -> Option<bool> {
    let cell = WorkerCell::decode(state)?;
    match cell.phase {
        Phase::CandidatePrime => Some(false),
        Phase::CandidateComposite => Some(true),
        _ => None,
    }
}

pub struct ParallelCandidateField {
    pub config: LogicalConfig,
    pub steps: usize,
    candidates: Vec<(usize, LogicalPoint)>,
    widths: Vec<i32>,
}

pub struct BinaryCounter {
    pub config: LogicalConfig,
    last_value: usize,
}

pub struct StrideCounter {
    pub config: LogicalConfig,
    pub value_snapshot: usize,
    stride: usize,
}

pub struct CounterBank {
    pub config: LogicalConfig,
    lanes: usize,
    last_batch: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct PipelineRecord {
    pub candidate: usize,
    pub result: CandidateResult,
    pub launched_at: usize,
    pub completed_at: usize,
}

/// An integration harness for the local launcher and local candidate tiles.
///
/// The counter and every classification tile are CA configurations.  Rust only supplies the
/// geometric placement and global clock needed to compare timestamps.  A production scheduler
/// will encode that placement and clock as finite-state wires and a FIFO.
pub struct CandidateBatchPipeline {
    bank: CounterBank,
    clock: usize,
    next_release: usize,
    pending: BTreeMap<usize, Vec<Option<PipelineRecord>>>,
    released: Vec<PipelineRecord>,
}

fn width_for(candidate: usize) -> usize {
    let bits = usize::BITS as usize - candidate.leading_zeros() as usize;
    bits.max(2) + 1
}

fn binary_bits(value: usize, width: usize) -> Vec<bool> {
    (0..width)
        .map(|bit| bit < usize::BITS as usize && value & (1usize << bit) != 0)
        .collect()
}

impl PrimeWorker {
    pub fn new(candidate: usize) -> Result<Self, String> {
        if candidate < 2 {
            return Err("a trial-division worker needs an integer of at least 2".to_string());
        }
        let width = width_for(candidate);
        let n = binary_bits(candidate, width);
        let divisor = binary_bits(2, width);
        let square = binary_bits(4, width);
        let mut config = LogicalConfig::new();
        for x in 0..width {
            let mut cell =
                WorkerCell::tape(n[x], divisor[x], square[x], false, x == 0, x + 1 == width);
            if x + 1 == width {
                cell.phase = Phase::CompareSquare(Relation::Equal);
            }
            config.insert((x as i32, 0), cell.encode());
        }
        Ok(PrimeWorker { config, steps: 0 })
    }

    pub fn run_until_result(&mut self, max_steps: usize) -> Result<WorkerResult, String> {
        let rule = PrimeWorkerRule;
        for _ in 0..max_steps {
            self.steps += 1;
            self.config = rule.step(&self.config)?;
            if let Some(result) = self.result() {
                return Ok(result);
            }
        }
        Err(format!(
            "worker did not classify its candidate in {} local steps",
            max_steps
        ))
    }

    pub fn result(&self) -> Option<WorkerResult> {
        self.config.values().find_map(|&state| {
            let phase = WorkerCell::decode(state)?.phase;
            match phase {
                Phase::Prime => Some(WorkerResult::Prime),
                Phase::Composite => Some(WorkerResult::Composite),
                _ => None,
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DivisorResult {
    Divides,
    DoesNotDivide,
}

impl DivisorWorker {
    pub fn new(candidate: usize, divisor: usize) -> Result<Self, String> {
        if divisor == 0 {
            return Err("a divisor worker needs a nonzero divisor".to_string());
        }
        if candidate < 2 || divisor > candidate {
            return Err(
                "a divisor worker needs 1 < candidate and divisor <= candidate".to_string(),
            );
        }
        let width = width_for(candidate);
        let n = binary_bits(candidate, width);
        let divisor = binary_bits(divisor, width);
        let mut config = LogicalConfig::new();
        for x in 0..width {
            let mut cell = WorkerCell::tape(n[x], divisor[x], false, false, x == 0, x + 1 == width);
            cell.single_divisor = true;
            if x + 1 == width {
                cell.cursor = true;
                cell.phase = Phase::ClearRemainderWest;
            }
            config.insert((x as i32, 0), cell.encode());
        }
        Ok(DivisorWorker { config, steps: 0 })
    }

    pub fn run_until_result(&mut self, max_steps: usize) -> Result<DivisorResult, String> {
        let rule = PrimeWorkerRule;
        for _ in 0..max_steps {
            self.steps += 1;
            self.config = rule.step(&self.config)?;
            if let Some(result) = self.result() {
                return Ok(result);
            }
        }
        Err(format!(
            "divisor worker did not finish in {} local steps",
            max_steps
        ))
    }

    pub fn result(&self) -> Option<DivisorResult> {
        self.config.values().find_map(|&state| {
            let phase = WorkerCell::decode(state)?.phase;
            match phase {
                Phase::Prime => Some(DivisorResult::DoesNotDivide),
                Phase::Composite => Some(DivisorResult::Divides),
                _ => None,
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateResult {
    Prime,
    Composite,
}

impl ParallelCandidate {
    pub fn new(candidate: usize) -> Result<Self, String> {
        if candidate < 4 {
            return Err("parallel trial division starts at candidate 4".to_string());
        }
        let limit = integer_sqrt(candidate);
        let mut config = LogicalConfig::new();
        for divisor in 2..=limit {
            let worker = DivisorWorker::new(candidate, divisor)?;
            for ((x, _), state) in worker.config {
                let mut cell = WorkerCell::decode(state)
                    .ok_or_else(|| "divisor worker produced an invalid state".to_string())?;
                cell.bottom = divisor == 2;
                cell.top = divisor == limit;
                config.insert((x, (divisor - 2) as i32), cell.encode());
            }
        }
        Ok(ParallelCandidate { config, steps: 0 })
    }

    pub fn run_until_result(&mut self, max_steps: usize) -> Result<CandidateResult, String> {
        let rule = PrimeWorkerRule;
        for _ in 0..max_steps {
            self.steps += 1;
            self.config = rule.step(&self.config)?;
            if let Some(result) = self.result() {
                return Ok(result);
            }
        }
        Err(format!(
            "parallel candidate did not finish in {} local steps",
            max_steps
        ))
    }

    pub fn result(&self) -> Option<CandidateResult> {
        self.config.values().find_map(|&state| {
            let phase = WorkerCell::decode(state)?.phase;
            match phase {
                Phase::CandidatePrime => Some(CandidateResult::Prime),
                Phase::CandidateComposite => Some(CandidateResult::Composite),
                _ => None,
            }
        })
    }
}

impl WorkerCell {
    fn wire(role: WireRole, phase: Phase) -> Self {
        WorkerCell {
            wire_role: role,
            phase,
            ..WorkerCell::tape(false, false, false, false, false, false)
        }
    }
}

impl MeshCandidate {
    pub fn new(candidate: usize) -> Result<Self, String> {
        if candidate < 4 {
            return Err("mesh trial division starts at candidate 4".to_string());
        }
        let limit = integer_sqrt(candidate);
        let divisor_count = limit.saturating_sub(1);
        if divisor_count == 0 {
            return Err("candidate has no trial divisors".to_string());
        }
        let columns = integer_sqrt(divisor_count).max(1).min(divisor_count);
        let columns = if columns * columns < divisor_count {
            columns + 1
        } else {
            columns
        };
        let rows = divisor_count.div_ceil(columns);
        let width = width_for(candidate);
        let block_stride = i32::try_from(
            width
                .checked_add(2)
                .ok_or_else(|| "mesh worker width overflowed".to_string())?,
        )
        .map_err(|_| "mesh worker width does not fit i32".to_string())?;
        let row_end = i32::try_from(columns)
            .map_err(|_| "mesh column count does not fit i32".to_string())?
            .checked_mul(block_stride)
            .ok_or_else(|| "mesh width overflowed".to_string())?;
        let mut config = LogicalConfig::new();

        for index in 0..divisor_count {
            let divisor = index + 2;
            let column = index % columns;
            let row = index / columns;
            let x0 = i32::try_from(column)
                .map_err(|_| "mesh column does not fit i32".to_string())?
                .checked_mul(block_stride)
                .ok_or_else(|| "mesh x coordinate overflowed".to_string())?;
            let y0 = i32::try_from(row)
                .map_err(|_| "mesh row does not fit i32".to_string())?
                .checked_mul(3)
                .ok_or_else(|| "mesh y coordinate overflowed".to_string())?;
            let worker = DivisorWorker::new(candidate, divisor)?;
            for ((x, _), state) in worker.config {
                let point = (
                    x0.checked_add(x)
                        .ok_or_else(|| "mesh worker exceeds the x axis".to_string())?,
                    y0,
                );
                if config.insert(point, state).is_some() {
                    return Err(format!("mesh worker overlap at {:?}", point));
                }
            }
        }

        for row in 0..rows {
            let wire_y = i32::try_from(row)
                .map_err(|_| "mesh row does not fit i32".to_string())?
                .checked_mul(3)
                .and_then(|y| y.checked_add(1))
                .ok_or_else(|| "mesh wire y coordinate overflowed".to_string())?;
            for x in -1..=row_end {
                let role = if x == row_end {
                    WireRole::RowEnd
                } else {
                    let local = x.rem_euclid(block_stride);
                    let column = x.div_euclid(block_stride);
                    let has_worker = column >= 0
                        && (column as usize) < columns
                        && row * columns + (column as usize) < divisor_count;
                    if local == width as i32 - 1 && has_worker {
                        WireRole::Port
                    } else {
                        WireRole::Row
                    }
                };
                let mut cell = WorkerCell::wire(role, Phase::RowWire);
                if x == -1 {
                    cell.phase = Phase::RowPrimeEast;
                }
                if role == WireRole::RowEnd {
                    cell.bottom = row == 0;
                    cell.top = row + 1 == rows;
                }
                if config.insert((x, wire_y), cell.encode()).is_some() {
                    return Err(format!("mesh wire overlap at ({},{})", x, wire_y));
                }
            }
            if row + 1 < rows {
                for offset in 1..=2 {
                    let point = (row_end, wire_y + offset);
                    let cell = WorkerCell::wire(WireRole::Column, Phase::ColumnWire);
                    if config.insert(point, cell.encode()).is_some() {
                        return Err(format!("mesh column wire overlap at {:?}", point));
                    }
                }
            }
        }

        let output_y = i32::try_from(rows - 1)
            .map_err(|_| "mesh output row does not fit i32".to_string())?
            .checked_mul(3)
            .and_then(|y| y.checked_add(1))
            .ok_or_else(|| "mesh output y coordinate overflowed".to_string())?;
        Ok(MeshCandidate {
            config,
            steps: 0,
            columns,
            rows,
            output: (row_end, output_y),
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = PrimeWorkerRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn result(&self) -> Option<CandidateResult> {
        let state = self.config.get(&self.output)?;
        match WorkerCell::decode(*state)?.phase {
            Phase::CandidatePrime => Some(CandidateResult::Prime),
            Phase::CandidateComposite => Some(CandidateResult::Composite),
            _ => None,
        }
    }

    pub fn run_until_result(&mut self, max_steps: usize) -> Result<CandidateResult, String> {
        while self.result().is_none() {
            if self.steps >= max_steps {
                let wires: Vec<_> = self
                    .config
                    .iter()
                    .filter_map(|(point, state)| {
                        let cell = WorkerCell::decode(*state)?;
                        (cell.wire_role != WireRole::None).then_some((
                            *point,
                            cell.phase,
                            cell.wire_role,
                        ))
                    })
                    .collect();
                return Err(format!(
                    "mesh candidate did not finish in {} local steps; {} cells; wires {:?}",
                    max_steps,
                    self.config.len(),
                    wires
                ));
            }
            self.step()?;
        }
        self.result()
            .ok_or_else(|| "mesh candidate lost its result".to_string())
    }
}

impl ParallelCandidateField {
    pub fn new(candidates: &[usize]) -> Result<Self, String> {
        if candidates.is_empty() {
            return Err("a candidate field needs at least one candidate".to_string());
        }
        let mut config = LogicalConfig::new();
        let mut origins = Vec::with_capacity(candidates.len());
        let mut widths = Vec::with_capacity(candidates.len());
        let mut cursor_x = 0i32;
        for &candidate in candidates {
            let tile = ParallelCandidate::new(candidate)?;
            let tile_width = width_for(candidate) as i32;
            let origin = (cursor_x, 0);
            for ((x, y), state) in tile.config {
                let point = (
                    origin
                        .0
                        .checked_add(x)
                        .ok_or_else(|| "candidate field exceeds the x axis".to_string())?,
                    y,
                );
                if config.insert(point, state).is_some() {
                    return Err(format!(
                        "candidate {} overlaps another tile at {:?}",
                        candidate, point
                    ));
                }
            }
            origins.push((candidate, origin));
            widths.push(tile_width);
            cursor_x = cursor_x
                .checked_add(tile_width + 2)
                .ok_or_else(|| "candidate field exceeds the x axis".to_string())?;
        }
        Ok(ParallelCandidateField {
            config,
            steps: 0,
            candidates: origins,
            widths,
        })
    }

    pub fn step(&mut self) -> Result<(), String> {
        self.config = PrimeWorkerRule.step(&self.config)?;
        self.steps += 1;
        Ok(())
    }

    pub fn run_until_complete(&mut self, max_steps: usize) -> Result<Vec<CandidateResult>, String> {
        while self.results().iter().any(Option::is_none) {
            if self.steps >= max_steps {
                return Err(format!(
                    "candidate field did not finish in {} synchronous steps",
                    max_steps
                ));
            }
            self.step()?;
        }
        self.results()
            .into_iter()
            .map(|result| result.ok_or_else(|| "candidate field lost a result".to_string()))
            .collect()
    }

    pub fn results(&self) -> Vec<Option<CandidateResult>> {
        self.candidates
            .iter()
            .zip(&self.widths)
            .map(|((_, origin), width)| {
                let x = origin.0 + width - 1;
                let max_y = self
                    .config
                    .keys()
                    .filter(|(px, _)| *px >= origin.0 && *px <= x)
                    .map(|(_, y)| *y)
                    .max()
                    .unwrap_or(0);
                let state = self.config.get(&(x, max_y))?;
                match WorkerCell::decode(*state)?.phase {
                    Phase::CandidatePrime => Some(CandidateResult::Prime),
                    Phase::CandidateComposite => Some(CandidateResult::Composite),
                    _ => None,
                }
            })
            .collect()
    }

    pub fn candidates(&self) -> impl Iterator<Item = usize> + '_ {
        self.candidates.iter().map(|(candidate, _)| *candidate)
    }
}

impl BinaryCounter {
    pub fn new(value: usize) -> Result<Self, String> {
        if value == 0 {
            return Err("the candidate counter starts at a positive integer".to_string());
        }
        let width = (usize::BITS - value.leading_zeros()) as usize;
        let bits = binary_bits(value, width);
        let mut config = LogicalConfig::new();
        for x in 0..width {
            let mut cell = WorkerCell::tape(bits[x], false, false, false, x == 0, x + 1 == width);
            if x == 0 {
                cell.phase = Phase::CounterIncrementEast;
            }
            config.insert((x as i32, 0), cell.encode());
        }
        Ok(BinaryCounter {
            config,
            last_value: value,
        })
    }

    pub fn value(&self) -> Option<usize> {
        let mut value = 0usize;
        for (point, state) in &self.config {
            let cell = WorkerCell::decode(*state)?;
            if point.0 < 0 || point.0 >= usize::BITS as i32 {
                return None;
            }
            value |= (cell.n as usize) << point.0;
        }
        Some(value)
    }

    /// Runs the local rule until the carrying head returns to the LSB and starts another tick.
    pub fn increment(&mut self) -> Result<usize, String> {
        let rule = PrimeWorkerRule;
        let max_steps = self.config.len().saturating_mul(2).saturating_add(4);
        for _ in 0..max_steps {
            self.config = rule.step(&self.config)?;
            let at_lsb_increment = self.config.values().any(|state| {
                WorkerCell::decode(*state)
                    .map(|cell| cell.lsb && cell.phase == Phase::CounterIncrementEast)
                    .unwrap_or(false)
            });
            if !at_lsb_increment {
                continue;
            }
            let value = self
                .value()
                .ok_or_else(|| "counter value does not fit in usize".to_string())?;
            if value > self.last_value {
                self.last_value = value;
                return Ok(value);
            }
        }
        Err("binary counter did not complete its local increment".to_string())
    }
}

impl StrideCounter {
    pub fn new(value: usize, stride: usize) -> Result<Self, String> {
        if value == 0 || stride == 0 {
            return Err("a stride counter needs positive value and stride".to_string());
        }
        let largest = value
            .checked_add(stride)
            .ok_or_else(|| "stride counter initial range exceeds usize".to_string())?;
        let width = (usize::BITS - largest.leading_zeros()) as usize + 1;
        Self::with_width(value, stride, width)
    }

    fn with_width(value: usize, stride: usize, width: usize) -> Result<Self, String> {
        let required = usize::BITS as usize - value.leading_zeros() as usize;
        if width < required || width >= usize::BITS as usize {
            return Err("stride counter width is insufficient or too large".to_string());
        }
        let value_bits = binary_bits(value, width);
        let stride_bits = binary_bits(stride, width);
        let mut config = LogicalConfig::new();
        for x in 0..width {
            let mut cell = WorkerCell::tape(
                value_bits[x],
                stride_bits[x],
                false,
                false,
                x == 0,
                x + 1 == width,
            );
            if x == 0 {
                cell.phase = Phase::CounterAddEast(false);
            }
            config.insert((x as i32, 0), cell.encode());
        }
        Ok(StrideCounter {
            config,
            value_snapshot: value,
            stride,
        })
    }

    pub fn value(&self) -> Option<usize> {
        counter_value(&self.config, 0)
    }

    pub fn advance(&mut self) -> Result<usize, String> {
        let rule = PrimeWorkerRule;
        let max_steps = self.config.len().saturating_mul(3).saturating_add(4);
        for _ in 0..max_steps {
            self.config = rule.step(&self.config)?;
            if !counter_ready(&self.config, 0, Phase::CounterAddEast(false)) {
                continue;
            }
            let value = self
                .value()
                .ok_or_else(|| "stride counter does not fit in usize".to_string())?;
            let expected = self
                .value_snapshot
                .checked_add(self.stride)
                .ok_or_else(|| "stride counter overflowed usize".to_string())?;
            if value == expected {
                self.value_snapshot = value;
                return Ok(value);
            }
        }
        Err("stride counter did not finish its local addition".to_string())
    }
}

impl CounterBank {
    pub fn new(start: usize, lanes: usize) -> Result<Self, String> {
        if lanes == 0 {
            return Err("a counter bank needs at least one lane".to_string());
        }
        let last_value = start
            .checked_add(lanes - 1)
            .ok_or_else(|| "counter bank initial batch exceeds usize".to_string())?;
        let largest_next = last_value
            .checked_add(lanes)
            .ok_or_else(|| "counter bank next batch exceeds usize".to_string())?;
        let width = (usize::BITS - largest_next.leading_zeros()) as usize + 1 + LAUNCHER_GUARD_BITS;
        let mut config = LogicalConfig::new();
        let mut batch = Vec::with_capacity(lanes);
        for lane in 0..lanes {
            let value = start
                .checked_add(lane)
                .ok_or_else(|| "counter bank initial batch exceeds usize".to_string())?;
            let counter = StrideCounter::with_width(value, lanes, width)?;
            for ((x, _), state) in counter.config {
                config.insert((x, 2 * lane as i32), state);
            }
            batch.push(value);
        }
        Ok(CounterBank {
            config,
            lanes,
            last_batch: batch,
        })
    }

    pub fn batch(&self) -> Option<Vec<usize>> {
        (0..self.lanes)
            .map(|lane| counter_value(&self.config, 2 * lane as i32))
            .collect()
    }

    pub fn advance_batch(&mut self) -> Result<Vec<usize>, String> {
        let rule = PrimeWorkerRule;
        let max_steps = self.config.len().saturating_mul(3).saturating_add(8);
        for _ in 0..max_steps {
            self.config = rule.step(&self.config)?;
            let ready = (0..self.lanes).all(|lane| {
                counter_ready(&self.config, 2 * lane as i32, Phase::CounterAddEast(false))
            });
            if !ready {
                continue;
            }
            let batch = self
                .batch()
                .ok_or_else(|| "counter bank contains an invalid lane".to_string())?;
            let stride = self.lanes;
            if batch
                .iter()
                .zip(&self.last_batch)
                .any(|(now, old)| now.checked_sub(*old) != Some(stride))
            {
                return Err("counter bank lanes did not advance by the lane stride".to_string());
            }
            self.last_batch.clone_from(&batch);
            return Ok(batch);
        }
        let pending: Vec<usize> = (0..self.lanes)
            .filter(|lane| {
                !counter_ready(&self.config, 2 * *lane as i32, Phase::CounterAddEast(false))
            })
            .collect();
        let heads: Vec<_> = pending
            .iter()
            .map(|lane| {
                let row = 2 * *lane as i32;
                self.config
                    .iter()
                    .filter_map(|((x, y), state)| {
                        (*y == row).then(|| {
                            WorkerCell::decode(*state)
                                .map(|cell| (*x, cell.phase, cell.lsb, cell.msb))
                        })?
                    })
                    .filter(|(_, phase, _, _)| *phase != Phase::Tape)
                    .collect::<Vec<_>>()
            })
            .collect();
        Err(format!(
            "counter bank did not complete its local batch; pending lanes {:?}, values {:?}, heads {:?}",
            pending,
            self.batch(),
            heads
        ))
    }
}

impl CandidateBatchPipeline {
    pub fn new(start: usize, lanes: usize) -> Result<Self, String> {
        let bank = CounterBank::new(start, lanes)?;
        Ok(CandidateBatchPipeline {
            bank,
            clock: 0,
            next_release: start,
            pending: BTreeMap::new(),
            released: Vec::new(),
        })
    }

    fn launch_current_batch(&mut self) -> Result<(), String> {
        let candidates = self
            .bank
            .batch()
            .ok_or_else(|| "counter bank lost a candidate".to_string())?;
        let launch = self.clock;
        let mut records = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let mut tile = MeshCandidate::new(candidate)?;
            let result = tile.run_until_result(500_000)?;
            records.push(Some(PipelineRecord {
                candidate,
                result,
                launched_at: launch,
                completed_at: launch + tile.steps,
            }));
        }
        self.pending.insert(
            records
                .first()
                .map(|record| record.as_ref().unwrap().candidate)
                .unwrap(),
            records,
        );
        Ok(())
    }

    pub fn step(&mut self) -> Result<Vec<PipelineRecord>, String> {
        if self.clock == 0 {
            self.launch_current_batch()?;
        }
        self.clock += 1;
        if let Err(error) = self.bank.advance_batch() {
            return Err(format!("local candidate launcher failed: {}", error));
        }
        self.launch_current_batch()?;

        let mut newly_released = Vec::new();
        loop {
            let key = self.pending.iter().find_map(|(first, records)| {
                records
                    .iter()
                    .any(|record| {
                        record
                            .as_ref()
                            .map(|record| record.candidate == self.next_release)
                            .unwrap_or(false)
                    })
                    .then_some(*first)
            });
            let Some(key) = key else {
                break;
            };
            let records = self.pending.get_mut(&key).unwrap();
            let position = records
                .iter()
                .position(|record| {
                    record
                        .as_ref()
                        .map(|record| record.candidate == self.next_release)
                        .unwrap_or(false)
                })
                .unwrap();
            let record = records[position]
                .take()
                .ok_or_else(|| "ordered candidate was already consumed".to_string())?;
            if record.completed_at > self.clock {
                records[position] = Some(record);
                break;
            }
            self.next_release += 1;
            newly_released.push(record);
            if records.iter().all(Option::is_none) {
                self.pending.remove(&key);
            }
        }
        self.released.extend(newly_released.iter().cloned());
        Ok(newly_released)
    }

    pub fn classifications(&self) -> &[PipelineRecord] {
        &self.released
    }
}

fn counter_ready(config: &LogicalConfig, row: i32, phase: Phase) -> bool {
    config.iter().any(|((_, y), state)| {
        *y == row
            && WorkerCell::decode(*state)
                .map(|cell| cell.lsb && cell.phase == phase)
                .unwrap_or(false)
    })
}

fn counter_value(config: &LogicalConfig, row: i32) -> Option<usize> {
    let mut value = 0usize;
    for ((x, y), state) in config {
        if *y != row {
            continue;
        }
        let cell = WorkerCell::decode(*state)?;
        if *x < 0 || *x >= usize::BITS as i32 {
            return None;
        }
        value = value.checked_add((cell.n as usize) << *x)?;
    }
    Some(value)
}

fn integer_sqrt(value: usize) -> usize {
    let mut low = 1usize;
    let mut high = value.min(1usize << (usize::BITS as usize / 2));
    while low <= high {
        let mid = low + (high - low) / 2;
        if mid <= value / mid {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    high
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_prime(candidate: usize) -> bool {
        candidate >= 2
            && (2..)
                .take_while(|divisor| divisor * divisor <= candidate)
                .all(|divisor| candidate % divisor != 0)
    }

    #[test]
    fn all_phase_codes_round_trip() {
        for code in 0..Phase::COUNT {
            let phase = Phase::decode(code).unwrap();
            assert_eq!(phase.code(), code, "phase code {}", code);
        }
    }

    #[test]
    fn worker_classifies_every_small_integer_autonomously() {
        for candidate in 2..=100 {
            let mut worker = PrimeWorker::new(candidate).unwrap();
            let result = worker
                .run_until_result(500_000)
                .unwrap_or_else(|error| panic!("candidate {}: {}", candidate, error));
            assert_eq!(
                result == WorkerResult::Prime,
                is_prime(candidate),
                "candidate {} after {} steps",
                candidate,
                worker.steps
            );
        }
    }

    #[test]
    fn worker_runtime_has_the_expected_subquadratic_shape() {
        let mut measurements = Vec::new();
        for candidate in [31, 127, 509, 1021] {
            let mut worker = PrimeWorker::new(candidate).unwrap();
            worker.run_until_result(2_000_000).unwrap();
            measurements.push(worker.steps);
        }
        assert!(measurements.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(
            measurements[3] < 100 * measurements[0],
            "{:?}",
            measurements
        );
    }

    #[test]
    fn parallel_divisor_workers_classify_every_small_remainder() {
        for candidate in 2..=120 {
            for divisor in 1..=candidate {
                let mut worker = DivisorWorker::new(candidate, divisor).unwrap();
                let result = worker.run_until_result(20_000).unwrap();
                assert_eq!(
                    result == DivisorResult::Divides,
                    candidate % divisor == 0,
                    "{} mod {} after {} steps",
                    candidate,
                    divisor,
                    worker.steps
                );
            }
        }
    }

    #[test]
    fn parallel_divisor_latency_depends_on_bit_length_not_candidate() {
        let mut samples = Vec::new();
        for candidate in [127, 509, 1021] {
            let mut max_steps = 0;
            for divisor in 2..=candidate {
                let mut worker = DivisorWorker::new(candidate, divisor).unwrap();
                worker.run_until_result(20_000).unwrap();
                max_steps = max_steps.max(worker.steps);
            }
            samples.push(max_steps);
        }
        assert!(samples.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(samples[2] < 5_000, "{:?}", samples);
    }

    #[test]
    fn parallel_candidate_tile_classifies_without_a_host_result() {
        for candidate in 4..=200 {
            let mut tile = ParallelCandidate::new(candidate).unwrap();
            let result = tile.run_until_result(50_000).unwrap();
            assert_eq!(
                result == CandidateResult::Prime,
                is_prime(candidate),
                "candidate {} after {} steps",
                candidate,
                tile.steps
            );
        }
    }

    #[test]
    fn mesh_candidate_reduces_workers_in_two_dimensions() {
        for candidate in 4..=200 {
            let mut tile = MeshCandidate::new(candidate).unwrap();
            let result = tile
                .run_until_result(50_000)
                .unwrap_or_else(|error| panic!("candidate {}: {}", candidate, error));
            assert_eq!(
                result == CandidateResult::Prime,
                is_prime(candidate),
                "candidate {} after {} steps",
                candidate,
                tile.steps
            );
        }
    }

    #[test]
    fn mesh_diameter_grows_as_fourth_root_times_word_width() {
        let mut measurements = Vec::new();
        for candidate in [127, 509, 1021, 4093] {
            let mut tile = MeshCandidate::new(candidate).unwrap();
            tile.run_until_result(50_000).unwrap();
            measurements.push((tile.rows, tile.columns, tile.steps));
        }
        assert!(measurements.windows(2).all(|pair| pair[0].1 <= pair[1].1));
        let width = width_for(4093);
        assert!(
            measurements[3].2
                < 100 * width * width + 10 * width * (measurements[3].0 + measurements[3].1),
            "{:?}",
            measurements
        );
    }

    #[test]
    fn parallel_candidate_latency_is_polylog_plus_sqrt_reduction_depth() {
        let mut samples = Vec::new();
        for candidate in [127, 509, 1021] {
            let mut tile = ParallelCandidate::new(candidate).unwrap();
            tile.run_until_result(50_000).unwrap();
            samples.push(tile.steps);
        }
        assert!(samples[1] > samples[0] && samples[2] > samples[1]);
        assert!(samples[2] < 100 * (candidate_binary_width(1021) as usize).pow(2));
    }

    #[test]
    fn candidate_field_advances_all_tiles_under_one_synchronous_rule() {
        let candidates = [97, 98, 99, 101, 127, 128, 129];
        let mut field = ParallelCandidateField::new(&candidates).unwrap();
        let results = field.run_until_complete(10_000).unwrap();
        assert_eq!(field.candidates().collect::<Vec<_>>(), candidates);
        for (candidate, result) in candidates.into_iter().zip(results) {
            assert_eq!(
                result == CandidateResult::Prime,
                is_prime(candidate),
                "candidate {}",
                candidate
            );
        }
        assert!(field.steps < 1_000);
    }

    #[test]
    fn binary_counter_grows_and_increments_without_host_state() {
        let mut counter = BinaryCounter::new(1).unwrap();
        for expected in 2..=10_000 {
            assert_eq!(counter.increment().unwrap(), expected);
            assert_eq!(counter.value(), Some(expected));
        }
    }

    #[test]
    fn stride_counter_bank_emits_contiguous_candidate_batches() {
        let mut one = StrideCounter::new(100, 16).unwrap();
        assert_eq!(one.advance().unwrap(), 116);
        let mut bank = CounterBank::new(100, 16).unwrap();
        assert_eq!(bank.batch().unwrap(), (100..116).collect::<Vec<_>>());
        for batch in 1..=20 {
            let expected = (100 + batch * 16..116 + batch * 16).collect::<Vec<_>>();
            assert_eq!(
                bank.advance_batch()
                    .unwrap_or_else(|error| panic!("batch {}: {}", batch, error)),
                expected
            );
        }
    }

    #[test]
    fn stride_counter_bank_survives_many_bit_carries() {
        let mut bank = CounterBank::new(100, 16).unwrap();
        for batch in 1..=100 {
            let values = bank.advance_batch().unwrap();
            assert_eq!(
                values,
                (100 + batch * 16..116 + batch * 16).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn stride_counter_survives_repeated_high_bit_growth() {
        let mut counter = StrideCounter::new(100, 8).unwrap();
        for step in 1..=100 {
            assert_eq!(counter.advance().unwrap(), 100 + step * 8);
        }
        let mut sixteen = StrideCounter::new(100, 16).unwrap();
        for step in 1..=100 {
            assert_eq!(sixteen.advance().unwrap(), 100 + step * 16);
        }
    }

    #[test]
    fn local_pipeline_releases_candidates_in_numeric_order() {
        let mut pipeline = CandidateBatchPipeline::new(4, 2).unwrap();
        let mut released = Vec::new();
        for _ in 0..200 {
            released.extend(pipeline.step().unwrap());
        }
        let candidates: Vec<_> = released.iter().map(|record| record.candidate).collect();
        assert!(!candidates.is_empty());
        assert_eq!(candidates, (4..4 + candidates.len()).collect::<Vec<_>>());
        for record in released {
            assert_eq!(
                record.result == CandidateResult::Prime,
                is_prime(record.candidate),
                "candidate {}",
                record.candidate
            );
        }
    }

    #[test]
    fn maximum_candidate_tape_does_not_overflow_its_guard_bit() {
        let worker = PrimeWorker::new(usize::MAX).unwrap();
        assert_eq!(worker.config.len(), usize::BITS as usize + 1);
    }
}

fn candidate_binary_width(candidate: usize) -> u32 {
    usize::BITS - candidate.leading_zeros()
}
