use std::collections::HashSet;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::cell::{Cell, CellValue};
use crate::rule::Rule;


// Index, Hash
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
    pub cells: Vec<Cell>,
    // cell index, hash
    cell_hashes: HashSet<CellHash>,
    rule: Rule
}


impl Space {
    pub fn new(dim_len: usize) -> Space {
        Space {
            current_iteration: 0,
            cells: vec![Cell::new(dim_len); 0],
            rule: Rule::new(dim_len),
            cell_hashes: HashSet::new()
        }
    }

    pub fn debug_print(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        
        println!("Space has {} elements.", self.cells.len());

        for cell in self.cells.as_slice() {
            println!("\t\t Coordinates: {:?}, Value: {:?}", cell.get_coordinates(), cell.get_value());
        }
    }

    pub fn set_rule(&mut self, rule: &Rule) {
        self.rule = rule.clone();
    }

    pub fn get_rule(&self) -> &Rule {
        &self.rule
    }

    pub fn push_cell(&mut self, cell: &Cell) {
        let found_cell: Option<&mut Cell> = self.search_cells_mut(cell.get_coordinates());
        if found_cell == None {
            self.cells.push(cell.clone());
            self.cell_hashes.insert(CellHash(self.cells.len()-1, cell.get_hash()));;
        }
        else {
            found_cell.unwrap().set_value(cell.get_value());
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

    pub fn generate_next_iteration(&mut self) {
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
                let mut c_cell = cell.clone();
                while c_cell.has_unexplored_nearby_cell() {
                    c_cell.generate_next_unexplored_nearby_cell();
                    let c_coordinate = c_cell.get_nearby_coordinate().clone();

                    if self.search_cells(&c_coordinate) == None {
                        let mut new_cell = Cell::new(c_cell.len());
                        new_cell.set_coordinates(c_coordinate);
                        self.push_cell(&new_cell);
                    }
                }
            }
            it += 1;
        }
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
        assert_eq!(space.cells.len(), 1);
        
        space.generate_next_iteration();
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


        // assert_eq!(space.find_number_of_cells(CellValue::Unset), 12);
        assert_eq!(space.find_number_of_cells(CellValue::Set), 2);
    }

    #[test]
    fn test_generate_cell_1d() {
        let mut space: Space = Space::new(1);
        let mut cell: Cell = Cell::new(1);
        cell.set();
        space.push_cell(&cell);
        
        assert_eq!(space.len(), 1);
        space.generate_next_iteration();
        assert_eq!(space.len(), 3);

        space.cells[1].set();
        space.cells[2].set();
        space.generate_next_iteration();
        assert_eq!(space.len(), 5);

        space.cells[3].set();
        space.cells[4].set();
        space.generate_next_iteration();
        assert_eq!(space.len(), 7);
    }

    #[test]
    fn test_generate_cell_2d_case1() {
        let mut space: Space = Space::new(2);

        let mut cell: Cell = Cell::new(2);
        cell.set();
        space.push_cell(&cell);
        space.generate_next_iteration();

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
        cell.set_coordinates(vec![3,3]);
        cell.set();
        space.push_cell(&cell);
        space.generate_next_iteration();

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
        space.generate_next_iteration();

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
        cell1.set_coordinates(vec![1, 0]);

        space.push_cell(&cell1);

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_coordinates(vec![0,2]);

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
        cell1.set_coordinates(vec![1,0]);

        space.push_cell(&cell1);

        let mut cell2 : Cell = Cell::new(2);
        cell2.set_coordinates(vec![0,2]);

        space.push_cell(&cell2);

        let found_cell = space.search_cells(&vec![2, 3]);
        assert_eq!(found_cell, None);
    }

    
}