
/*
* Set permuter
* Generates all possible permutation by 
* Replacing elements in one set with elements in another set
*/

// Replace element X with Y
#[derive(Clone, Debug, PartialEq)]
pub struct Permuter {
    data: Vec<Vec<usize>>,
    child: Option<Box<Permuter>>,

    x_i: usize,
    x_j: usize,

    y_i: usize,
    y_j: usize
}

impl Permuter {
    pub fn new(data: Vec<Vec<usize>>) -> Self {
        let mut muter = Permuter {
            data: data,
            child: None,
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
        self.child = None;
    }

    // First permutation is no-replacement
    pub fn has_next(&self) -> bool {
        if self.child != None && self.child.as_ref().unwrap().has_next() {
            return true;
        } else {
            // Either first or last permutation
            if self.y_i == self.x_i {
                if self.x_i + 1 < self.data.len() {
                    // A valid index, first permutation
                    return true;
                }
                else {
                    return false;
                }
            }
            else {
                if self.y_j < self.data[self.y_i].len() - 1 {
                    return true;
                }
                else if self.y_i < self.data.len() - 1 {
                    return true;
                }
                else if self.x_j < self.data[self.x_i].len() - 1 {
                    return true;
                }
                else if self.x_i + 1 < self.data.len() - 2 {
                    return true;
                }
                else {
                    return false;
                }
            }
        }
    }

    fn generate_child(&mut self) {
        let mut _child = self.clone();
        _child.replace();

        if _child.x_j < _child.data[_child.x_i].len() - 1 {
            _child.x_j += 1;
            _child.y_i = _child.x_i + 1;
            _child.y_j = 0;
        }
        else {
            if _child.x_i < _child.data.len() - 2 {
                _child.x_i += 1;
                _child.x_j = 0;
                _child.y_i = _child.x_i + 1;
                _child.y_j = 0;
            }
            else {
                // Terminal child
                _child.x_i = _child.data.len() - 2;
                _child.x_j = _child.data[_child.x_i].len() - 1;
                _child.y_i = _child.data.len() - 1;
                _child.y_j = _child.data[_child.y_i].len() - 1;
            }
        }

        self.child = Some(Box::new(_child.clone()));
    }

    pub fn generate_next(&mut self) {

        if self.child == None {
            // Either first or last permutation
            if self.y_i == self.x_i {
                if self.x_i < self.data.len() - 1 {
                    // A valid index, first permutation
                    self.y_i = self.x_i + 1;
                }
                else {
                    panic!("Overflow while generating permutations");
                }
            }
            else {
                if self.y_j < self.data[self.y_i].len() - 1 {
                    self.y_j += 1;
                } else if self.y_i < self.data.len() - 1 {
                    self.y_i += 1;
                    self.y_j = 0;
                }
                else if self.x_j < self.data[self.x_i].len() - 1 {
                    self.x_j += 1;
                    self.y_i = self.x_i + 1;
                    self.y_j = 0;
                }
                else if self.x_i + 1 < self.data.len() - 2 {
                    self.x_i += 1;
                    self.x_j = 0;
                    self.y_i = self.x_i + 1;
                    self.y_j = 0;
                }
                else {
                    panic!("Overflow while generating permutations");
                }
            }
            self.generate_child();
        }
        else {
            if self.child.as_ref().unwrap().has_next() {
                self.child.as_mut().unwrap().generate_next();
            }
            else {
                self.child = None;
                if self.has_next() {
                    self.generate_next();
                }
            }
        }
    }

    fn replace(&mut self) {

        let temp = self.data[self.x_i][self.x_j];
        self.data[self.x_i][self.x_j] = self.data[self.y_i][self.y_j];
        self.data[self.y_i][self.y_j] = temp;
    }

    pub fn get_sequence(&self) -> Vec<usize> {
        if self.child != None {
            return self.child.as_ref().unwrap().get_sequence();
        }

        let mut v : Vec<usize> = vec![];
        for ii in self.data.as_slice() {
            for jj in ii.as_slice() {
                v.push(*jj);
            }
        }

        return v;
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
        assert_eq!(permuter.get_sequence(), vec![0, 1, 2]);
        assert_eq!(permuter.has_next(), false);
    }

    #[test]
    fn test_permuter_three_literals_two_set() {
        let mut permuter = Permuter::new(vec![vec![0,1], vec![2]]);
        assert_eq!(permuter.get_sequence(), vec![0, 1, 2]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![2, 1, 0]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![0, 2, 1]);

        assert_eq!(permuter.has_next(), false);

        permuter = Permuter::new(vec![vec![0], vec![1, 2]]);
        assert_eq!(permuter.get_sequence(), vec![0, 1, 2]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![1, 0, 2]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![2, 1, 0]);

        assert_eq!(permuter.has_next(), false);
    }


    #[test]
    fn test_permuter_four_literals() {
        let mut permuter = Permuter::new(vec![vec![0,1], vec![2,3]]);
        assert_eq!(permuter.get_sequence(), vec![0, 1, 2, 3]);

        /*
        println!("{:?}", permuter.get_sequence());
        while permuter.has_next() {
            permuter.generate_next();
            println!("{:?}", permuter.get_sequence());
        }
         */

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![2, 1, 0, 3]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![2, 3, 0, 1]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![3, 1, 2, 0]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![3, 0, 2, 1]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![0, 2, 1, 3]);

        assert_eq!(permuter.has_next(), true);
        permuter.generate_next();
        assert_eq!(permuter.get_sequence(), vec![0, 3, 2, 1]);

        assert_eq!(permuter.has_next(), false);
    }
}