use std::io;
use std::io::prelude::*;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;

use crate::cell::CellValue;
use crate::cell::Cell;
use crate::rule::Action;
use crate::rule::Rule;
use crate::rule_set::RuleSet;
use crate::graph::op_node::OpNode;
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
            let num_neighbors = (3_usize).pow(dim_len as u32);

            // Pre-collect all rule variants (expression + action combos)
            let mut all_rules: Vec<Rule> = vec![];
            let mut rule = Rule::new(dim_len);
            all_rules.push(rule.clone());
            while rule.has_next() {
                rule.generate_next();
                all_rules.push(rule.clone());
            }

            let total_rules = all_rules.len();
            println!("Dimension {}: searching {} rule expressions across all threads...", dim_len, total_rules);

            let counter = AtomicU64::new(0);
            let found_rule: Mutex<Option<Rule>> = Mutex::new(None);
            let found_flag = AtomicBool::new(false);

            // Parallel search: each thread takes a rule expression and iterates its permutations
            all_rules.par_iter().for_each(|base_rule| {
                if found_flag.load(Ordering::Relaxed) {
                    return;
                }

                let mut rule = base_rule.clone();
                let mut truth_buf: Vec<bool> = Vec::with_capacity(num_neighbors);
                let mut coord_buf: Vec<i32> = Vec::with_capacity(dim_len);

                loop {
                    if found_flag.load(Ordering::Relaxed) {
                        return;
                    }

                    let cnt = counter.fetch_add(1, Ordering::Relaxed) + 1;

                    let mut space: Space = Space::new(dim_len);
                    space.set_rule(&rule);

                    let mut init_cell: Cell = Cell::new(dim_len);
                    init_cell.set();
                    space.push_cell(&init_cell);

                    let mut all_matched: bool = true;
                    for &el in self.expected_num_set_cells.iter() {
                        if space.get_set_count() != el {
                            all_matched = false;
                            break;
                        }
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
                    }

                    if all_matched {
                        found_flag.store(true, Ordering::Relaxed);
                        let mut guard = found_rule.lock().unwrap();
                        *guard = Some(rule.clone());
                        return;
                    }

                    if !cfg!(debug_assertions) && cnt % 100000 == 0 {
                        println!("Dimension {}: explored {} permutations so far", dim_len, cnt);
                    }

                    if !rule.has_next_eval_permutation() {
                        break;
                    }
                    rule.generate_next_eval_permutation();
                }
            });

            let result = found_rule.lock().unwrap().take();
            if let Some(matched_rule) = result {
                println!("All elements matched for rule");
                matched_rule.print();
                if !cfg!(test) {
                    self.emulate_rule_on_user_input(&matched_rule, dim_len);
                }
                return true;
            }

            let total_checked = counter.load(Ordering::Relaxed);
            println!("Done exploring dimension {}, explored {} rules and {} permutations", dim_len, total_rules, total_checked);
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
        Self::build_truth_values(cell, space, v, coord_buf);
        rule.evaluate(v)
    }

    fn build_truth_values(cell: &mut Cell, space: &Space, v: &mut Vec<bool>, coord_buf: &mut Vec<i32>) {
        cell.reset_explore();
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
    }

    fn apply_rule_set(rule_set: &RuleSet, cell: &mut Cell, space: &Space, truth_buf: &mut Vec<bool>, coord_buf: &mut Vec<i32>) {
        Self::build_truth_values(cell, space, truth_buf, coord_buf);

        match cell.get_value() {
            CellValue::Unset => {
                if rule_set.evaluate_birth(truth_buf) {
                    cell.set();
                }
            }
            CellValue::Set => {
                if !rule_set.evaluate_survival(truth_buf) {
                    cell.unset();
                }
            }
        }
    }

    pub fn explore_rule_sets(&self) -> bool {
        for dim_len in self.min_dimensions..=self.max_dimensions {
            let num_neighbors = (3_usize).pow(dim_len as u32);
            let end_index = num_neighbors - 1;

            // Pre-collect all distinct expression variants (shared by birth & survival)
            let mut all_exprs: Vec<OpNode> = vec![];
            let mut expr = OpNode::new(0, end_index);
            all_exprs.push(expr.clone());
            while expr.has_next() {
                expr.generate_next();
                all_exprs.push(expr.clone());
            }

            let num_exprs = all_exprs.len();
            // Build cartesian product indices: (birth_expr_idx, survival_expr_idx)
            let pairs: Vec<(usize, usize)> = (0..num_exprs)
                .flat_map(|b| (0..num_exprs).map(move |s| (b, s)))
                .collect();

            let total_pairs = pairs.len();
            println!("Dimension {}: searching {} expression pairs ({} x {}) across all threads...",
                dim_len, total_pairs, num_exprs, num_exprs);

            let counter = AtomicU64::new(0);
            let found_flag = AtomicBool::new(false);
            let found_rule_set: Mutex<Option<RuleSet>> = Mutex::new(None);

            // Parallel search over (birth_expr, survival_expr) pairs
            pairs.par_iter().for_each(|&(b_idx, s_idx)| {
                if found_flag.load(Ordering::Relaxed) {
                    return;
                }

                let mut rule_set = RuleSet {
                    birth_condition: all_exprs[b_idx].clone(),
                    survival_condition: all_exprs[s_idx].clone(),
                };

                let mut truth_buf: Vec<bool> = Vec::with_capacity(num_neighbors);
                let mut coord_buf: Vec<i32> = Vec::with_capacity(dim_len);

                // Iterate all birth permutations
                loop {
                    if found_flag.load(Ordering::Relaxed) {
                        return;
                    }
                    // Iterate all survival permutations
                    loop {
                        if found_flag.load(Ordering::Relaxed) {
                            return;
                        }

                        let cnt = counter.fetch_add(1, Ordering::Relaxed) + 1;

                        // Simulate the cellular automaton
                        let mut space = Space::new(dim_len);
                        let mut init_cell = Cell::new(dim_len);
                        init_cell.set();
                        space.push_cell(&init_cell);

                        let mut all_matched = true;
                        for &el in self.expected_num_set_cells.iter() {
                            if space.get_set_count() != el {
                                all_matched = false;
                                break;
                            }
                            space.generate_next_iteration();
                            let mut modified_cells: Vec<Cell> = Vec::with_capacity(space.len());
                            for existing_cell in space.cells.iter() {
                                let mut c = existing_cell.clone();
                                Self::apply_rule_set(&rule_set, &mut c, &space, &mut truth_buf, &mut coord_buf);
                                modified_cells.push(c);
                            }
                            for c in modified_cells.iter() {
                                space.push_cell(c);
                            }
                        }

                        if all_matched {
                            found_flag.store(true, Ordering::Relaxed);
                            let mut guard = found_rule_set.lock().unwrap();
                            *guard = Some(rule_set.clone());
                            return;
                        }

                        if !cfg!(debug_assertions) && cnt % 500000 == 0 {
                            println!("Dimension {}: explored {} rule set combinations", dim_len, cnt);
                        }

                        // Advance survival permutation
                        if !rule_set.survival_condition.has_next_eval_permutation() {
                            break;
                        }
                        rule_set.survival_condition.generate_next_eval_permutation();
                    }
                    // Reset survival permutation for next birth permutation
                    rule_set.survival_condition.reset_permuter();

                    // Advance birth permutation
                    if !rule_set.birth_condition.has_next_eval_permutation() {
                        break;
                    }
                    rule_set.birth_condition.generate_next_eval_permutation();
                }
            });

            let result = found_rule_set.lock().unwrap().take();
            if let Some(matched_rule_set) = result {
                println!("Found matching rule set!");
                matched_rule_set.print();
                if !cfg!(test) {
                    self.emulate_rule_set_on_user_input(&matched_rule_set, dim_len);
                }
                return true;
            }

            let total_checked = counter.load(Ordering::Relaxed);
            println!("Done exploring dimension {} with rule sets: {} combinations checked", dim_len, total_checked);
        }

        println!("Found no rule set with dimensions between {} and {} that can generate sequence {:?}",
            self.min_dimensions, self.max_dimensions, self.expected_num_set_cells);
        false
    }

    fn emulate_rule_set_on_user_input(&self, rule_set: &RuleSet, dim_len: usize) {
        let mut stdin = io::stdin();
        let mut stdout = io::stdout();

        let mut truth_buf: Vec<bool> = Vec::with_capacity((3_usize).pow(dim_len as u32));
        let mut coord_buf: Vec<i32> = Vec::with_capacity(dim_len);

        let mut cell = Cell::new(dim_len);
        cell.set();
        let mut space = Space::new(dim_len);
        space.push_cell(&cell);

        let mut iter_counter = 1;

        loop {
            print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
            println!("All elements matched for sequence {:?}", self.expected_num_set_cells);
            println!("Emulating rule set:");
            rule_set.print();
            println!("Iteration: {}", iter_counter);

            space.generate_next_iteration();
            space.print();
            iter_counter += 1;
            write!(stdout, "Press any key for generating next iteration. Ctrl + C to exit").unwrap();
            stdout.flush().unwrap();
            let _ = stdin.read(&mut [0u8]).unwrap();

            let mut modified_cells: Vec<Cell> = vec![];
            for c in space.cells.iter() {
                let mut _cell = c.clone();
                Self::apply_rule_set(rule_set, &mut _cell, &space, &mut truth_buf, &mut coord_buf);
                modified_cells.push(_cell);
            }
            for c in modified_cells.iter() {
                space.push_cell(c);
            }
        }
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

    #[test]
    fn test_rule_set_simple_sequences() {
        // Birth/survival model should find monotonic sequences too
        let explorer = StateExplorer::new(1, 1, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(explorer.explore_rule_sets(), true);

        let explorer = StateExplorer::new(1, 1, vec![1, 3, 5, 7, 9, 11, 13, 15]);
        assert_eq!(explorer.explore_rule_sets(), true);
    }
}