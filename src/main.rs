use yuyutsu::board::{self, in_check};

/// Entry point: plays the first legal move from the starting position and shows
/// the resulting board.
///
/// Technically: this is a smoke-test harness, not a user interface. It uses
/// `generate_legal_moves` rather than the raw pseudo-legal generator, so the
/// move it picks can no longer strand its own king. The empty-list arm is a real
/// outcome -- checkmate or stalemate -- and is reported as such instead of being
/// silently skipped.
fn main() {
    let mut b = board::Board::new();

    let moves_list = b.generate_legal_moves();
    println!("{} legal moves for {:?}", moves_list.len(), b.side_to_move);

    match moves_list.first() {
        Some(mv) => match b.make_move(mv) {
            Ok(_undo) => {}
            Err(e) => println!("make_move rejected {mv:?}: {e:?}"),
        },
        None => {
            let state = if in_check(&b, b.side_to_move) { "checkmate" } else { "stalemate" };
            println!("no legal moves: {state}");
        }
    }

    println!("{b}");
}
