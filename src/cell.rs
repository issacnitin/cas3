
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    dimension_values: Vec<i32>,
    value: bool
}

impl Cell {
    pub fn new(len: usize) -> Self {
        Cell {
            dimension_values : vec![0; len],
            value: false
        }
    }
    
    pub fn len(&self) -> usize {
        self.dimension_values.len()
    }

    pub fn get_value(&self) -> bool {
        return self.value;
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
        self.value = true;
    }

    pub fn unset(&mut self) {
        self.value = false;
    }

    pub fn flip(&mut self) {
        self.value = !self.value;
    }
}  


#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_new() {
        let cell : Cell = Cell::new(10);

        assert_eq!(cell.value, false);
        assert_eq!(cell.dimension_values.len(), 10);
        
        for it in cell.dimension_values.iter() {
            assert_eq!(*it, 0);
        }
    }

    #[test]
    fn test_set_unset_flip() {
        let mut cell: Cell = Cell::new(10);

        cell.set();
        assert_eq!(cell.get_value(), true);
        
        cell.set();
        assert_eq!(cell.get_value(), true);

        cell.unset();
        assert_eq!(cell.get_value(), false);

        cell.unset();
        assert_eq!(cell.get_value(), false);

        cell.flip();
        assert_eq!(cell.get_value(), true);
    }

}