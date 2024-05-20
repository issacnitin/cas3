use crate::cell::CellValue;
use crate::Cell;
use crate::Space;

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
pub struct Rule {
    expected_cell_value: CellValue,
    elements: Vec<RuleCoordinate>,
    result: RuleResult,
}

impl Rule {
    pub fn new(len: usize) -> Self {
        Rule {
            expected_cell_value: CellValue::Unset,
            elements: vec![RuleCoordinate::SameCoordinate; len],
            result: RuleResult::Flip,
        }
    }

    pub fn set_expected_cell_value(&mut self, expected: CellValue) {
        self.expected_cell_value = expected;
    }

    pub fn set_elements(&mut self, coordinates: Vec<RuleCoordinate>) {
        self.elements = coordinates;
    }

    pub fn get_elements(&self) -> Vec<RuleCoordinate> {
        return self.elements.clone();
    }

    pub fn set_result(&mut self, result: RuleResult) {
        self.result = result;
    }

    pub fn has_next_rule(&self) -> bool {
        for it in self.elements.clone() {
            if (it) != RuleCoordinate::Negative {
                return true;
            }
        }

        return false;
    }

    // Goes from SameCoordinate -> Positive -> Negative
    pub fn gen_next_rule(self) -> Self {

        let mut it: usize = 0;
        let mut new_vec = self.elements.clone();

        while new_vec[it] == RuleCoordinate::Negative {
            new_vec[it] = RuleCoordinate::SameCoordinate;
            it += 1;
        }

        if it >= self.elements.len() {
            panic!("Overflow");
        }
        
        if new_vec[it] == RuleCoordinate::SameCoordinate {
            new_vec[it] = RuleCoordinate::Positive;
        }
        else if new_vec[it] == RuleCoordinate::Positive {
            new_vec[it] = RuleCoordinate::Negative;
        }
        
        Rule {
            expected_cell_value: self.expected_cell_value,
            elements: new_vec,
            result: RuleResult::Flip
        }
    }

    pub fn apply_rule(&self, cell: &Cell, space: &Space) -> Cell {
        if self.is_rule_applicable(cell, space) {
            let mut new_cell = cell.clone();
            new_cell.flip();
            return new_cell;
        }

        cell.clone()
    }

    fn is_rule_applicable(&self, cell: &Cell, space: &Space) -> bool {
        let mut new_cell : Cell = cell.clone();
        let mut i = 0;
        for it in self.elements.iter() {
            if (*it) == RuleCoordinate::Positive {
                new_cell.set_ith_coordinate(i, new_cell.get_ith_coordinate(i) + 1);
            }
            else if (*it) == RuleCoordinate::Negative {
                new_cell.set_ith_coordinate(i, new_cell.get_ith_coordinate(i) - 1);    
            }
            i += 1;
        }

        let found_cell: Option<&Cell> = space.search_cells(new_cell.get_coordinates());

        if found_cell == None {
            if self.expected_cell_value == CellValue::Unset {
                return true;
            }
            return false;
        }
        else {
            if self.expected_cell_value == CellValue::Unset {
                if found_cell.unwrap().get_value() == CellValue::Set {
                    return false;
                } else {
                    return true;
                }
            } else {
                if found_cell.unwrap().get_value() == CellValue::Set {
                    return true;
                }
                else {
                    return false;
                }
            }
        }
    }

}

mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rule : Rule = Rule::new(10);

        assert_eq!(rule.elements.len(), 10);
        
        for it in rule.elements.iter() {
            assert_eq!(*it, RuleCoordinate::SameCoordinate);
        }
    }

    #[test]
    fn test_is_rule_applicable_2d_case1() {
        let dimensions = 2;
        
        let mut space: Space = Space::new(dimensions);
        
        // For (0,0), (0,1), (1,0), (1,1)
        // (0, 0) will be flipped if (-1, -1) is SET
        // (1, 1) will be flipped if (0, 0) is SET
        // (2, 2) will be fipped if (1, 1) is SET (second iteration)
        let mut rule: Rule = Rule::new(dimensions);
        rule.set_expected_cell_value(CellValue::Set);
        rule.set_elements(vec![RuleCoordinate::Negative, RuleCoordinate::Negative]);
        rule.set_result(RuleResult::Flip);
        space.set_rule(rule);

        let mut cell1: Cell = Cell::new(dimensions);
        cell1.set_ith_coordinate(0, 0);
        cell1.set_ith_coordinate(1, 0);
        cell1.set();
        space.push_cell(cell1);

        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(cell4);

        let mut cell5 : Cell = Cell::new(dimensions);
        cell5.set_ith_coordinate(0, 2);
        cell5.set_ith_coordinate(1, 2);
        cell5.unset();
        space.push_cell(cell5);

        let mut resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);
        
        space.apply_rules(1);

        resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![2, 2]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        // 2nd iteration
        space.apply_rules(2);
        resultant_cell = space.search_cells(vec![2,2]);

        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

    }


    #[test]
    fn test_is_rule_applicable_2d_case2() {
        let dimensions = 2;
        
        let mut space: Space = Space::new(dimensions);
        
        // For (0,0), (0,1), (1,0), (1,1)
        // (0, 0) will be flipped if (0, -1) is SET
        // (1, 1) will be flipped if (1, 0) is SET
        let mut rule: Rule = Rule::new(dimensions);
        rule.set_expected_cell_value(CellValue::Set);
        rule.set_elements(vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        rule.set_result(RuleResult::Flip);
        space.set_rule(rule);

        let mut cell1: Cell = Cell::new(dimensions);
        cell1.set_ith_coordinate(0, 0);
        cell1.set_ith_coordinate(1, 0);
        cell1.unset();
        space.push_cell(cell1);

        let mut cell2: Cell = Cell::new(dimensions);
        cell2.set_ith_coordinate(0, 1);
        cell2.set_ith_coordinate(1, 0);
        cell2.set();
        space.push_cell(cell2);

        let mut cell3: Cell = Cell::new(dimensions);
        cell3.set_ith_coordinate(0, 0);
        cell3.set_ith_coordinate(1, 1);
        cell3.unset();
        space.push_cell(cell3);
        
        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(cell4);


        let mut resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        space.apply_rules(1);

        resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

    }

    #[test]
    fn test_is_rule_applicable_2d_case3() {
        let dimensions = 2;
        
        let mut space: Space = Space::new(dimensions);
        
        // For (0,0), (0,1), (1,0), (1,1)
        // (0, 0) won't be flipped if (0, 1) is UNSET
        let mut rule: Rule = Rule::new(dimensions);
        rule.set_expected_cell_value(CellValue::Set);
        rule.set_elements(vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        rule.set_result(RuleResult::Flip);
        space.set_rule(rule);

        let mut cell1: Cell = Cell::new(dimensions);
        cell1.set_ith_coordinate(0, 0);
        cell1.set_ith_coordinate(1, 0);
        cell1.set();
        space.push_cell(cell1);

        let mut cell2: Cell = Cell::new(dimensions);
        cell2.set_ith_coordinate(0, 1);
        cell2.set_ith_coordinate(1, 0);
        cell2.unset();
        space.push_cell(cell2);

        let mut cell3: Cell = Cell::new(dimensions);
        cell3.set_ith_coordinate(0, 0);
        cell3.set_ith_coordinate(1, 1);
        cell3.unset();
        space.push_cell(cell3);
        
        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(cell4);


        let mut resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        space.apply_rules(1);

        resultant_cell = space.search_cells(vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

    }

    #[test]
    fn test_next_gen() {
        let mut rule : Rule = Rule {
            expected_cell_value: CellValue::Unset,
            elements : vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate],
            result: RuleResult::Flip
        };

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);
        
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);
    }
}