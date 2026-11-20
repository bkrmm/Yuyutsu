//! The bitboard representation itself: square mapping, occupancy bookkeeping,
//! attack tables, and the repetition key.
//!
//! The chess rules are covered by the perft and special-move suites. These tests
//! cover the substrate underneath them, because a wrong attack table or a stale
//! occupancy bit produces legal-looking moves in the wrong places rather than an
//! obvious failure.

mod common;

use common::{piece as p, position};
use yuyutsu::board::{
    Board, Color, PieceType, bishop_attacks, king_attacks, knight_attacks,
    queen_attacks, rook_attacks, square_index, square_of,
};
use yuyutsu::moves::generate_rook_moves;

/// Confirms the square-to-bit mapping is a bijection over all 64 squares.
///
/// Technically: every bitboard operation assumes `row * 8 + column` and its
/// inverse agree. A duplicated or skipped index would silently alias two squares
/// and corrupt piece placement, so this is checked exhaustively rather than on a
/// sample.
#[test]
fn square_indices_round_trip() {
    let mut seen = [false; 64];

    for row in 0..8 {
        for column in 0..8 {
            let index = square_index((row, column));
            assert!(index < 64, "index out of range for ({row}, {column})");
            assert!(!seen[index], "index {index} used twice");
            seen[index] = true;
            assert_eq!(square_of(index), (row, column));
        }
    }

    assert!(seen.iter().all(|&used| used), "not every index was produced");
}

/// Confirms placing and replacing pieces keeps both occupancy caches exact.
///
/// Technically: `occupied` and `occupied_by` are cached rather than recomputed,
/// so a replacement that forgets to clear the old piece would leave a phantom
/// bit. The test walks through add, replace, and clear, checking all three views
/// of the board at each step.
#[test]
fn set_piece_maintains_occupancy() {
    let mut board = Board::empty();
    assert_eq!(board.occupied(), 0);

    board.set_piece((4, 4), Some(p(Color::White, PieceType::Queen)));
    assert_eq!(board.piece_at((4, 4)), Some(p(Color::White, PieceType::Queen)));
    assert_eq!(board.occupied().count_ones(), 1);
    assert_eq!(board.occupied_by(Color::White).count_ones(), 1);
    assert_eq!(board.occupied_by(Color::Black), 0);

    // Replace a White queen with a Black rook: both colors must change, and the
    // total must not grow.
    board.set_piece((4, 4), Some(p(Color::Black, PieceType::Rook)));
    assert_eq!(board.piece_at((4, 4)), Some(p(Color::Black, PieceType::Rook)));
    assert_eq!(board.occupied().count_ones(), 1);
    assert_eq!(board.occupied_by(Color::White), 0);
    assert_eq!(board.occupied_by(Color::Black).count_ones(), 1);

    board.set_piece((4, 4), None);
    assert_eq!(board.piece_at((4, 4)), None);
    assert_eq!(board.occupied(), 0);
    assert_eq!(board.occupied_by(Color::Black), 0);
}

/// Confirms the precomputed attack tables on an empty board.
///
/// Technically: these counts are geometric constants, not chess opinions. d4 is
/// a central square with the maximum reach for each slider; a1 is a corner that
/// truncates a knight to two jumps and a king to three steps.
#[test]
fn attack_tables_match_open_board_geometry() {
    assert_eq!(rook_attacks((4, 3), 0).count_ones(), 14);
    assert_eq!(bishop_attacks((4, 3), 0).count_ones(), 13);
    assert_eq!(queen_attacks((4, 3), 0).count_ones(), 27);

    assert_eq!(knight_attacks((7, 0)).count_ones(), 2);
    assert_eq!(knight_attacks((7, 0)), (1 << square_index((5, 1))) | (1 << square_index((6, 2))));

    assert_eq!(king_attacks((7, 0)).count_ones(), 3);}

/// Confirms a ray stops at the first blocker and does not pass through it.
///
/// Technically: the rook is on a1 with a friendly pawn on a4. The ray should
/// reach a2 and a3, stop at a4, and never see a5 or beyond. Friendly-piece
/// masking then removes a4 from the move list while leaving the rank-one moves
/// intact.
#[test]
fn rays_stop_at_the_nearest_blocker() {
    let board = position(
        &[
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((4, 0), p(Color::White, PieceType::Pawn)),
        ],
        Color::White,
    );

    let moves = generate_rook_moves(&board, 7, 0, Color::White);
    let targets: Vec<_> = moves.iter().map(|mv| mv.to).collect();

    assert!(targets.contains(&(6, 0)), "a2 must be reachable");
    assert!(targets.contains(&(5, 0)), "a3 must be reachable");
    assert!(!targets.contains(&(4, 0)), "the friendly blocker must be excluded");
    assert!(!targets.contains(&(3, 0)), "the ray must not pass the blocker");
    assert_eq!(targets.len(), 9, "two files north plus the seven rank squares");
}

/// Confirms the repetition key ignores insertion order and the clock, but not
/// the pieces, side, or castling rights.
///
/// Technically: the key is an exact structural copy of the bitboards plus the
/// three position flags. Two boards built by placing the same pieces in opposite
/// order must therefore be identical, while any chess-relevant change must split
/// them. The halfmove clock is excluded by FIDE 9.2.2.
#[test]
fn position_key_compares_structure_not_history() {
    let white_king = ((7, 4), p(Color::White, PieceType::King));
    let black_king = ((0, 7), p(Color::Black, PieceType::King));

    let forward = position(&[white_king, black_king], Color::White);
    let reverse = position(&[black_king, white_king], Color::White);
    assert_eq!(forward.position_key(), reverse.position_key());

    let mut older = forward.clone();
    older.halfmove_clock = 99;
    assert_eq!(forward.position_key(), older.position_key());

    let mut black_to_move = forward.clone();
    black_to_move.side_to_move = Color::Black;
    assert_ne!(forward.position_key(), black_to_move.position_key());

    let mut with_rook = forward.clone();
    with_rook.set_piece((7, 0), Some(p(Color::White, PieceType::Rook)));
    assert_ne!(forward.position_key(), with_rook.position_key());
}

/// Confirms make/unmake leaves the cached occupancy exactly as it found it.
///
/// Technically: `unmake_move` restores bitboards and both occupancy words. The
/// debug assertions in the library also check this, but a test makes the
/// invariant explicit and fails in release builds too if the cache drifts.
#[test]
fn make_unmake_round_trips_the_occupancy_caches() {
    let mut board = Board::new();
    let before_occupied = board.occupied();
    let before_white = board.occupied_by(Color::White);
    let before_black = board.occupied_by(Color::Black);

    let moves = board.generate_legal_moves();
    assert!(!moves.is_empty());

    for mv in moves {
        let undo = board.make_move(&mv).expect("legal move rejected");
        board.unmake_move(&undo);

        assert_eq!(board.occupied(), before_occupied, "total occupancy drifted for {mv:?}");
        assert_eq!(board.occupied_by(Color::White), before_white, "white occupancy drifted");
        assert_eq!(board.occupied_by(Color::Black), before_black, "black occupancy drifted");
    }
}
