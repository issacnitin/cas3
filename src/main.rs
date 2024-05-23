mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
mod set;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 1, vec![1,2,4,6,8,10]);
    explorer.explore();
}   
