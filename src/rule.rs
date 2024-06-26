/*
* A rule is a boolean expression of surrounding cells's Set/Unset values
* For a given cell, there are 2^N surrounding cells.
* Surrounding cells can be iterated on in any order, but order is fixed for a rule
* 
* A rule-element hence has an applicable co-ordinate relative to current cell
* and an expected cell value (True or False)
*
* For a cell of N-dimension, 2^N rule elements are required:
* For each applicable coordinate, there will be a rule element
* Rule contains the order in which the coordinate check should be applied,
* along with result (Set/Unset/Flip current cell)
*/

use crate::space::Space;
use crate::graph::op_node::OpNode;
use crate::cell::{Cell, CellValue};

#[derive(Debug, PartialEq, Clone)]
pub enum Action {
    Set,
    Unset,
    Flip
}

#[derive(Debug, PartialEq, Clone)]
pub struct Rule {
    condition: OpNode,
    pub result: Action
}

impl Rule {
    pub fn new(dim_len: usize) -> Self {
        Rule {
            // 3 ain't a magic number
            // for spacial dimension of any number
            // there are 3^N surrounding cells (including not just neighbours)
            // What if the rule in reality only considers it's neighbours ? If that's the case
            // There's 2*N nodes only and we can potentially explore many more dimensions
            condition: OpNode::new(0, (3 as usize).pow(dim_len as u32) - 1),
            result: Action::Set
        }
    }

    pub fn debug_print(&self) {
        if !cfg!(debug_assertions) {
            return;
        }

        self.print();
        println!("Cluster: {:?}", self.condition.get_clustered_variables());
    }

    pub fn print(&self) {
        print!("Rule: ");
        self.condition.print();
        println!("");
        println!("Permutation: {:?}", self.condition.eval_permutation.get_vector());
        println!("Action: {:?}", self.result);
    }

    pub fn has_next_eval_permutation(&self) -> bool {
        self.condition.has_next_eval_permutation()
    }

    pub fn generate_next_eval_permutation(&mut self) {
        // Only called on root node
        self.condition.generate_next_eval_permutation();
    }

    pub fn has_next(&self) -> bool {
        self.result == Action::Set || self.condition.has_next()
    }

    pub fn generate_next(&mut self) {
        if self.result == Action::Set {
            self.result = Action::Flip;
            return;
        }
        
        self.result = Action::Set;
        self.condition.generate_next();
    }

    pub fn evaluate(&self, v: &Vec<bool>) -> bool {
        self.condition.evaluate(v, true)
    }
}