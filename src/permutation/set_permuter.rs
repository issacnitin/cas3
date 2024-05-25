
/*
* Set permuter
* Generates all possible permutation by 
* Replacing elements in one set with elements in another set
*/

use std::cmp::min;

// Replace element X with Y
#[derive(Clone, Debug, PartialEq)]
pub struct SetPermuter {
    data: Vec<Vec<usize>>,
    child: Option<Box<SetPermuter>>,

    x: (usize, usize),
    y: (usize, usize),
    stack_size: usize
}

impl SetPermuter {
    pub fn new(data: Vec<Vec<usize>>) -> Self {
        let mut muter = SetPermuter {
            data: data,
            child: None,
            x: (0, 0),
            y: (0, 0),
            stack_size: 0
        };

        muter
    }

    pub fn reset(&mut self, data: Vec<Vec<usize>>) {
        self.data = data;
        let mut v : Vec<Vec<usize>> = vec![];
        let mut i = 0;
        let mut j = 0;

        let mut counter = 0;
        while i < self.data.len() {
            v.push(vec![]);
            while j < self.data[i].len() {
                v[i].push(counter);
                j += 1;
                counter += 1;
            }
            i += 1;
            j = 0;
        }

        self.data = v;
        self.x = (0, 0);
        self.y = (0, 0);
        self.child = None;
    }

    // First permutation is no-replacement
    pub fn has_next(&self) -> bool {
        if self.data.len() <= 1 {
            return false;
        }

        if self.child != None {
            if self.child.as_ref().unwrap().has_next() {
                return true;
            }
        }

        self.y.0 < self.data.len() - 1 
        || self.y.1 < self.data[self.y.0].len() - 1 
        || self.x.0 < self.data.len() - 2
        || self.x.1 < self.data[self.x.0].len() - 1
        || self.child != None
    }


    // Optimizing for swaps
    // P: [1 2] [3 4 5]
    // P: [3 2] [1 4 5]
    // P: [4 2] [3 1 5]
    // P: [5 2] [3 4 1]
    // C1: [3 4] [1 2 5]
    // C1: [3 5] [1 4 2]
    // C1: [4 5] [3 1 2] 
    pub fn generate_next(&mut self) {
        // First permutation
        if self.x.0 == 0 && self.x.1 == 0 && self.y.0 == 0 && self.y.1 == 0 {
            self.y.0 = self.x.0 + 1;
            self.y.1  = 0;
            return;
        }

        if self.child != None {
            if self.child.as_ref().unwrap().has_next() {
                self.child.as_mut().unwrap().generate_next();
            }
            else {
                self.child = None;
            }
        }
        else {
            self.generate_child();

            // self is commited to x
            if self.y.1 < self.data[self.y.0].len() - 1 {
                self.y.1 += 1;
            }
            else if self.y.0 < self.data.len() - 1 {
                self.y.0 += 1;
                self.y.1 = 0;
            }
            else if self.x.1 < self.data[self.x.0].len() - 1 {
                self.x.1 += 1;
                self.y.0 = self.x.0 + 1;
                self.y.1 = 0;
            }
            else if self.x.0 < self.data.len() - 2 {
                self.x.0 += 1;
                self.x.1 = 0;
                self.y.0 = self.x.0 + 1;
                self.y.1 = 0;
            }
            else {
                panic!("Overflow");
            }

        }
    }

    fn replace(&mut self) {
        let t = self.data[self.x.0][self.x.1];
        self.data[self.x.0][self.x.1] = self.data[self.y.0][self.y.1];
        self.data[self.y.0][self.y.1] = t;
    }

    fn generate_child(&mut self) {
        let mut child_permuter = self.clone();
        child_permuter.replace(); 

        // self increases x by 1 for child
        if child_permuter.x.1 < child_permuter.data[child_permuter.x.0].len() - 1 {
            child_permuter.x.1 += 1;
            if child_permuter.y.1 < child_permuter.data[child_permuter.y.0].len() - 1  {
                child_permuter.y.1 += 1;    
            }
            else {
                if child_permuter.y.0 < child_permuter.data.len() - 1 {
                    child_permuter.y.0 += 1;
                    child_permuter.y.1 = 0;
                }
                else {
                    return;
                }
            }
        }
        else if child_permuter.x.0 < child_permuter.data.len() - 2 {
            child_permuter.x.0 += 1;
            child_permuter.x.1 = 0;
            child_permuter.y.0 = child_permuter.x.0 + 1;
            child_permuter.y.1 = 0;
        }
        else {
            return;
        }
        
        child_permuter.stack_size += 1;
        self.child = Some(Box::new(child_permuter));
    }

    pub fn get_vector(&self) -> Vec<usize> {
        if self.child != None {
            return self.child.as_ref().unwrap().get_vector();
        }

        
        let mut v = self.data.clone();
        let t = v[self.x.0][self.x.1];
        v[self.x.0][self.x.1] = v[self.y.0][self.y.1];
        v[self.y.0][self.y.1] = t;
        

        let mut result: Vec<usize> = vec![];
        for i in v {
            for j in i {
                result.push(j);
            }
        }

        result
    }
}

mod test {
    use super::*;


    #[test]
    fn test_generate_set_permutations() {
        let mut permuter = SetPermuter::new(vec![vec![0, 1], vec![2]]);
        assert_eq!(permuter.has_next(), true);
        //assert_eq!(permuter.get_vector(), [0, 1, 2, 3, 4]);
        println!("{:?}", permuter.get_vector());

        while permuter.has_next() {
            permuter.generate_next();
            println!("{:?}", permuter.get_vector());
        }
    }
}