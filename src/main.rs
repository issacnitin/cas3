mod cell;
mod space;
mod rule;
use cell::Cell;
use space::Space;
use rule::Rule;

fn main() {
    let mut cell : Cell = Cell::new(10);
    println!("{:?}", cell);
    
    let mut space: Space = Space::new(10);
    space.push(cell);
    println!("{:?}", space);

    space.apply_rules(1);
    println!("{:?}", space);


    space.apply_rules(2);
    println!("{:?}", space);
}
