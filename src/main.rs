use yuyutsu::game::Game;

/// Entry point: reports the game state, plays the first legal move from the
/// starting position, and shows the resulting board.
///
/// Technically: this is a smoke-test harness, not a user interface. `Game`
/// owns the repetition history and the halfmove clock, so `state` answers
/// checkmate, stalemate, and every draw rule in one place. The binary no longer
/// re-derives terminal state from an empty move list, which is exactly the
/// duplication `GameState` exists to remove.
fn main() {
    let mut game = Game::new();

    let moves_list = game.legal_moves();
    println!(
        "{} legal moves for {:?}",
        moves_list.len(),
        game.board().side_to_move
    );

    match moves_list.first() {
        Some(mv) => match game.play(mv) {
            Ok(_undo) => {}
            Err(e) => println!("make_move rejected {mv:?}: {e:?}"),
        },
        None => println!("no legal moves"),
    }

    println!("state: {:?}", game.state());
    println!("{}", game.board());
}
