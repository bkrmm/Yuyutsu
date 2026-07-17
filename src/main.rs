mod moves;
mod board;

use crate::board::Color;

fn main() {
    let b = board::Board::new();
    //let moves = generate_legal_move(&b, Color:: White);
    b.make_move(&moves[0]);
    println!("{b}");
}
