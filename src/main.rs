mod cell;
mod space;
mod rule;
mod state_explorer;
mod graph;
mod set;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 2, vec![1,2,4,8,16]);
    explorer.explore();
}   
