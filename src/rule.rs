use crate::Cell;
use crate::Space;

/*
  Rules follow this structure:
  for N-dimensional cell, there are N-possible neighbours (barring diagonal elements)
  each direction correspond to an index (left - 1, up - 2, right - 3, down - 4 etc..)
  
  A rule is making the cell corresponding to one combination of indices
  set or unset
  based on another combination of indices and the cells corresponding to second combination
  is set or unset.

  Another way to look at it would be, the cell corresponding to one vector
  is set or unset, based on some combination of surrounding, is set or unset

  In a cellular automata, only one rule can be applied to one cell at a time
  
  Hence, a rule is a vector of N (= dimensions) and if ith element is 1, 
  the cell is set (1), not set(2) or flipped (3)
*/

#[derive(PartialEq)]
#[derive(Clone)]
#[derive(Debug)]
pub enum RuleResult {
    Unset,
    Set,
    Flip
}

#[derive(Debug)]
#[derive(Clone)]
pub struct Rule {
    if_applied: Vec<usize>,
    result: RuleResult
}

impl Rule {
    pub fn new(len: usize) -> Self {
        Rule {
            if_applied: vec![0; len],
            result: RuleResult::Set
        }
    }

    // Just so happens it's adding numbers in binary
    pub fn gen_next_rule(self) -> Self {
        if self.result == RuleResult::Unset {
            return Rule {
                if_applied: self.if_applied,
                result: RuleResult::Set
            }
        }
        else if self.result == RuleResult::Set {
            return Rule {
                if_applied: self.if_applied,
                result: RuleResult::Flip
            };
        }

        let mut it: usize = 0;
        let mut new_vec = self.if_applied.to_vec();

        while it < self.if_applied.len() {
            if new_vec[it] == 1 {
                new_vec[it] = 0;
                it += 1;
            }
            else {
                new_vec[it] = 1;
                return Rule {
                    if_applied: new_vec,
                    result: RuleResult::Unset
                }; 
            }
        }

        panic!("Running over dimensions while generating rules!")
    }

    pub fn apply_rule(&self, cell: &Cell, space: &Space) -> Cell {
        let mut new_cell = cell.clone();
        
        // Algorithm

        new_cell
    }

    fn _apply_rule(&self, cell: &mut Cell) {
        if self.result == RuleResult::Set {
            cell.set();
        }
        else if self.result == RuleResult::Unset {
            cell.unset();
        }
        else if self.result == RuleResult::Flip {
            cell.flip();
        }
    }

    fn is_rule_applicable(&self, cell: &Cell, space: &Space) {
        
    }

}

mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let rule : Rule = Rule::new(10);

        assert_eq!(rule.if_applied.len(), 10);
        
        for it in rule.if_applied.iter() {
            assert_eq!(*it, 0);
        }
    }

    #[test]
    fn test_next_gen() {
        let mut rule : Rule = Rule {
            if_applied : vec![1,0,1,0,1,0,1],
            result: RuleResult::Flip
        };

        rule = rule.gen_next_rule();

        assert_eq!(rule.if_applied, vec![0,1,1,0,1,0,1]);
        assert_eq!(rule.result, RuleResult::Unset);

        rule = rule.gen_next_rule();
        assert_eq!(rule.if_applied, vec![0,1,1,0,1,0,1]);
        assert_eq!(rule.result, RuleResult::Set);

        rule = rule.gen_next_rule();
        assert_eq!(rule.if_applied, vec![0,1,1,0,1,0,1]);
        assert_eq!(rule.result, RuleResult::Flip);

        rule = rule.gen_next_rule();
        assert_eq!(rule.if_applied, vec![1,1,1,0,1,0,1]);
        assert_eq!(rule.result, RuleResult::Unset);
    }
}