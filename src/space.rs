use std::collections::btree_map::Range;
use std::iter;

use crate::cell::{Cell, CellValue};
use crate::rule::{Rule, RuleCoordinate};

#[derive(Debug)]
#[derive(Clone)]
pub struct Space {
    current_iteration: usize,
    cells: Vec<Cell>,
    rule: Rule
}

impl Space {
    pub fn new(len: usize) -> Space {
        Space {
            current_iteration: 0,
            cells: vec![Cell::new(len); 0],
            rule: Rule::new(len)
        }
    }

    pub fn set_rule(&mut self, rule: Rule) {
        self.rule = rule;
    }

    pub fn get_rule(&self) {
        self.rule.clone();
    }

    pub fn push_cell(&mut self, cell: Cell) {
        let found_cell: Option<&mut Cell> = self.search_cells_mut(cell.get_coordinates());
        if found_cell == None {
            self.cells.push(cell.clone());
            self.generate_surrounding_cells(cell);
        }
        else {
            // TODO
            found_cell.unwrap().copy(cell.clone());
            if cell.get_value() == CellValue::Set {
                self.generate_surrounding_cells(cell);
            }
        }
    }

    pub fn pop_cell(&mut self) {
        self.cells.pop();
    }

    pub fn find_number_of_cells(&self, expectedValue: CellValue) -> usize {
        let mut count = 0;
        for cell in self.cells.to_vec() {
            if cell.get_value() == expectedValue {
                count += 1;
            }
        }
        count
    }

    pub fn apply_rules(&mut self, intented_iteration: usize) {
        if intented_iteration < self.current_iteration {
            println!("Intented iteration < self.it");
            return;
        }

        let mut new_space : Vec<Cell> = vec![];

        for cell_iterator in self.cells.clone() {
            let new_cell: Cell = self.rule.apply_rule(&cell_iterator, self);
            new_space.push(new_cell);
        }

        self.cells = new_space;

        for cell_iterator in self.cells.clone(){
            if cell_iterator.get_value() == CellValue::Set {
                self.generate_surrounding_cells(cell_iterator);
            }
        }
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

    pub fn search_cells_mut(&mut self, coordinates: Vec<i32>) -> Option<&mut Cell> {
        for cell in self.cells.iter_mut() {
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

    fn generate_surrounding_cells(&mut self, cell: Cell) {
        let mut r: Rule = Rule::new(cell.len()); // Fake rule so we can use RuleCoordiantes

        while r.has_next_rule() {
            r = r.gen_next_rule();
            let mut new_cell = Cell::new(cell.len());
            let mut i = 0;
            for it in r.get_elements() {
                let mut ith_coordinate = cell.get_ith_coordinate(i);
                ith_coordinate = match it {
                    RuleCoordinate::SameCoordinate => ith_coordinate,
                    RuleCoordinate::Positive => ith_coordinate + 1,
                    RuleCoordinate::Negative => ith_coordinate - 1
                };

                new_cell.set_ith_coordinate(i, ith_coordinate);
                i += 1;
            }

            if self.search_cells(new_cell.get_coordinates()) == None {
                self.cells.push(new_cell);
            }
        }
    }

}

mod test {
    use super::*;


    #[test]
    fn test_push_cell_2d() {
        let mut space: Space = Space::new(2);
        let mut cell: Cell = Cell::new(2);

        space.push_cell(cell.clone());

        assert_eq!(space.cells.len(), 9);
        assert_eq!(space.find_number_of_cells(CellValue::Unset), 9);

        cell.set();
        assert_eq!(space.find_number_of_cells(CellValue::Unset), 9);
        
        space.push_cell(cell);
        assert_eq!(space.find_number_of_cells(CellValue::Unset), 8);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 1);

        cell = space.search_cells(vec![1,1]).unwrap().clone();
        cell.set();

        assert_eq!(space.find_number_of_cells(CellValue::Unset), 8);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 1);
        space.push_cell(cell);


        assert_eq!(space.find_number_of_cells(CellValue::Unset), 12);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 2);
    }

    #[test]
    fn test_generate_cell_2d_case1() {
        let mut space: Space = Space::new(2);

        let cell: Cell = Cell::new(2);
        space.push_cell(cell);

        assert_ne!(space.search_cells(vec![0,0]), None);
        assert_ne!(space.search_cells(vec![0,1]), None);
        assert_ne!(space.search_cells(vec![1,0]), None);
        assert_ne!(space.search_cells(vec![1,1]), None);
        assert_ne!(space.search_cells(vec![-1,0]), None);
        assert_ne!(space.search_cells(vec![0,-1]), None);
        assert_ne!(space.search_cells(vec![-1,-1]), None);
        assert_ne!(space.search_cells(vec![1,-1]), None);
        assert_ne!(space.search_cells(vec![-1,1]), None);

        assert_eq!(space.search_cells(vec![2,0]), None);

        assert_eq!(space.cells.len(), 9);
    }

    #[test]
    fn test_generate_cell_2d_case2() {
        let mut space: Space = Space::new(2);

        let mut cell: Cell = Cell::new(2);
        cell.set_ith_coordinate(0, 3);
        cell.set_ith_coordinate(1, 3);
        space.push_cell(cell);

        assert_ne!(space.search_cells(vec![3,3]), None);
        assert_ne!(space.search_cells(vec![3,4]), None);
        assert_ne!(space.search_cells(vec![4,3]), None);
        assert_ne!(space.search_cells(vec![4,4]), None);
        assert_ne!(space.search_cells(vec![2,3]), None);
        assert_ne!(space.search_cells(vec![3,2]), None);
        assert_ne!(space.search_cells(vec![2,2]), None);
        assert_ne!(space.search_cells(vec![3,2]), None);
        assert_ne!(space.search_cells(vec![2,3]), None);

        assert_eq!(space.search_cells(vec![0,0]), None);

        assert_eq!(space.cells.len(), 9);
    }

    #[test]
    fn test_generate_cell_3d() {
        let mut space: Space = Space::new(3);

        let mut cell: Cell = Cell::new(3);
        space.push_cell(cell);

        assert_ne!(space.search_cells(vec![0,0,0]), None);
        assert_ne!(space.search_cells(vec![0,0,1]), None);
        assert_ne!(space.search_cells(vec![0,1,0]), None);
        assert_ne!(space.search_cells(vec![1,0,0]), None);
        assert_ne!(space.search_cells(vec![0,0,-1]), None);
        assert_ne!(space.search_cells(vec![0,-1,0]), None);
        assert_ne!(space.search_cells(vec![-1,0,0]), None);

        assert_eq!(space.search_cells(vec![1,2,1]), None);

        assert_eq!(space.cells.len(), 27);
    }

    #[test]
    fn test_search_cell_exist() {
        let mut space: Space = Space::new(2);

        let mut cell1 : Cell = Cell::new(2);
        cell1.set_ith_coordinate(0, 1);

        space.push_cell(cell1.clone());

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push_cell(cell2.clone());

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

        space.push_cell(cell1.clone());

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push_cell(cell2.clone());

        let found_cell = space.search_cells(vec![2, 3]);
        assert_eq!(found_cell, None);
    }

    
}