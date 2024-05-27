
/*
* Set permuter
* Generates all possible permutation by 
* Replacing elements in one set with elements in another set
*/

// Replace element X with Y
#[derive(Clone, Debug, PartialEq)]
pub struct SetPermuter {
    data: Vec<Vec<usize>>,
    stack: Vec<((usize, usize), (usize, usize))>,
}

impl SetPermuter {
    pub fn new(data: Vec<Vec<usize>>) -> Self {
        let mut muter = SetPermuter {
            data: data,
            stack: vec![((0,0), (0,0))],
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
        self.stack = vec![((0,0),(0,0))];
    }

    // First permutation is no-replacement
    pub fn has_next(&self) -> bool {
        if self.data.len() <= 1 {
            return false;
        }

        if self.stack.len() == 0 {
            return false;
        }

        let last_el = self.stack.last().unwrap();

        last_el.1.0 < self.data.len() - 1 
        || last_el.1.1 < self.data[last_el.1.0].len() - 1 
        || last_el.0.0 < self.data.len() - 2
        || last_el.0.1 < self.data[last_el.0.0].len() - 1
        || self.stack.len() > 1
    }

    pub fn generate_next(&mut self) {
        if self.stack.len() == 0 {
            panic!("Overflow");
        }


        let mut last_el = self.stack.last().unwrap().clone();
        self.apply_swap(last_el);
        self.stack.pop();

        if last_el.0.0 == 0 && last_el.0.1 == 0 && last_el.1.0 == 0 && last_el.1.1 == 0 {
            last_el.1.0 = 1;
            last_el.1.1 = 0;
            self.apply_swap(last_el);
            self.stack.push(last_el);

            
            let mut child_el = last_el.clone();

            // incr x 
            if child_el.0.1 < self.data[child_el.0.0].len() - 1 {
                child_el.0.1 += 1;
                if child_el.1.1 < self.data[child_el.1.0].len() - 1 {
                    child_el.1.1 += 1;
                }
                else {
                    if child_el.1.0 < self.data.len() - 1 {
                        child_el.1.0 += 1;
                        child_el.1.1 = 0;
                    }
                    else {
                        return;
                    }
                }
            }
            else if child_el.0.0 < self.data.len() - 2 {
                child_el.0.0 += 1;
                child_el.0.1 = 0;
                child_el.1.0 = child_el.0.0 + 1;
                child_el.1.1 = 0;
            }
            else {
                return;
            }

            self.apply_swap(child_el);
            self.stack.push(child_el);
            
            return;
        }

        if last_el.1.1 < self.data[last_el.1.0].len() - 1 {
            last_el.1.1 += 1;
        }
        else if last_el.1.0 < self.data.len() - 1 {
            last_el.1.0 += 1;
            last_el.1.1 = 0;
        }
        else if last_el.0.1 < self.data[last_el.0.0].len() - 1 {
            last_el.0.1 += 1;
            last_el.1.0 = last_el.0.0 + 1;
            last_el.1.1 = 0;
        }
        else if last_el.0.0 < self.data.len() - 2 {
            last_el.0.0 += 1;
            last_el.0.1 = 0;
            last_el.1.0 = last_el.0.0 + 1;
            last_el.1.1 = 0;
        }
        else {
            return;
        }

        self.apply_swap(last_el);
        self.stack.push(last_el);

        let mut child_el = last_el.clone();

        // incr x 
        if child_el.0.1 < self.data[child_el.0.0].len() - 1 {
            child_el.0.1 += 1;
            if child_el.1.1 < self.data[child_el.1.0].len() - 1 {
                child_el.1.1 += 1;
            }
            else {
                if child_el.1.0 < self.data.len() - 1 {
                    child_el.1.0 += 1;
                    child_el.1.1 = 0;
                }
                else {
                    return;
                }
            }
        }
        else if child_el.0.0 < self.data.len() - 2 {
            child_el.0.0 += 1;
            child_el.0.1 = 0;
            child_el.1.0 = child_el.0.0 + 1;
            child_el.1.1 = 0;
        }
        else {
            return;
        }

        self.apply_swap(child_el);
        self.stack.push(child_el);

    }

    fn can_incr(&self, i_j: ((usize, usize), (usize, usize))) -> bool {
        i_j.1.0 < self.data.len() - 1 
        || i_j.1.1 < self.data[i_j.1.0].len() - 1 
        || i_j.0.0 < self.data.len() - 2
        || i_j.0.1 < self.data[i_j.0.0].len() - 1
    }

    fn incr(&self, i_j: &mut ((usize, usize), (usize, usize))) {
        if i_j.0.0 > self.data.len() - 2 || i_j.1.0 > self.data.len() - 1 {
            panic!("Overflow");
        }

        if i_j.1.1 < self.data[i_j.1.0].len() - 1 {
            i_j.1.1 += 1;
        }
        else if i_j.1.0 < self.data.len() - 1 {
            i_j.1.0 += 1;
            i_j.1.1 = 0;
        }
        else if i_j.0.1 < self.data[i_j.0.0].len() - 1 {
            i_j.0.1 += 1;
            i_j.1.0 = i_j.0.0 + 1;
            i_j.1.1 = 0;
        }
        else if i_j.0.0 < self.data.len() - 2 {
            i_j.0.0 += 1;
            i_j.0.1 = 0;
            i_j.1.0 = i_j.0.0 + 1;
            i_j.1.1 = 0;
        }
    }

    fn apply_swap(&mut self, i_j: ((usize, usize), (usize, usize))) {
        let t = self.data[i_j.0.0][i_j.0.1];
        self.data[i_j.0.0][i_j.0.1] = self.data[i_j.1.0][i_j.1.1];
        self.data[i_j.1.0][i_j.1.1] = t;
    }

    pub fn get_vector(&self) -> Vec<usize> {
        let mut v = vec![];
        for i in self.data.clone() {
            for j in i {
                v.push(j);
            }
        }

        v
    }
}

mod test {
    use super::*;


    #[test]
    fn test_generate_set_permutations() {
        let mut permuter = SetPermuter::new(vec![vec![0, 1], vec![2, 3, 4]]);

        println!("{:?}", permuter.get_vector());
        while permuter.has_next() {
            permuter.generate_next();
            println!("{:?}, {:?}", permuter.get_vector(), permuter.stack);
        }
        return;

        let expected: Vec<Vec<usize>> = vec![
            vec![2, 1, 0, 3],
            vec![2, 3, 0, 1],
            vec![3, 1, 2, 0],
            vec![0, 2, 1, 3],
            vec![0, 3, 2, 1]
        ];

        assert_eq!(permuter.get_vector(), vec![0, 1, 2, 3]);

        for ex in expected {
            assert_eq!(permuter.has_next(), true);
            permuter.generate_next();
            assert_eq!(permuter.get_vector(), ex);
        }
        
        assert_eq!(permuter.has_next(), false);
    }
}