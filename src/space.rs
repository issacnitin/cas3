use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::cell::{Cell, CellValue};
use crate::rule::{Rule, RuleCoordinate, RuleResult};


#[derive(Debug, Clone)]
struct CellHash(usize, u64);

impl Hash for CellHash {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Only hash the first element of the tuple
        self.1.hash(state);
        // You could also hash other attributes or apply custom logic
    }
}

impl PartialEq for CellHash {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1
    }
}

impl Eq for CellHash {}

#[derive(Debug, Clone)]
pub struct Space {
    current_iteration: usize,
    cells: Vec<Cell>,
    // cell index, hash
    cell_hashes: HashSet<CellHash>,
    rule: Rule
}


impl Space {
    pub fn new(len: usize) -> Space {
        Space {
            current_iteration: 0,
            cells: vec![Cell::new(len); 0],
            rule: Rule::new(len),
            cell_hashes: HashSet::new()
        }
    }

    pub fn set_rule(&mut self, rule: Rule) {
        self.rule = rule;
    }

    pub fn get_rule(&self) -> &Rule {
        &self.rule
    }

    pub fn get_current_iteration(&self) -> usize {
        self.current_iteration
    }

    pub fn get_cells(&self) -> &Vec<Cell> {
        &self.cells
    }

    pub fn get_ith_cell_mut(&mut self, i: usize) -> &mut Cell {
        self.cells.get_mut(i).unwrap()
    }

    pub fn set_ith_cell(&mut self, i: usize, cell: &mut Cell) {
        self.cells[i] = cell.clone();
    }

    pub fn push_cell(&mut self, cell: &Cell) {
        let found_cell: Option<&mut Cell> = self.search_cells_mut(cell.get_coordinates());
        if found_cell == None {
            self.cells.push(cell.clone());
            self.cell_hashes.insert(CellHash(self.cells.len()-1, cell.get_hash()));
            self.gen_next_iteration();
        }
        else {
            // TODO
            found_cell.unwrap().copy(cell.clone());
            self.gen_next_iteration();
        }
    }

    pub fn pop_cell(&mut self) {
        let popped_cell = self.cells.pop();
        if popped_cell != None {
            self.cell_hashes.remove(&CellHash(self.cells.len(), popped_cell.unwrap().get_hash()));
        }
    }

    pub fn len(&self) -> usize {
        self.cells.len()
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

    fn get_vector_hash(&self, vec: &Vec<i32>) -> u64 {
        let mut hasher = DefaultHasher::new();
        vec.hash(&mut hasher);
        hasher.finish()
    }

    pub fn search_cells(&self, coordinates: &Vec<i32>) -> Option<&Cell> {

        let hash = self.get_vector_hash(&coordinates);
        // Hash based on vector alone, as implemented above
        if !self.cell_hashes.contains(&CellHash(0, hash)) {
            return None;
        }
        
        return self.cells.get(
            self.cell_hashes.get(&CellHash(0, hash)).unwrap().0
        );
    }

    pub fn search_cells_mut(&mut self, coordinates: &Vec<i32>) -> Option<&mut Cell> {
        let hash = self.get_vector_hash(&coordinates);
        if !self.cell_hashes.contains(&CellHash(0, hash)) {
            return None;
        }

        return self.cells.get_mut(
            self.cell_hashes.get(&CellHash(0, hash)).unwrap().0
        );
    }

    pub fn gen_next_iteration(&mut self) {
        let mut it = 0;

        // Snapshot length
        let length = self.cells.len();
        
        while it < length {
            let cell: &Cell = &self.cells[it];

            // Only if the cell is SET, do we generate neighbours
            // Because if the cell is not set and generating neighbours, 
            // neighbours can't be set unless the RULE is to SET if all surrounding
            // cells are unset, which is naive
            if cell.get_value() == CellValue::Set {
                let mut r = &mut self.generate_surrounding_cells(cell);

                for it in r.iter_mut() {
                    self.cells.push(it.clone());
                    self.cell_hashes.insert(CellHash(self.cells.len() - 1, it.get_hash()));
                }
            }
            it += 1;
        }
    }

    fn generate_surrounding_cells(&self, cell: &Cell) -> Vec<Cell> {
        let mut r: Rule = Rule::new(cell.len()); // Fake rule so we can use RuleCoordiantes

        let mut result: Vec<Cell> = vec![];

        while r.has_next_rule() {
            r = r.gen_next_rule();

            // Only consider one rule for idempotency (avoid duplicates)
            if r.get_result() != RuleResult::Unset {
                continue;
            }
            
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
                result.push(new_cell);
            }
        }

        return result;
    }

}

