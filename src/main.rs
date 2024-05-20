mod cell;
mod space;
mod rule;
mod state_explorer;
use crate::state_explorer::StateExplorer;

fn main() {
    let explorer: StateExplorer = StateExplorer::new(4);
    explorer.explore();
}   
