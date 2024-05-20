mod cell;
mod space;
mod rule;
mod state_explorer;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(1, 7);
    explorer.explore();
}   
