use crate::set::set_permuter::Permuter;

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
    pub eval_permutation: Permuter,
    
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
            split_index: 0,
            eval_permutation: Permuter::new(vec![])
        };
        node.reset(start_index, end_index, true);
        node.eval_permutation.reset(node.get_clustered_variables());

        return node;
    }

    pub fn has_next_eval_permutation(&self) -> bool {
        self.eval_permutation.has_next()
    }

    pub fn generate_next_eval_permutation(&mut self) {
        // Only called on root node
        self.eval_permutation.generate_next();
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
            self.eval_permutation.reset(self.get_clustered_variables());
            return;
        }

        self.operation = Op::And;

        let left_node = OpNode::new(start_index, self.split_index);
        let right_node: OpNode = OpNode::new(self.split_index+1, end_index);

        self.left_child = Some(Box::new(left_node));
        self.right_child = Some(Box::new(right_node));
        self.eval_permutation.reset(self.get_clustered_variables());
    }

    /* 
     * Cluster of variables are invariant in forming permutations
     * variables should be exchanged across clusters
     * for ex. in case of x1 AND x2 AND x3, all variables should form one cluster
     * in case of (x1 AND x2) OR x3, x1 AND x2 should form a cluster and x3 should form another
     * So exchanging x1<->x3 and x2<->x3 make sense, but x1<->x2 doesn't
     * 
     * My current thought process is that NOT expression doesn't need permutations of input value
     * As we are exploring all possible NOT combinations as well
     * We have all possible boolean expressions with N variables
     * 
     */
    pub fn get_clustered_variables(&self) -> Vec<Vec<usize>> {
        // Leaf node
        if self.start_index == self.end_index {
            return vec![vec![self.start_index]];
        }

        let mut same_op_children: Vec<usize> = vec![];
        let mut result : Vec<Vec<usize>> = vec![];

        if self.left_child != None {
            let left_node = self.left_child.as_ref().unwrap().as_ref();

            let mut left_cluster = left_node.get_clustered_variables();
            if left_node.operation == self.operation || left_node.start_index == left_node.end_index {
                // Last index is same-op cleft_clusterluster
                // As we push it at the end
                same_op_children.append(&mut left_cluster.last().unwrap().clone());
                left_cluster = left_cluster.as_slice()[0..left_cluster.len() - 1].to_vec();
            }

            if left_cluster.len() > 0 {
                result.append(&mut left_cluster);
            }
        }

        if self.right_child != None {
            let right_node = self.right_child.as_ref().unwrap().as_ref();
            
            let mut right_cluster = right_node.get_clustered_variables();
            if right_node.operation == self.operation || right_node.start_index == right_node.end_index {
                same_op_children.append(&mut right_cluster.last().unwrap().clone());
                right_cluster = right_cluster.as_slice()[0..right_cluster.len()-1].to_vec();
            }

            if right_cluster.len() > 0 {
                result.append(&mut right_cluster);
            }
        }

        if same_op_children. len() > 0 {
            result.push(same_op_children);
        }
        
        result
    }

    // value has corresponding (T/F)
    pub fn evaluate(&self, values: &Vec<bool>, apply_permutation_at_current_level: bool) -> bool {
        if values.len() == 0 {
            panic!("Invalid input");
        }
        
        if self.start_index > values.len() || self.end_index > values.len() {
            panic!("Start Index {}, End Index {}, greater than passed in vector of length {}", self.start_index, self.end_index, values.len());
        }


        let evaluation_values: &Vec<bool>;
        let mut permuted_values: Vec<bool> = vec![];

        if apply_permutation_at_current_level {
            // Apply permutation
            // WARN: memory leak, avoid clone?
            /* Disable permutation for now */
            let v = self.eval_permutation.get_sequence();
            for ii in values {
                permuted_values.push(*ii);
            }

            evaluation_values = &permuted_values;
        }
        else {
            evaluation_values = values;
        }

        // Leaf node
        if self.start_index == self.end_index {
            if self.operation == Op::None {
                return evaluation_values[self.start_index];
            }
            else if self.operation == Op::Not {
                return !evaluation_values[self.start_index];
            }
            panic!("Invalid input");
        }

        // Parent node
        let left_eval = self.left_child.as_ref().unwrap().evaluate(&evaluation_values, false);
        let right_eval = self.right_child.as_ref().unwrap().evaluate(&evaluation_values, false);
        
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
                self.eval_permutation.reset(self.get_clustered_variables());
                return;
            }
            panic!("Leaf node exhausted next-gen");
        }

        // Non-leaf node

        // Check if left_child can be restructured
        if self.left_child != None && self.left_child.as_ref().unwrap().has_next() {
            self.left_child.as_mut().unwrap().generate_next();
            self.eval_permutation.reset(self.get_clustered_variables());
            return;
        }
        
        // If left_child cannot be, check if right_child can be
        // If right_child is being restructured, reset left_child to initial state
        if self.right_child != None && self.right_child.as_mut().unwrap().has_next() {
            self.right_child.as_mut().unwrap().generate_next();
            self.left_child.as_mut().unwrap().reset(self.start_index, self.split_index, true);
            self.eval_permutation.reset(self.get_clustered_variables());
            return;
        }


        // Explore the same structure with Or rule as current node
        // And -> Or
        if self.operation == Op::And {
            self.reset(self.start_index, self.end_index, false);
            self.operation = Op::Or;
            self.eval_permutation.reset(self.get_clustered_variables());
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

        let left_node = OpNode::new(self.start_index, self.split_index);
        let right_node = OpNode::new(self.split_index+1, self.end_index);

        self.left_child = Some(Box::new(left_node));
        self.right_child = Some(Box::new(right_node));

        self.eval_permutation.reset(self.get_clustered_variables());
    }
}

