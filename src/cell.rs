use std::hash::{DefaultHasher, Hash, Hasher};

#[derive(PartialEq, Clone, Debug)]
pub enum DeltaCoordinate {
    SameCoordinate,
    Positive,
    Negative
}


#[derive(Debug, Clone, PartialEq, Copy)]
pub enum CellValue {
    Set,
    Unset
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    coordinates: Vec<i32>,
    value: CellValue,
    delta_coordinate: Vec<DeltaCoordinate>
}

impl Cell {
    pub fn new(len: usize) -> Self {
        Cell {
            coordinates : vec![0; len],
            value: CellValue::Unset,
            delta_coordinate: vec![DeltaCoordinate::SameCoordinate; len]
        }
    }
    
    pub fn len(&self) -> usize {
        self.coordinates.len()
    }

    pub fn get_value(&self) -> CellValue {
        return self.value;
    }

    pub fn set_value(&mut self, v: CellValue) {
        self.value = v;
    }

    pub fn get_coordinates(&self) -> &Vec<i32> {
        &self.coordinates
    }

    pub fn set_coordinates(&mut self, coordinates: Vec<i32>) {
        self.coordinates = coordinates;
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

    pub fn get_nearby_coordinate(&self) -> Vec<i32> {
        
        let mut my_coordinates = self.coordinates.clone();

        let mut i = 0;
        for it in self.delta_coordinate.as_slice() {
            if *it == DeltaCoordinate::Positive {
                my_coordinates[i] += 1;
            }
            else if *it == DeltaCoordinate::Negative {
                my_coordinates[i] -= 1;
            }
            i += 1;
        }

        return my_coordinates;
    }

    pub fn has_unexplored_nearby_cell(&self) -> bool {
        for it in self.delta_coordinate.as_slice() {
            if (*it) != DeltaCoordinate::Negative {
                return true;
            }
        }

        return false;
    }

    // Goes from SameCoordinate -> Positive -> Negative
    pub fn generate_next_unexplored_nearby_cell(&mut self) {

        let mut it: usize = 0;
        let mut new_vec = self.delta_coordinate.clone();

        while it < new_vec.len() && new_vec[it] == DeltaCoordinate::Negative {
            new_vec[it] = DeltaCoordinate::SameCoordinate;
            it += 1;
        }

        if it >= self.delta_coordinate.len() {
            panic!("Overflow");
        }
        
        if new_vec[it] == DeltaCoordinate::SameCoordinate {
            new_vec[it] = DeltaCoordinate::Positive;
        }
        else if new_vec[it] == DeltaCoordinate::Positive {
            new_vec[it] = DeltaCoordinate::Negative;
        }
        
        self.delta_coordinate = new_vec;
    }

    pub fn reset_explore(&mut self) {
        self.delta_coordinate = vec![DeltaCoordinate::SameCoordinate; self.delta_coordinate.len()];
    }   
}  


#[cfg(test)]
mod tests {
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


    #[test]
    fn test_generate_cell_combinations() {
        let mut cell = Cell::new(1);

        assert_eq!(*cell.get_coordinates(), vec![0]);
        assert_eq!(cell.has_unexplored_nearby_cell(), true);
        cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*cell.get_coordinates(), vec![0]);
        assert_eq!(cell.get_nearby_coordinate(), vec![1]);
        assert_eq!(cell.has_unexplored_nearby_cell(), true);
        cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*cell.get_coordinates(), vec![0]);
        assert_eq!(cell.get_nearby_coordinate(), vec![-1]);
        assert_eq!(cell.has_unexplored_nearby_cell(), false);

        let mut two_d_cell = Cell::new(2);
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![1, 0]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![-1, 0]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![0, 1]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![1, 1]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![-1, 1]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![0, -1]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![1, -1]);
        assert_eq!(two_d_cell.has_unexplored_nearby_cell(), true);
        two_d_cell.generate_next_unexplored_nearby_cell();
        assert_eq!(*two_d_cell.get_coordinates(), vec![0, 0]);
        assert_eq!(two_d_cell.get_nearby_coordinate(), vec![-1, -1]);
    }

}