use crate::cell::CellValue;
use crate::cell::Cell;
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
            result: RuleResult::Unset,
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

    pub fn get_result(&self) -> RuleResult {
        self.result.clone()
    }

    pub fn get_expected_cell_value(&self) -> CellValue {
        self.expected_cell_value.clone()
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

        return !(self.result == RuleResult::Flip);
    }

    // Goes from SameCoordinate -> Positive -> Negative
    pub fn gen_next_rule(self) -> Self {

        if self.result == RuleResult::Unset {
            return Rule {
                expected_cell_value: self.expected_cell_value,
                elements: self.elements.clone(),
                result: RuleResult::Set
            }
        }
        else if self.result == RuleResult::Set {
            return Rule {
                expected_cell_value: self.expected_cell_value,
                elements: self.elements.clone(),
                result: RuleResult::Flip
            }
        }

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
        let rule : Rule = Rule::new(10);

        assert_eq!(rule.elements.len(), 10);
        
        for it in rule.elements.iter() {
            assert_eq!(*it, RuleCoordinate::SameCoordinate);
        }
    }

    #[test]
    fn test_next_gen() {
        let mut rule : Rule = Rule {
            expected_cell_value: CellValue::Unset,
            elements : vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate],
            result: RuleResult::Unset
        };

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);


        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Unset);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Unset);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::SameCoordinate]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);
        
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::Positive]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::SameCoordinate, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Positive, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::SameCoordinate, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Positive, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        rule = rule.gen_next_rule();
        assert_eq!(rule.elements, vec![RuleCoordinate::Negative, RuleCoordinate::Negative, RuleCoordinate::Negative]);
        assert_eq!(rule.result, RuleResult::Flip);
    }
}