mod test {
    use super::*;

    #[test]
    fn test_variable_cluster() {
        let mut leaf_node = OpNode::new(0, 0);
        let mut result = leaf_node.get_clustered_variables();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0], 0);

        leaf_node.generate_next();
        result = leaf_node.get_clustered_variables();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 1);
        assert_eq!(result[0][0], 0);


        let two_var_node = OpNode::new(0, 1);
        result = two_var_node.get_clustered_variables();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);

        let mut three_var_node = OpNode::new(0, 2);
        result = three_var_node.get_clustered_variables();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);
        assert_eq!(result[0][2], 2);

        // Trigger change in structure from x1 AND x2 AND x3 to x1 AND (x2 OR x3)
        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[0][0], 1);
        assert_eq!(result[0][1], 2);
        assert_eq!(result[1][0], 0);
        three_var_node.print();
        println!();

        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[0][0], 1);
        assert_eq!(result[0][1], 2);
        assert_eq!(result[1][0], 0);
        three_var_node.print();
        println!();


        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        three_var_node.print();
        println!();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);
        assert_eq!(result[0][2], 2);


        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        three_var_node.print();
        println!();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);
        assert_eq!(result[0][2], 2);


        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        three_var_node.print();
        println!();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 3);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);
        assert_eq!(result[0][2], 2);


        for _ in 0..8 {
            three_var_node.generate_next();
        }
        result = three_var_node.get_clustered_variables();
        three_var_node.print();
        println!();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[1].len(), 1);
        assert_eq!(result[0][0], 0);
        assert_eq!(result[0][1], 1);
        assert_eq!(result[1][0], 2);


        let mut four_node = OpNode::new(0, 4);
        let mut four_node_left_child = OpNode::new(0, 2);
        let mut four_node_right_child = OpNode::new(3, 4);

        for _ in 0..8 {
            four_node_left_child.generate_next();
        }
        four_node.left_child = Some(Box::new(four_node_left_child));

        // ( ( 0 & ( 1 | 2 ) ) & ( !3 | 4 ) )
        for _ in 0..5 {
            four_node_right_child.generate_next();
        }
        four_node.right_child = Some(Box::new(four_node_right_child));

        four_node.print();
        println!();
        result = four_node.get_clustered_variables();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[1].len(), 2);
        assert_eq!(result[2].len(), 1);
        
    }


    #[test]
    fn test_op_node_eval() {
        let mut leaf_node = OpNode::new(0, 0);

        assert_eq!(leaf_node.evaluate(&vec![true], true), true);
        leaf_node.generate_next();
        assert_eq!(leaf_node.evaluate(&vec![true], true), false);

        let mut two_var_node = OpNode::new(0, 1);

        // None And None
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), false);

        // Not AND None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), false);

        // None AND Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), false);
        
        // Not AND Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), true);

        // Switch to OR
        // None OR None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), false);

        // Not OR None
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), true);

        // None OR Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), true);


        // Not OR Not
        two_var_node.generate_next();
        assert_eq!(two_var_node.evaluate(&vec![true, true], true), false);
        assert_eq!(two_var_node.evaluate(&vec![true, false], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, true], true), true);
        assert_eq!(two_var_node.evaluate(&vec![false, false], true), true);

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