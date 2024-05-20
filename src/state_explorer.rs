use crate::cell::CellValue;
use crate::cell::Cell;
use crate::space::Space;
use crate::rule::Rule;

pub struct StateExplorer {
    max_dimensions: usize,
    expected_num_set_cells: Vec<usize>
}

impl StateExplorer {
    pub fn new(dims: usize) -> Self {
        StateExplorer {
            max_dimensions : dims,
            // Hard-coding to prime numbers for now
            expected_num_set_cells: vec![1,2,3,5,7,11,13,17,19,23,29]
        }
    }

    pub fn explore(&self) {
        // Explore all dimensions
        for dims in 0..self.max_dimensions {
            let mut r = Rule::new(dims);

            // For all possible rules
            while r.has_next_rule() {
                r = r.gen_next_rule();
                let mut space: Space = Space::new(dims);
                space.set_rule(r.clone());

                // Constructing first cell
                let mut first_cell: Cell = Cell::new(dims);
                first_cell.set();
                space.push_cell(first_cell);
                
                // Iterations
                let mut it = 1;
                // Condition must always hold, remember prime number generation in order
                while it < self.expected_num_set_cells.len() && space.find_number_of_cells(CellValue::Set) == *self.expected_num_set_cells.get(it).unwrap() 
                {
                    space.apply_rules(it);
                }
    
                if it == self.expected_num_set_cells.len() {
                    println!("Found rule generating primes: {:?}", space.get_rule());
                }
            }
        }
    }
}