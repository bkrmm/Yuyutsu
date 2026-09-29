use yuyutsu::{board, moves};

fn main() {
    let mut b = board::Board::new();
    let color = b.side_to_move;
    let moves_list = moves::generate_all_moves(&b, color);

    match moves_list.first() {
        Some(mv) => match b.make_move(mv) {
            Ok(_undo) => {}
            Err(e) => println!("make_move rejected {mv:?}: {e:?}"),
        },
        None => println!("no legal moves for {color:?}"),
    }

    println!("{b}");
}
