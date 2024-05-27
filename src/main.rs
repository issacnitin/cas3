mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
mod permutation;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 2, vec![1, 4, 9, 16, 25]);
    //let explorer: StateExplorer = StateExplorer::new(2, 2, vec![1, 2, 3, 4, 5]);
    explorer.explore();
}   
