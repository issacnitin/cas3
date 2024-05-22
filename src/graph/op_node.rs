#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    // Parent Node properties
    And,
    Or,

    // Leaf node property
    None,
    Not
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpNode {
    // Parent Node properties
    operation : Op,
    left_child: Option<Box<OpNode>>,
    right_child: Option<Box<OpNode>>,
    
    // Aligned to left
    // So if split index = start index, it'll split the array [1], [2..]
    split_index: usize,

    // Leaf node properties
    start_index: usize,
    end_index: usize
}

// Binary Operation Tree
impl OpNode {
    pub fn new(start_index: usize, end_index: usize) -> Self {
        let mut node = OpNode {
            operation: Op::None,
            left_child: None,
            right_child: None,
            start_index: start_index,
            end_index: end_index,
            split_index: 0
        };
        node.reset(start_index, end_index, true);
        return node;
    }

    pub fn debug_print(&self) {
        if !cfg!(debug_assertions) {
            return;
        }

        self.print();
    }

    pub fn print(&self) {
        // Leaf node
        if self.start_index == self.end_index {
            if self.operation == Op::Not {
                
                print!("!");
            }
            print!("{}", self.start_index);
            return;
        }

        // Parent node
        print!("( ");

        if self.left_child != None {
            self.left_child.as_ref().unwrap().print();
        }

        if self.operation == Op::And {
            print!(" & ");
        }
        else if self.operation == Op::Or {
            print!(" | ");
        }

        if self.right_child != None {
            self.right_child.as_ref().unwrap().print();
        }

        print!(" )");
    }

    pub fn len(&self) -> usize {
        self.end_index - self.start_index + 1
    }

    pub fn has_next(&self) -> bool {
        if self.start_index == self.end_index
        {
            // Leaf node
            return self.operation == Op::None;
        }

        (self.left_child != None && self.left_child.as_ref().unwrap().has_next())
        || (self.right_child != None && self.right_child.as_ref().unwrap().has_next())
        || self.operation == Op::And
        || self.split_index < self.end_index - 1
    }

    fn reset(&mut self, start_index: usize, end_index: usize, reset_split: bool) {  
        if start_index > end_index {
            panic!("start_index < end_index while reset");
        } 

        self.start_index = start_index;
        self.end_index = end_index;
        if reset_split {
            self.split_index = start_index;
        }

        if start_index == end_index {
            self.operation = Op::None;
            return;
        }

        self.operation = Op::And;

        let mut left_node = OpNode::new(start_index, self.split_index);
        let mut right_node: OpNode = OpNode::new(self.split_index+1, end_index);

        self.left_child = Some(Box::new(left_node));
        self.right_child = Some(Box::new(right_node));
    }

    // value has corresponding (T/F)
    pub fn evaluate(&self, values: &Vec<bool>) -> bool {
        if values.len() == 0 {
            panic!("Invalid input");
        }
        
        if self.start_index > values.len() || self.end_index > values.len() {
            panic!("Start Index {}, End Index {}, greater than passed in vector of length {}", self.start_index, self.end_index, values.len());
        }

        // Leaf node
        if self.start_index == self.end_index {
            if self.operation == Op::None {
                return values[self.start_index];
            }
            else if self.operation == Op::Not {
                return !values[self.start_index];
            }
            panic!("Invalid input");
        }

        // Parent node
        let left_eval = self.left_child.as_ref().unwrap().evaluate(values);
        let right_eval = self.right_child.as_ref().unwrap().evaluate(values);
        
        if self.operation == Op::And {
            return left_eval && right_eval;
        }
        else if self.operation == Op::Or {
            return left_eval || right_eval;
        }
        
        panic!("Invalid input");
    }

