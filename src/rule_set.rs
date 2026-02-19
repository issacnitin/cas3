/// A RuleSet with separate birth and survival conditions,
/// analogous to Conway's Game of Life (B3/S23).
///
/// - **Birth**: evaluated on Unset cells. If true → cell becomes Set.
/// - **Survival**: evaluated on Set cells. If true → cell stays Set; if false → cell becomes Unset.
///
/// This is strictly more expressive than a single rule with a Set/Flip action,
/// because cells can now die — enabling non-monotonic sequences like primes.

use crate::graph::op_node::OpNode;

#[derive(Debug, Clone)]
pub struct RuleSet {
    pub birth_condition: OpNode,
    pub survival_condition: OpNode,
}

impl RuleSet {
    pub fn new(dim_len: usize) -> Self {
        let end_index = (3_usize).pow(dim_len as u32) - 1;
        RuleSet {
            birth_condition: OpNode::new(0, end_index),
            survival_condition: OpNode::new(0, end_index),
        }
    }

    pub fn evaluate_birth(&self, v: &[bool]) -> bool {
        self.birth_condition.evaluate(v, true)
    }

    pub fn evaluate_survival(&self, v: &[bool]) -> bool {
        self.survival_condition.evaluate(v, true)
    }

    pub fn print(&self) {
        print!("Birth: ");
        self.birth_condition.print();
        println!();
        println!("Birth Permutation: {:?}", self.birth_condition.eval_permutation.get_vector());
        print!("Survival: ");
        self.survival_condition.print();
        println!();
        println!("Survival Permutation: {:?}", self.survival_condition.eval_permutation.get_vector());
    }

    pub fn debug_print(&self) {
        if !cfg!(debug_assertions) {
            return;
        }
        self.print();
        println!("Birth Cluster: {:?}", self.birth_condition.get_clustered_variables());
        println!("Survival Cluster: {:?}", self.survival_condition.get_clustered_variables());
    }
}