mod test {
    use super::*;
    
    #[test]
    fn test_push_cell_2d() {
        let mut space: Space = Space::new(2);
        let mut cell: Cell = Cell::new(2);
        cell.set();

        space.push_cell(&cell);

        assert_eq!(space.cells.len(), 9);
        assert_eq!(space.find_number_of_cells(CellValue::Unset), 8);
        
        space.push_cell(&cell);
        assert_eq!(space.find_number_of_cells(CellValue::Unset), 8);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 1);

        cell = space.search_cells(&vec![1,1]).unwrap().clone();
        cell.set();

        assert_eq!(space.find_number_of_cells(CellValue::Unset), 8);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 1);
        space.push_cell(&cell);


        assert_eq!(space.find_number_of_cells(CellValue::Unset), 12);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 2);
    }

    #[test]
    fn test_generate_cell_2d_case1() {
        let mut space: Space = Space::new(2);

        let mut cell: Cell = Cell::new(2);
        cell.set();
        space.push_cell(&cell);

        assert_ne!(space.search_cells(&vec![0,0]), None);
        assert_ne!(space.search_cells(&vec![0,1]), None);
        assert_ne!(space.search_cells(&vec![1,0]), None);
        assert_ne!(space.search_cells(&vec![1,1]), None);
        assert_ne!(space.search_cells(&vec![-1,0]), None);
        assert_ne!(space.search_cells(&vec![0,-1]), None);
        assert_ne!(space.search_cells(&vec![-1,-1]), None);
        assert_ne!(space.search_cells(&vec![1,-1]), None);
        assert_ne!(space.search_cells(&vec![-1,1]), None);

        assert_eq!(space.search_cells(&vec![2,0]), None);

        assert_eq!(space.cells.len(), 9);
    }

    #[test]
    fn test_generate_cell_2d_case2() {
        let mut space: Space = Space::new(2);

        let mut cell: Cell = Cell::new(2);
        cell.set_ith_coordinate(0, 3);
        cell.set_ith_coordinate(1, 3);
        cell.set();
        space.push_cell(&cell);

        assert_ne!(space.search_cells(&vec![3,3]), None);
        assert_ne!(space.search_cells(&vec![3,4]), None);
        assert_ne!(space.search_cells(&vec![4,3]), None);
        assert_ne!(space.search_cells(&vec![4,4]), None);
        assert_ne!(space.search_cells(&vec![2,3]), None);
        assert_ne!(space.search_cells(&vec![3,2]), None);
        assert_ne!(space.search_cells(&vec![2,2]), None);
        assert_ne!(space.search_cells(&vec![3,2]), None);
        assert_ne!(space.search_cells(&vec![2,3]), None);

        assert_eq!(space.search_cells(&vec![0,0]), None);

        assert_eq!(space.cells.len(), 9);
    }

    #[test]
    fn test_generate_cell_3d() {
        let mut space: Space = Space::new(3);

        let mut cell: Cell = Cell::new(3);
        cell.set();
        space.push_cell(&cell);

        assert_ne!(space.search_cells(&vec![0,0,0]), None);
        assert_ne!(space.search_cells(&vec![0,0,1]), None);
        assert_ne!(space.search_cells(&vec![0,1,0]), None);
        assert_ne!(space.search_cells(&vec![1,0,0]), None);
        assert_ne!(space.search_cells(&vec![0,0,-1]), None);
        assert_ne!(space.search_cells(&vec![0,-1,0]), None);
        assert_ne!(space.search_cells(&vec![-1,0,0]), None);

        assert_eq!(space.search_cells(&vec![1,2,1]), None);

        assert_eq!(space.cells.len(), 27);
    }

    #[test]
    fn test_search_cell_exist() {
        let mut space: Space = Space::new(2);

        let mut cell1 : Cell = Cell::new(2);
        cell1.set_ith_coordinate(0, 1);

        space.push_cell(&cell1);

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push_cell(&cell2);

        let found_cell = space.search_cells(&vec![1, 0]);
        assert_eq!(*found_cell.unwrap(), cell1);


        let found_cell2 = space.search_cells(&vec![0, 2]);
        assert_eq!(*found_cell2.unwrap(), cell2);
    }


    #[test]
    fn test_search_cell_not_exist() {
        let mut space: Space = Space::new(2);

        let mut cell1 : Cell = Cell::new(2);
        cell1.set_ith_coordinate(0, 1);

        space.push_cell(&cell1);

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_ith_coordinate(1, 2);

        space.push_cell(&cell2);

        let found_cell = space.search_cells(&vec![2, 3]);
        assert_eq!(found_cell, None);
    }

    
}