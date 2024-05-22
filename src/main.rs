mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(2, 7, vec![1,2,3,5,7,11,13]);
    explorer.explore();
}   
