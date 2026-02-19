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
            let mut rule_counter: u64 = 0;
            let mut permutation_counter: u64 = 0;
            let num_neighbors = (3_usize).pow(dim_len as u32);
            let mut truth_buf: Vec<bool> = Vec::with_capacity(num_neighbors);
            let mut coord_buf: Vec<i32> = Vec::with_capacity(dim_len);
            // Explore all rules of given dimension
            loop {
                rule_counter += 1;
                // For all evaluation permutations 
                loop {
                    permutation_counter += 1;
                    let mut space: Space = Space::new(dim_len);
                    space.set_rule(&rule);
    
                    let mut __cell: Cell = Cell::new(dim_len);
                    __cell.set();
                    space.push_cell(&__cell);
    
                    let mut all_matched: bool = true;
                    let mut match_counter = 0;
                    for &el in self.expected_num_set_cells.iter() {
                        if cfg!(debug_assertions) {
                            print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
                            println!("Exploring dimension {}", dim_len);
                            println!("Explored {rule_counter} rules and {permutation_counter} permutations. The last one was: ");
                            rule.print();
                            rule.debug_print();
                            space.debug_print();
                        }
                        if space.get_set_count() != el {
                            all_matched = false;
                            break;
                        }
                        match_counter += 1;
                        space.generate_next_iteration();
                        let mut modified_cells: Vec<Cell> = Vec::with_capacity(space.len());
                        for _cell in space.cells.iter() {
                            let mut c = _cell.clone();
                            StateExplorer::apply_rule_if_applicable(&rule, &mut c, &space, &mut truth_buf, &mut coord_buf);
                            modified_cells.push(c);
                        }
                        for c in modified_cells.iter() {
                            space.push_cell(c);
                        }
    
                        if cfg!(debug_assertions) {
                            println!("Matched {} elements.", match_counter);
                        }
                    }
    
                    if cfg!(debug_assertions) {
                        println!("\tDone exploring rule {}", rule_counter);
                    }
                    
                    if all_matched {
                        println!("All elements matched for rule");
                        rule.print();

                        if !cfg!(test) {
                            self.emulate_rule_on_user_input(&rule, dim_len);
                        }
                        return true;
                    }

                    if !cfg!(debug_assertions) && permutation_counter % 100000 == 0 {
                        println!("Dimension {}: explored {} rules, {} permutations", dim_len, rule_counter, permutation_counter);
                    }

                    if !rule.has_next_eval_permutation() {
                        break;
                    }
                    rule.generate_next_eval_permutation();
                }

                if !rule.has_next() {
                    break;
                }
                rule.generate_next();
            }

            println!("Done exploring dimension {}, explored {} rules and {} permutations", dim_len, rule_counter, permutation_counter);
        }

        println!("Found no rule with dimensions between {} and {} that can generate sequence {:?}", self.min_dimensions, self.max_dimensions, self.expected_num_set_cells);
        
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

        let mut truth_buf: Vec<bool> = Vec::with_capacity((3_usize).pow(dim_len as u32));
        let mut coord_buf: Vec<i32> = Vec::with_capacity(dim_len);

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

            let mut modified_cells: Vec<Cell> = vec![];
            for cell in space.cells.iter() {
                let mut _cell = cell.clone();
                StateExplorer::apply_rule_if_applicable(rule, &mut _cell, &space, &mut truth_buf, &mut coord_buf);
                modified_cells.push(_cell);
            }
            for c in modified_cells.iter() {
                space.push_cell(c);
            }
        }
    }

    pub fn apply_rule_if_applicable(rule: &Rule, cell: &mut Cell, space: &Space, truth_buf: &mut Vec<bool>, coord_buf: &mut Vec<i32>) {
        if StateExplorer::is_rule_applicable(rule, cell, space, truth_buf, coord_buf) {
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

    fn is_rule_applicable(rule: &Rule, cell: &mut Cell, space: &Space, v: &mut Vec<bool>, coord_buf: &mut Vec<i32>) -> bool {
        // Always reset before applying rules
        cell.reset_explore();

        // truth values to be applied to condition
        v.clear();
        
        cell.get_nearby_coordinate_buf(coord_buf);
        let cell_in_space = space.search_cells(coord_buf);
        v.push(matches!(cell_in_space, Some(c) if c.get_value() == CellValue::Set));

        while cell.has_unexplored_nearby_cell() {
            cell.generate_next_unexplored_nearby_cell();
            cell.get_nearby_coordinate_buf(coord_buf);
            
            let cell_in_space = space.search_cells(coord_buf);
            v.push(matches!(cell_in_space, Some(c) if c.get_value() == CellValue::Set));
        }

        return rule.evaluate(v);
    }
}

#[cfg(test)]
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