
/*
* Set permuter
* Generates all possible permutation by 
* Replacing elements in one set with elements in another set
*/

// Replace element X with Y
#[derive(Clone, Debug, PartialEq)]
pub struct Permuter {
    data: Vec<Vec<usize>>,
    x_i: usize,
    x_j: usize,

    y_i: usize,
    y_j: usize
}

impl Permuter {
    pub fn new(data: Vec<Vec<usize>>) -> Self {
        let mut muter = Permuter {
            data: data,
            x_i: 0,
            x_j: 0,
            y_i: 0,
            y_j: 0
        };

        muter
    }

    pub fn reset(&mut self, data: Vec<Vec<usize>>) {
        self.data = data;
        self.x_i = 0;
        self.x_j = 0;
        self.y_i = 0;
        self.y_j = 0;
    }

    pub fn has_next(&self) -> bool {
        if self.data.len() <= 1 {
            return false;
        }

        // y_j can be incremented
        if self.y_j < self.data[self.y_i].len() - 1 {
            return true;
        }

        // y_i can be incremented
        if self.y_i < self.data.len() - 1 {
            return true;
        }

        // x_j can be incremented
        if self.x_j < self.data[self.x_i].len() - 1 {
            return true;
        }

        // x_i can be incremented
        // x_i should go only until second last
        if self.x_i < self.data.len() - 2 {
            return true;
        }
        
        return false;
    }

    // First permutation is no-replacement
    pub fn generate_next(&mut self) {
        if self.x_i == self.y_i {
            self.y_i += 1;
            self.y_j = 0;
            return;
        }

        if self.y_j < self.data[self.y_i].len() - 1 {
            self.y_j += 1;
            return;
        }
        else if self.y_i < self.data.len() - 1 {
            self.y_i += 1;
            self.y_j = 0;
        }
        else if self.x_j < self.data[self.x_i].len() - 1 {
            self.x_j += 1;

            self.y_i = self.x_i + 1;
            self.y_j = 0;
        }
        else if self.x_i < self.data.len() - 1 {
            self.x_i += 1;
            self.x_j = 0;

            self.y_i = self.x_i + 1;
            self.y_j = 0;
        }
    }

    pub fn get_replaced_indices(&self) -> Vec<usize> {
        let mut v = self.data.clone();
        let temp = v[self.x_i][self.x_j];
        v[self.x_i][self.x_j] = v[self.y_i][self.y_j];
        v[self.y_i][self.y_j] = temp;


        // construct to an array
        let mut result: Vec<usize> = vec![];

        for ii in v {
            for jj in ii {
                result.push(jj);
            }
        }

        return result;
    }
}

mod test {
    use super::*;


    #[test]
    fn test_permuter_empty() {
        let mut permuter = Permuter::new(vec![]);
        assert_eq!(permuter.has_next(), false);
    }

    #[test]
    fn test_permuter_three_literals_one_set() {
        let mut permuter = Permuter::new(vec![vec![0, 1, 2]]);
        assert_eq!(permuter.get_replaced_indices(), vec![0, 1, 2]);
        assert_eq!(permuter.has_next(), false);
    }

    #[test]
    fn test_permuter_three_literals_two_set() {
        let mut permuter = Permuter::new(vec![vec![0,1], vec![2]]);
        assert_eq!(permuter.get_replaced_indices(), vec![0, 1, 2]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![2, 1, 0]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![0, 2, 1]);

        assert_eq!(permuter.has_next(), false);
    }


    #[test]
    fn test_permuter_four_literals() {
        let mut permuter = Permuter::new(vec![vec![0,1], vec![2, 3]]);
        assert_eq!(permuter.get_replaced_indices(), vec![0, 1, 2, 3]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![2, 1, 0, 3]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![3, 1, 2, 0]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![0, 2, 1, 3]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_replaced_indices(), vec![0, 3, 2, 1]);

        assert_eq!(permuter.has_next(), false);
    }
}