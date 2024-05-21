use crate::cell::CellValue;
use crate::cell::Cell;
use crate::space::Space;
use crate::rule::{Rule, RuleCoordinate, RuleResult};

pub struct StateExplorer {
    min_dimensions: usize,
    max_dimensions: usize,
    expected_num_set_cells: Vec<usize>
}

impl StateExplorer {
    pub fn new(min_dims:usize, max_dims: usize) -> Self {
        StateExplorer {
            min_dimensions: min_dims,
            max_dimensions : max_dims,
            // Hard-coding to prime numbers for now
            expected_num_set_cells: vec![1,2,3,5,7,11,13,17,19,23,29]
        }
    }

    pub fn explore(&self) {
        // Explore all dimensions
        for dims in self.min_dimensions..self.max_dimensions+1 {
            let mut r = Rule::new(dims);
            r.set_expected_cell_value(CellValue::Set);
            println!("Exploring dimension {}", dims);

            let mut rule_count = 0;
            let mut max_iteration = 0;
            let mut max_itr_rule : Option<Rule> = None;
            // For all possible rules
            while r.has_next_rule() {
                r = r.gen_next_rule();
                rule_count +=1;
                println!("\tExploring rule {:?} {:?}, {:?}", r.get_expected_cell_value(), r.get_elements(), r.get_result());

                let mut space: Space = Space::new(dims);
                space.set_rule(r.clone());

                // Constructing first cell
                let mut first_cell: Cell = Cell::new(dims);
                first_cell.set();
                space.push_cell(&first_cell);
                
                // println!("{:?}", space.get_cells());
                    

                // Iterations
                let mut it = 1;
                // Condition must always hold, remember prime number generation in order
                while it < self.expected_num_set_cells.len() && space.find_number_of_cells(CellValue::Set) == *self.expected_num_set_cells.get(it-1).unwrap() 
                {
                    println!("\t\tIteration {} has {} elements set out of {}", it, space.find_number_of_cells(CellValue::Set), space.len());
                    if it > max_iteration {
                        max_iteration = it;
                        max_itr_rule = Some(space.get_rule().clone());
                    }
                    self.apply_rules(&mut space, it);
                    // println!("{:?}", space.get_cells());
                    it += 1;
                }
    
                if it == self.expected_num_set_cells.len() {
                    println!("Found rule generating primes: {:?}", space.get_rule());
                    return;
                }
            }

            println!("\tExplored {} rules for dimension {}. Max values matched is {} in rule {:?}", rule_count, dims, max_iteration, max_itr_rule.unwrap());
        }

        println!("Found no rule with dimensions between {} and {} that can generate primes", self.min_dimensions, self.max_dimensions);
    }


    fn is_rule_applicable(&self, cell: &Cell, space: &Space) -> bool {
        let mut new_cell : Cell = cell.clone();
        let mut i = 0;
        for it in space.get_rule().get_elements().iter() {
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
            return false;
        }
        else {
            if space.get_rule().get_expected_cell_value() == CellValue::Unset {
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
    fn test_is_rule_applicable_2d_case1() {
        let dimensions = 2;
        let explorer: StateExplorer = StateExplorer::new(dimensions, dimensions);

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
        space.push_cell(&cell1);

        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(&cell4);

        let mut cell5 : Cell = Cell::new(dimensions);
        cell5.set_ith_coordinate(0, 2);
        cell5.set_ith_coordinate(1, 2);
        cell5.unset();
        space.push_cell(&cell5);

        let mut resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![2, 2]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);
        
        explorer.apply_rules(&mut space, 1);

        resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![2, 2]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        // 2nd iteration
        explorer.apply_rules(&mut space, 2);
        resultant_cell = space.search_cells(&vec![2,2]);

        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

    }


    #[test]
    fn test_is_rule_applicable_2d_case2() {
        let dimensions = 2;
        let explorer: StateExplorer = StateExplorer::new(dimensions, dimensions);

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
        space.push_cell(&cell1);

        let mut cell2: Cell = Cell::new(dimensions);
        cell2.set_ith_coordinate(0, 1);
        cell2.set_ith_coordinate(1, 0);
        cell2.set();
        space.push_cell(&cell2);

        let mut cell3: Cell = Cell::new(dimensions);
        cell3.set_ith_coordinate(0, 0);
        cell3.set_ith_coordinate(1, 1);
        cell3.unset();
        space.push_cell(&cell3);
        
        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(&cell4);


        let mut resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        explorer.apply_rules(&mut space, 1);

        resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

    }

    #[test]
    fn test_is_rule_applicable_2d_case3() {
        let dimensions = 2;
        let explorer: StateExplorer = StateExplorer::new(dimensions, dimensions);

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
        space.push_cell(&cell1);

        let mut cell2: Cell = Cell::new(dimensions);
        cell2.set_ith_coordinate(0, 1);
        cell2.set_ith_coordinate(1, 0);
        cell2.unset();
        space.push_cell(&cell2);

        let mut cell3: Cell = Cell::new(dimensions);
        cell3.set_ith_coordinate(0, 0);
        cell3.set_ith_coordinate(1, 1);
        cell3.unset();
        space.push_cell(&cell3);
        
        let mut cell4: Cell = Cell::new(dimensions);
        cell4.set_ith_coordinate(0, 1);
        cell4.set_ith_coordinate(1, 1);
        cell4.unset();
        space.push_cell(&cell4);


        let mut resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        explorer.apply_rules(&mut space, 1);

        resultant_cell = space.search_cells(&vec![0, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Set);

        resultant_cell = space.search_cells(&vec![0, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 0]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

        resultant_cell = space.search_cells(&vec![1, 1]);
        assert_ne!(resultant_cell, None);
        assert_eq!((*resultant_cell.unwrap()).get_value(), CellValue::Unset);

    }


}