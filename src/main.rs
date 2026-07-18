#![allow(dead_code)]
#![allow(unused)]

mod moves;
mod board;

use crate::board::Color;

fn main() {
    let mut b = board::Board::new();
    let moves_list = moves::generate_all_moves(&b, board::Color::White); //implement a legality filter over this fn
    if !moves_list.is_empty() {
        b.make_move(&moves_list[0]);
    }
    println!("{b}");
}
