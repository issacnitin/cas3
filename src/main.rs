mod cell;
mod space;
mod rule;
mod rule_set;
mod state_explorer;
mod graph;
mod permutation;
mod lattice;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 2, vec![1, 2, 3, 5, 7, 11, 13, 17, 19, 23]);
    //let explorer: StateExplorer = StateExplorer::new(2, 2, vec![1, 2, 3, 4, 5]);

    // Single-rule search (monotonic sequences only — cells can't die)
    // explorer.explore();

    // Birth/Survival rule-set search (cells can die — supports primes!)
    explorer.explore_rule_sets();
}   
