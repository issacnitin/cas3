use std::io;
use std::io::prelude::*;

use crate::cell::CellValue;
use crate::cell::Cell;
use crate::rule::Action;
use crate::rule::Rule;
use crate::space::Space;

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
        for dim_len in self.min_dimensions..self.max_dimensions+1 {
            let mut rule = Rule::new(dim_len);
            let mut rule_counter = 0;
            // Explore all rules of given dimension
            loop {
                // For all evaluation permutations 
                loop {
                    print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                    rule_counter += 1;
    
                    let mut space: Space = Space::new(dim_len);
                    space.set_rule(&rule);
    
                    let mut __cell: Cell = Cell::new(dim_len);
                    __cell.set();
                    space.push_cell(&__cell);
    
                    let mut all_matched: bool = true;
                    let mut match_counter = 0;
                    for el in self.expected_num_set_cells.clone() {
                        print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                        println!("Exploring dimension {}", dim_len);
                        rule.debug_print();
                        space.debug_print();
                        if space.find_number_of_cells(CellValue::Set) != el {
                            all_matched = false;
                            break;
                        }
                        match_counter += 1;
                        space.generate_next_iteration();
                        for _cell in space.cells.clone().iter() {
                            let mut c = _cell.clone();
                            StateExplorer::apply_rule_if_applicable(&rule, &mut c, &space);
                            space.push_cell(&c);
                        }
    
                        if cfg!(debug_assertions) {
                            println!("Matched {} elements.", match_counter);
                        }
                        space.debug_print();
                        rule.debug_print();
                    }
    
                    if cfg!(debug_assertions) {
                        println!("\tDone exploring rule {}", rule_counter);
                    }
                    
                    if all_matched {
                        println!("All elements matched for rule");
                        rule.print();
                        self.emulate_rule_on_user_input(&rule, dim_len);
                        return true;
                    }

                    if !rule.has_next_eval_permutation() {
                        break;
                    }
                    rule.generate_next_eval_permutation();
                }

                if !rule.has_next() {
                    break;
                }

                if rule_counter % 1000 == 0 {
                    println!("Explored {rule_counter} rules. The last one was: ");
                    rule.print();
                }
                rule.generate_next();
            }

            println!("Done exploring dimension {}, explored {} rules", dim_len, rule_counter);
        }

        println!("Found no rule with dimensions between {} and {} that can generate primes", self.min_dimensions, self.max_dimensions);
        
        false
    }

    fn emulate_rule_on_user_input(&self, rule: &Rule, dim_len: usize) {
        let mut stdin = io::stdin();
        let mut stdout = io::stdout();

        let mut cell = Cell::new(dim_len);
        cell.set();
        let mut space = Space::new(dim_len);
        space.set_rule(rule);
        space.push_cell(&cell);

        let mut iter_counter = 1;

        loop {
            print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
            println!("All elements matched for sequence {:?}", self.expected_num_set_cells);

            println!("Emulating rule ");
            rule.print();
            println!("Iteration: {}", iter_counter);

            space.generate_next_iteration();
            space.print();
            iter_counter += 1;
            write!(stdout, "Press any key for generating next iteration. Ctrl + C to exit").unwrap();
            stdout.flush().unwrap();
            // Read a single byte and discard
            let _ = stdin.read(&mut [0u8]).unwrap();

            for cell in space.cells.clone().iter() {
                let mut _cell = cell.clone();
                StateExplorer::apply_rule_if_applicable(rule, &mut _cell, &space);
                space.push_cell(&_cell);
            }
        }
    }

    pub fn apply_rule_if_applicable(rule: &Rule, cell: &mut Cell, space: &Space) {
        if StateExplorer::is_rule_applicable(rule, cell, space) {
            StateExplorer::apply_rule(rule, cell);
        }
    }

    fn apply_rule(rule: &Rule, cell: &mut Cell) {
        if rule.result == Action::Set {
            cell.set();
        }
        else if rule.result == Action::Unset {
            cell.unset();
        }
        else {
            cell.flip();
        }
    }

    fn is_rule_applicable(rule: &Rule, cell: &mut Cell, space: &Space) -> bool {
        // Always reset before applying rules
        cell.reset_explore();

        // truth values to be applied to condition
        let mut v: Vec<bool> = vec![];
        
        let cell_coordinates: Vec<i32> = cell.get_nearby_coordinate();
        let cell_in_space = space.search_cells(&cell_coordinates);
        if cell_in_space == None {
            // Cell unset
            v.push(false);
        }
        else {
            if cell_in_space.unwrap().get_value() == CellValue::Set {
                v.push(true);
            }
            else {
                v.push(false);
            }
        }

        while cell.has_unexplored_nearby_cell() {
            cell.generate_next_unexplored_nearby_cell();
            let cell_coordinates: Vec<i32> = cell.get_nearby_coordinate();
            
            let cell_in_space = space.search_cells(&cell_coordinates);
            if cell_in_space == None {
                // Cell unset
                v.push(false);
            }
            else {
                if cell_in_space.unwrap().get_value() == CellValue::Set {
                    v.push(true);
                }
                else {
                    v.push(false);
                }
            }
        }

        return rule.evaluate(&v);
    }
}

mod test {
    use super::*;


    #[test]
    fn test_is_rule_applicable() {
        let mut explorer: StateExplorer = StateExplorer::new(1, 1, vec![1,3,5,7,9,11,13,15]);
        assert_eq!(explorer.explore(), true);

        explorer = StateExplorer::new(1, 1, vec![1,2,3,4,5,6,7,8,9]);
        assert_eq!(explorer.explore(), true);

        explorer = StateExplorer::new(1, 1, vec![1,2,3,5,7,11]);
        assert_eq!(explorer.explore(), false);

        explorer = StateExplorer::new(2, 2, vec![1,2,3,4,5,6,7,8,9]);
        assert_eq!(explorer.explore(), true);

        // Warning: This takes a few seconds
        explorer = StateExplorer::new(2, 2, vec![1,3,5,7,9,11,13,15]);
        // assert_eq!(explorer.explore(), true);
    }
}