mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 4, vec![1,2,3,5,7,11,13,17,19,23,29]);
    explorer.explore();
}   
