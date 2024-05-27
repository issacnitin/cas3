mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
mod permutation;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 2, vec![1, 2, 3, 5, 7, 11, 13, 17, 19]);
    //let explorer: StateExplorer = StateExplorer::new(2, 2, vec![1, 2, 3, 4, 5]);
    explorer.explore();
}   
