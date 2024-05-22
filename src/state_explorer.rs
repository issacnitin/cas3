use crate::cell::CellValue;
use crate::cell::Cell;
use crate::space::Space;
use crate::rule::{RuleElement, RuleCoordinate, RuleResult};

pub struct StateExplorer {
    min_dimensions: usize,
    max_dimensions: usize,
    expected_num_set_cells: Vec<usize>
}

impl StateExplorer {
    pub fn new(min_dims:usize, max_dims: usize, expected_set: Vec<usize>) -> Self {
        StateExplorer {
            min_dimensions: min_dims,
            max_dimensions : max_dims,
            // Hard-coding to prime numbers for now
            expected_num_set_cells: expected_set
        }
    }

    pub fn explore(&self) -> bool {
        // Explore all dimensions
        for dims in self.min_dimensions..self.max_dimensions+1 {
            let mut r = RuleElement::new(dims);
            r.set_expected_cell_value(CellValue::Set);
            println!("Exploring dimension {}", dims);

            let mut rule_count = 0;
            let mut max_iteration = 0;
            let mut max_itr_rule : Option<RuleElement> = None;
            // For all possible rules
            while r.has_next_applicable_coordinate() {
                r = r.gen_next_applicable_coordinate();
                rule_count +=1;
                // println!("\tExploring rule {:?} {:?}, {:?}", r.get_expected_cell_value(), r.get_elements(), r.get_result());

                let mut space: Space = Space::new(dims);
                space.set_rule(r.clone());

                // Constructing first cell
                let mut first_cell: Cell = Cell::new(dims);
                first_cell.set();
                space.push_cell(&first_cell.clone());
                
                // println!("{:?}", space.get_cells());
                    

                // Iterations
                let mut it = 1;
                // println!("\t\tIteration {} has {} elements set out of {}", it, space.find_number_of_cells(CellValue::Set), space.len());
                    
                // Condition must always hold, remember prime number generation in order
                while it < self.expected_num_set_cells.len() && space.find_number_of_cells(CellValue::Set) == *self.expected_num_set_cells.get(it-1).unwrap() 
                {
                    if it > max_iteration {
                        max_iteration = it;
                        max_itr_rule = Some(space.get_rule().clone());
                    }
                    self.apply_rules(&mut space, it);
                    
                    it += 1;
                    // println!("\t\tIteration {} has {} elements set out of {}", it, space.find_number_of_cells(CellValue::Set), space.len());
                }
    
                if it == self.expected_num_set_cells.len() {
                    println!("Found rule generating primes: {:?}", space.get_rule());
                    return true;
                }
            }

            println!("\tExplored {} rules for dimension {}. Max values matched is {} in rule {:?}", rule_count, dims, max_iteration, max_itr_rule.unwrap());
        }

        println!("Found no rule with dimensions between {} and {} that can generate primes", self.min_dimensions, self.max_dimensions);
        
        false
    }


    fn is_rule_applicable(&self, cell: &Cell, space: &Space) -> bool {
        // Orientation-agnostic, hence (1,2,3), (3,1,2), (2,3,1) are the same
        let mut rule = space.get_rule().get_applicable_coordinate().clone();

        let mut rule_it = 0;
        let mut negated: bool = false;
        let mut applied : bool = false;
        while rule_it < rule.len() {
            
            if !applied {
                applied = true;
            }
            else if applied && !negated {
                let mut it = 0;
                while it < rule.len() {
                    if rule[it] == RuleCoordinate::Positive {
                        rule[it] = RuleCoordinate:: Negative;
                    }
                    else if rule[it] == RuleCoordinate::Negative {
                        rule[it] = RuleCoordinate::Positive;
                    }
                    it +=1;
                }
                negated = true;
            }

            let mut new_cell : Cell = cell.clone();

            let mut i = 0;
            for it in rule.iter() {
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
                if space.get_rule().get_expected_cell_value() == CellValue::Unset {
                    return true;
                }
            }
            else {
                if space.get_rule().get_expected_cell_value() == CellValue::Unset {
                    if found_cell.unwrap().get_value() != CellValue::Set {
                        return true;
                    } 
                } else {
                    if found_cell.unwrap().get_value() == CellValue::Set {
                        return true;
                    }
                }
            }

            if applied && negated {
                // Negate to original
                let mut it = 0;
                while it < rule.len() {
                    if rule[it] == RuleCoordinate::Positive {
                        rule[it] = RuleCoordinate:: Negative;
                    }
                    else if rule[it] == RuleCoordinate::Negative {
                        rule[it] = RuleCoordinate::Positive;
                    }
                    it +=1;
                }

                rule_it += 1;
                rule.rotate_right(1);
                applied = false;
                negated = false;
            }
        }
        false
    }


    fn apply_rules(&self, space: &mut Space, intented_iteration: usize) {
        if intented_iteration < space.get_current_iteration() {
            println!("Intented iteration < self.it");
            return;
        }

        let mut it = 0;
        let mut pending: Vec<(usize, Cell)> = vec![];

        while it < space.get_cells().len() {
            let r = space.get_rule().clone();
            let mut c: Cell = space.get_ith_cell_mut(it).clone();
            if self.is_rule_applicable(&mut c, space) {
                r.apply_rule(&mut c);
                pending.push((it, c));
            }
            it += 1;
        }

        for it in pending.iter_mut() {
            space.set_ith_cell(it.0, &mut it.1);
        }

        space.gen_next_iteration();
    }
}

mod test {
    use super::*;


    #[test]
    fn test_is_rule_applicable() {
        let explorer: StateExplorer = StateExplorer::new(1, 2, vec![1,3,5,7]);
        assert_eq!(explorer.explore(), true);

        let explorer2: StateExplorer = StateExplorer::new(1, 5, vec![1,11,61,231]);
        assert_eq!(explorer2.explore(), true);
    }
}