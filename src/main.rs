mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
mod set;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(2, 2, vec![1, 3, 5, 7, 9]);
    explorer.explore();
}   
