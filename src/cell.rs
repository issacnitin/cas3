
#[derive(Debug, Clone, PartialEq, Copy)]
pub enum CellValue {
    Set,
    Unset
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    dimension_values: Vec<i32>,
    value: CellValue
}

impl Cell {
    pub fn new(len: usize) -> Self {
        Cell {
            dimension_values : vec![0; len],
            value: CellValue::Unset
        }
    }
    
    pub fn len(&self) -> usize {
        self.dimension_values.len()
    }

    pub fn get_value(&self) -> CellValue {
        return self.value;
    }

    pub fn get_dimensions(&self) -> Vec<i32> {
        return self.dimension_values.clone();
    }

    pub fn get_ith_coordinate(&self, pos: usize) -> i32 {
        if pos >= self.dimension_values.len() {
            panic!("Array out of bounds!");
        }

        return self.dimension_values[pos];
    }

    pub fn set_ith_coordinate(&mut self, pos: usize, val: i32) {
        if pos >= self.dimension_values.len() {
            panic!("Array out of bounds!");
        }

        self.dimension_values[pos] = val;
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
}  


#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_new() {
        let cell : Cell = Cell::new(10);

        assert_eq!(cell.value, CellValue::Unset);
        assert_eq!(cell.dimension_values.len(), 10);
        
        for it in cell.dimension_values.iter() {
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