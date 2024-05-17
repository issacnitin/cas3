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

// Rule can be, a corresponding set of cells are SET or UNSET

#[derive(PartialEq, Clone, Debug)]
pub enum ExpectedCellValue {
    Unset,
    Set
}

#[derive(PartialEq, Clone, Debug)]
pub struct Rule {
    expected_cell_value: ExpectedCellValue,
    elements: Vec<RuleCoordinate>,
    result: RuleResult,
}

impl Rule {
    pub fn new(len: usize) -> Self {
        Rule {
            expected_cell_value: ExpectedCellValue::Unset,
            elements: vec![RuleCoordinate::SameCoordinate; len],
            result: RuleResult::Flip,
        }
    }

    // Goes from SameCoordinate -> Positive -> Negative
    pub fn gen_next_rule(self) -> Self {

        let mut it: usize = 0;
        let mut new_vec = self.elements.clone();

        while new_vec[it] == RuleCoordinate::Negative {
            new_vec[it] = RuleCoordinate::SameCoordinate;
            it += 1;
        }
        
        if new_vec[it] == RuleCoordinate::SameCoordinate {
            new_vec[it] = RuleCoordinate::Positive;
        }
        else if new_vec[it] == RuleCoordinate::Positive {
            new_vec[it] = RuleCoordinate::Negative;
        }
        else {
            if it >= self.elements.len() {
                panic!("Overflow");
            }
            new_vec[it] = RuleCoordinate::Positive;
        }
        
        Rule {
            expected_cell_value: self.expected_cell_value,
            elements: new_vec,
            result: RuleResult::Flip
        }
    }

    pub fn apply_rule(&self, cell: &Cell, space: &Space) -> Cell {
        let mut new_cell = cell.clone();
        
        // Algorithm
        new_cell.flip();

        new_cell
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
        }

        let found_cell = space.search_cells(new_cell.get_dimensions());

        if found_cell == None {
            if self.expected_cell_value == ExpectedCellValue::Unset {
                return true;
            }
            return false;
        }
        else {
            if self.expected_cell_value == ExpectedCellValue::Unset {
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
    fn test_next_gen() {
        let mut rule : Rule = Rule {
            expected_cell_value: ExpectedCellValue::Unset,
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