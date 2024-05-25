use std::cmp::min;


#[derive(Clone, Debug, PartialEq)]
pub struct VectorPermuter {
    vector: Vec<usize>,
    x: usize,
    y: usize,
    child: Option<Box<VectorPermuter>>,
    stack_size: usize
}

impl VectorPermuter {
    pub fn new(len: usize) -> Self {
        let mut permuter = VectorPermuter{
            vector: vec![],
            x: 0,
            y: 0,
            child: None,
            stack_size: 0
        };

        for i in 0..len {
            permuter.vector.push(i);
        }

        permuter
    }

    pub fn reset(&mut self) {
        let len = self.vector.len();
        self.vector.clear();
        for i in 0..len {
            self.vector.push(i);
        }
        self.x = 0;
        self.y = 0;
    }

    pub fn has_next(&self) -> bool {
        if self.child != None {
            if self.child.as_ref().unwrap().has_next() {
                return true;
            }
        }
        return self.y < self.vector.len() - 1 || self.x < self.vector.len() - 2 || self.child != None;
    }

    pub fn generate_next(&mut self) {
        if self.x == 0 && self.y == 0 {
            self.y = 1;
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

            if self.y < self.vector.len() - 1 {
                self.y += 1;
            }
            else if self.x < self.vector.len() - 2 {
                self.x += 1;
                self.y = self.x + 1;
            }
            else {
                panic!("Overflow");
            }
        }
    }

    fn generate_child(&mut self) {
        let mut child_permuter = self.clone();
        child_permuter.replace(); 
        child_permuter.x = min(child_permuter.x + 1, child_permuter.vector.len() - 1);
        child_permuter.y = min(child_permuter.x + 1, child_permuter.vector.len() - 1);

        child_permuter.stack_size += 1;
        self.child = Some(Box::new(child_permuter));
    }

    pub fn get_vector(&self) -> Vec<usize> {
        if self.child != None {
            return self.child.as_ref().unwrap().get_vector();
        }

        let mut v = self.vector.clone();
        v.swap(self.x, self.y);
        v
    }

    fn replace(&mut self) {
        self.vector.swap(self.x, self.y);
    }
}

mod test {
    use super::VectorPermuter;

    #[test]
    fn test_vector_permutation() {
        let mut permuter = VectorPermuter::new(4);
        assert_eq!(permuter.get_vector(), vec![0, 1, 2, 3]);
        let ordered_vecs = vec![
            vec![1, 0, 2, 3],
            vec![1, 2, 0, 3],
            vec![1, 2, 3, 0],
            vec![1, 3, 2, 0],
            vec![1, 3, 0, 2],
            vec![1, 0, 3, 2],
            vec![2, 1, 0, 3],
            vec![2, 0, 1, 3],
            vec![2, 0, 3, 1],
            vec![2, 3, 0, 1],
            vec![2, 3, 1, 0],
            vec![2, 1, 3, 0],
            vec![3, 1, 2, 0],
            vec![3, 2, 1, 0],
            vec![3, 2, 0, 1],
            vec![3, 0, 2, 1],
            vec![3, 0, 1, 2],
            vec![3, 1, 0, 2],
            vec![0, 2, 1, 3],
            vec![0, 2, 3, 1],
            vec![0, 3, 2, 1],
            vec![0, 3, 1, 2],
            vec![0, 1, 3, 2]
        ];

        for v in ordered_vecs {
            assert_eq!(permuter.has_next(), true);
            permuter.generate_next();
            assert_eq!(permuter.get_vector(), v);
        }
    }
}