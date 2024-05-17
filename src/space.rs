use std::collections::btree_map::Range;
use std::iter;

use crate::cell::Cell;
use crate::rule::Rule;

#[derive(Debug)]
#[derive(Clone)]
pub struct Space {
    it: i32,
    cells: Vec<Cell>,
    rule: Rule
}

impl Space {
    pub fn new(len: usize) -> Space {
        Space {
            it: 0,
            cells: vec![Cell::new(len); 0],
            rule: Rule::new(len)
        }
    }

    pub fn push(&mut self, cell: Cell) {
        self.cells.push(cell);
    }

    pub fn pop(&mut self) {
        self.cells.pop();
    }

    pub fn apply_rules(&mut self, intented_iteration: i32) {
        if intented_iteration < self.it {
            println!("Intented iteration < self.it");
            return;
        }

        let mut new_space : Vec<Cell> = vec![];

        // Write tests that none of the existing values get changed
        for cell_iterator in self.cells.as_slice() {
            let new_cell: Cell = self.rule.apply_rule(&cell_iterator, self);
            new_space.push(new_cell);
        }

        self.cells = new_space;
    }

    pub fn search_cells(&self, coordinates: Vec<i32>) -> Option<&Cell> {
        for cell in self.cells.iter() {
            let mut all_equal = true;
            for idx in 0..cell.len() {
                if cell.get_ith_coordinate(idx) != *coordinates.get(idx).unwrap() {
                    all_equal = false;
                    break;
                }
            }

            if all_equal {
                return Some(cell);
            }
        }

        None
    }
}

mod test {
    use super::*;


    #[test]
    fn test_search_cell_exist() {
        let mut space: Space = Space::new(2);

        let mut cell1 : Cell = Cell::new(2);
        cell1.set_ith_coordinate(0, 1);

        space.push(cell1.clone());

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push(cell2.clone());

        let found_cell = space.search_cells(vec![1, 0]);
        assert_eq!(*found_cell.unwrap(), cell1);


        let found_cell2 = space.search_cells(vec![0, 2]);
        assert_eq!(*found_cell2.unwrap(), cell2);
    }


    #[test]
    fn test_search_cell_not_exist() {
        let mut space: Space = Space::new(2);

        let mut cell1 : Cell = Cell::new(2);
        cell1.set_ith_coordinate(0, 1);

        space.push(cell1.clone());

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push(cell2.clone());

        let found_cell = space.search_cells(vec![1, 1]);
        assert_eq!(found_cell, None);
    }

    
}