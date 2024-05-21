use std::hash::{DefaultHasher, Hash, Hasher};


#[derive(Debug, Clone, PartialEq, Copy)]
pub enum CellValue {
    Set,
    Unset
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    coordinates: Vec<i32>,
    value: CellValue
}

impl Cell {
    pub fn new(len: usize) -> Self {
        Cell {
            coordinates : vec![0; len],
            value: CellValue::Unset
        }
    }

    pub fn copy(&mut self, cell: Cell) {
        self.coordinates = cell.get_coordinates().clone();
        self.value = cell.get_value();
    }
    
    pub fn len(&self) -> usize {
        self.coordinates.len()
    }

    pub fn get_value(&self) -> CellValue {
        return self.value;
    }

    pub fn get_coordinates(&self) -> &Vec<i32> {
        &self.coordinates
    }

    pub fn get_ith_coordinate(&self, pos: usize) -> i32 {
        if pos >= self.coordinates.len() {
            panic!("Array out of bounds!");
        }

        return self.coordinates[pos];
    }

    pub fn set_ith_coordinate(&mut self, pos: usize, val: i32) {
        if pos >= self.coordinates.len() {
            panic!("Array out of bounds!");
        }

        self.coordinates[pos] = val;
    }

    pub fn set(&mut self) {
        self.value = CellValue::Set;
    }

    pub fn unset(&mut self) {
        self.value = CellValue::Unset;
    }

    pub fn flip(&mut self) {
        if self.value == CellValue::Set {
            self.value = CellValue::Unset;
        }
        else {
            self.value = CellValue::Set;
        }
    }

    pub fn get_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.coordinates.hash(&mut hasher);
        hasher.finish()
    }
}  


#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_new() {
        let cell : Cell = Cell::new(10);

        assert_eq!(cell.value, CellValue::Unset);
        assert_eq!(cell.coordinates.len(), 10);
        
        for it in cell.coordinates.iter() {
            assert_eq!(*it, 0);
        }
    }

    #[test]
    fn test_set_unset_flip() {
        let mut cell: Cell = Cell::new(10);

        cell.set();
        assert_eq!(cell.get_value(), CellValue::Set);
        
        cell.set();
        assert_eq!(cell.get_value(), CellValue::Set);

        cell.unset();
        assert_eq!(cell.get_value(), CellValue::Unset);

        cell.unset();
        assert_eq!(cell.get_value(), CellValue::Unset);

        cell.flip();
        assert_eq!(cell.get_value(), CellValue::Set);
    }

}