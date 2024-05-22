use crate::cell::CellValue;
use crate::cell::Cell;
use crate::graph::op_node::OpNode;
use crate::space::Space;

#[derive(PartialEq, Clone, Debug)]
pub enum RuleCoordinate {
    SameCoordinate,
    Positive,
    Negative
}


#[derive(PartialEq, Clone, Debug)]
pub enum RuleResult {
    Unset,
    Set,
    Flip
}


#[derive(PartialEq, Clone, Debug)]
pub struct RuleElement {
    expected_cell_value: CellValue,
    applicable_coordinate: Vec<RuleCoordinate>,
    // TODO: Remove after Rule-Set is developed
    result: RuleResult,
}

pub struct RuleSet {
    applicable_elements: Vec<RuleElement>, 
    condition: OpNode
}

impl RuleElement {
    pub fn new(len: usize) -> Self {
        RuleElement {
            expected_cell_value: CellValue::Unset,
            applicable_coordinate: vec![RuleCoordinate::SameCoordinate; len],
            result: RuleResult::Unset,
        }
    }

    pub fn set_expected_cell_value(&mut self, expected: CellValue) {
        self.expected_cell_value = expected;
    }

    pub fn set_applicable_coordinate(&mut self, coordinates: Vec<RuleCoordinate>) {
        self.applicable_coordinate = coordinates;
    }

    pub fn get_applicable_coordinate(&self) -> Vec<RuleCoordinate> {
        return self.applicable_coordinate.clone();
    }

    pub fn get_result(&self) -> RuleResult {
        self.result.clone()
    }

    pub fn get_expected_cell_value(&self) -> CellValue {
        self.expected_cell_value.clone()
    }

    pub fn set_result(&mut self, result: RuleResult) {
        self.result = result;
    }

    pub fn has_next_applicable_coordinate(&self) -> bool {
        for it in self.applicable_coordinate.clone() {
            if (it) != RuleCoordinate::Positive {
                return true;
            }
        }

        return !(self.result == RuleResult::Flip);
    }

    // Goes from SameCoordinate -> Positive
    pub fn gen_next_applicable_coordinate(self) -> Self {

        if self.result == RuleResult::Unset {
            return RuleElement {
                expected_cell_value: self.expected_cell_value,
                applicable_coordinate: self.applicable_coordinate.clone(),
                result: RuleResult::Set
            }
        }
        else if self.result == RuleResult::Set {
            return RuleElement {
                expected_cell_value: self.expected_cell_value,
                applicable_coordinate: self.applicable_coordinate.clone(),
                result: RuleResult::Flip
            }
        }

        let mut it: usize = 0;
        let mut new_vec = self.applicable_coordinate.clone();

        while it < new_vec.len() && new_vec[it] == RuleCoordinate::Positive {
            new_vec[it] = RuleCoordinate::SameCoordinate;
            it += 1;
        }

        if it >= self.applicable_coordinate.len() {
            panic!("Overflow");
        }
        
        if new_vec[it] == RuleCoordinate::SameCoordinate {
            new_vec[it] = RuleCoordinate::Positive;
        }
        /*
        Skipping making it negative, as is_rule_applicable already does it
        else if new_vec[it] == RuleCoordinate::Positive {
            new_vec[it] = RuleCoordinate::Negative;
        }
        */
        
        RuleElement {
            expected_cell_value: self.expected_cell_value,
            applicable_coordinate: new_vec,
            result: RuleResult::Unset
        }
    }

    pub fn apply_rule(&self, cell: &mut Cell) {
        if self.result == RuleResult::Unset {
            cell.unset();
        }
        else if self.result == RuleResult::Set {
            cell.set();
        }
        else {
            cell.flip();
        }
    }

    

}

mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rule : RuleElement = RuleElement::new(10);

        assert_eq!(rule.applicable_coordinate.len(), 10);
        
        for it in rule.applicable_coordinate.iter() {
            assert_eq!(*it, RuleCoordinate::SameCoordinate);
        }
    }


    #[test]
    fn test_apply_rule() {
        let mut rule: RuleElement = RuleElement::new(2);
        rule.set_applicable_coordinate(vec![RuleCoordinate::Positive, RuleCoordinate::Positive]);
        rule.set_expected_cell_value(CellValue::Set);
        rule.set_result(RuleResult::Set);   

        let mut space: Space = Space::new(2);
        let mut cell0: Cell = Cell::new(2);
        cell0.set();

        space.push_cell(&cell0);
        space.gen_next_iteration();

        space.set_rule(rule.clone());
        
        assert_eq!(space.get_cells().len(), 9);

        for mut it in space.get_cells().clone() {
            rule.apply_rule(&mut it);
            assert_eq!(it.get_value(), CellValue::Set);
            space.push_cell(&it);
        }

        space.gen_next_iteration();
        assert_eq!(space.get_cells().len(), 25);

        for mut it in space.get_cells().clone() {
            rule.apply_rule(&mut it);
            assert_eq!(it.get_value(), CellValue::Set);
        }

    }

    #[test]
    fn test_next_gen() {
        let mut rule : RuleElement = RuleElement {
            expected_cell_value: CellValue::Unset,
            applicable_coordinate : vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate],
            result: RuleResult::Unset
        };

        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);


        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Unset);

        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        rule = rule.gen_next_applicable_coordinate();
        assert_eq!(rule.applicable_coordinate, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

    }
}