    pub fn generate_next(&mut self) {
        // Check if leaf node
        if self.start_index == self.end_index {
            if self.operation == Op::None {
                self.operation = Op::Not;
                return;
            }
            panic!("Leaf node exhausted next-gen");
        }

        // Non-leaf node

        // Check if left_child can be restructured
        if self.left_child != None && self.left_child.as_ref().unwrap().has_next() {
            self.left_child.as_mut().unwrap().generate_next();
            return;
        }
        
        // If left_child cannot be, check if right_child can be
        // If right_child is being restructured, reset left_child to initial state
        if self.right_child != None && self.right_child.as_mut().unwrap().has_next() {
            self.right_child.as_mut().unwrap().generate_next();
            self.left_child.as_mut().unwrap().reset(self.start_index, self.split_index, true);
            return;
        }


        // Explore the same structure with Or rule as current node
        // And -> Or
        if self.operation == Op::And {
            self.reset(self.start_index, self.end_index, false);
            self.operation = Op::Or;
            return;
        }

        if self.split_index + 1 == self.end_index {
            panic!("All possible trees generated");
        }

        // Explore a new structure

        // Reset my operation to And
        self.operation = Op::And;
        // Increase split index, so children are different structure wise
        self.split_index += 1;

        let mut left_node = OpNode::new(self.start_index, self.split_index);
        left_node.reset(self.start_index, self.split_index, true);

        let mut right_node = OpNode::new(self.split_index+1, self.end_index);

        self.left_child = Some(Box::new(left_node));
        self.right_child = Some(Box::new(right_node));
    }
}

mod test {
    use super::*;

    #[test]
    fn test_op_node_eval() {
        let mut leaf_node = OpNode::new(0, 0);

        assert_eq!(leaf_node.evaluate(&vec![true]), true);
        leaf_node.generate_next();
        assert_eq!(leaf_node.evaluate(&vec![true]), false);

        let mut two_var_node = OpNode::new(0, 1);

        // None And None
        assert_eq!(two_var_node.evaluate(&vec![true, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), false);

        // Not AND None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), false);

        // None AND Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), false);
        
        // Not AND Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), true);

        // Switch to OR
        // None OR None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), false);

        // Not OR None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), true);

        // None OR Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), true);


        // Not OR Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true]), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true]), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false]), true);

    }


    #[test]
    fn test_op_node_generate() {
        let mut leaf_node = OpNode::new(0, 0);

        test_leaf_node(&mut leaf_node, 0);

        let mut and_node = OpNode::new(0, 1);

        assert_eq!(and_node.operation, Op::And);
        assert_eq!(and_node.start_index, 0);
        assert_eq!(and_node.end_index, 1);
        
        assert_ne!(and_node.left_child, None);
        assert_ne!(and_node.right_child, None);

        let mut and_node_left_child = and_node.left_child.clone().unwrap();
        let mut and_node_right_child = and_node.right_child.clone().unwrap();

        test_leaf_node(&mut and_node_left_child, 0);
        test_leaf_node(&mut and_node_right_child, 1);

        // Being made Or here
        assert_eq!(and_node.has_next(), true);

        and_node.generate_next();
        assert_eq!(and_node.operation, Op::And);
        assert_eq!(and_node.has_next(), true);
        assert_eq!(and_node.left_child.clone().unwrap().operation, Op::Not);
        assert_eq!(and_node.right_child.clone().unwrap().operation, Op::None);

        and_node.generate_next();
        assert_eq!(and_node.operation, Op::And);
        assert_eq!(and_node.has_next(), true);
        assert_eq!(and_node.left_child.clone().unwrap().operation, Op::None);
        assert_eq!(and_node.right_child.clone().unwrap().operation, Op::Not);

        and_node.generate_next();
        assert_eq!(and_node.operation, Op::And);
        assert_eq!(and_node.has_next(), true);
        assert_eq!(and_node.left_child.clone().unwrap().operation, Op::Not);
        assert_eq!(and_node.right_child.clone().unwrap().operation, Op::Not);

        and_node.generate_next();
        assert_eq!(and_node.operation, Op::Or);
        assert_eq!(and_node.has_next(), true);
        assert_eq!(and_node.left_child.clone().unwrap().operation, Op::None);
        assert_eq!(and_node.right_child.clone().unwrap().operation, Op::None);
        

        let mut three_child_node = OpNode::new(0, 2);

        let mut counter = 1;
        while three_child_node.has_next() {
            three_child_node.generate_next();
            counter += 1;
        }

        // TODO: Verify
        assert_eq!(counter, 64);
    }

    fn test_leaf_node(node: &mut OpNode, expected_index: usize) {
        assert_eq!(node.start_index, expected_index);
        assert_eq!(node.end_index, expected_index);
        assert_eq!(node.left_child, None);
        assert_eq!(node.right_child, None);
        if node.has_next() {
            assert_eq!(node.operation, Op::None);
            node.generate_next();
            assert_eq!(node.has_next(), false);
            assert_eq!(node.operation, Op::Not);
        }
        else {
            assert_eq!(node.operation, Op::Not);
        }
    }
}