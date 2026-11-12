//! Perft: the standard correctness oracle for a chess move generator.
//!
//! `perft(board, d)` counts the number of distinct legal move sequences of
//! exactly `d` plies. The counts for the starting position are published and
//! non-negotiable, so any deviation points at a concrete defect in generation,
//! in legality filtering, or in the make/unmake pair.
//!
//! Depths 1..=4 are usable as a clean oracle even though castling and en passant
//! are not implemented yet: en passant needs at least five plies to become
//! possible and castling needs roughly eight, so neither can occur in the tree
//! being counted. Those rules only become a prerequisite at depth 5 and beyond.

use yuyutsu::board::{Board, Color, Piece, PieceType, in_check};
use yuyutsu::moves::generate_all_moves;

/// Published perft values for the starting position, per the Chess Programming
/// Wiki.
const START_POSITION_PERFT: [(u32, u64); 5] = [
    (0, 1),
    (1, 20),
    (2, 400),
    (3, 8_902),
    (4, 197_281),
];

/// Counts legal move sequences of exactly `depth` plies starting from `board`.
///
/// Technically: depth 0 is the leaf case and counts 1 by definition. Bulk
/// counting is applied at depth 1 -- returning the number of legal moves rather
/// than making each one and recursing -- which yields the same leaf count while
/// skipping a full make/unmake round trip per leaf. Every recursive step is
/// paired with an unmake, so the caller's board is byte-identical on return.
fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = board.generate_legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0;
    for mv in moves {
        let undo = board.make_move(&mv).expect("legal move rejected by make_move");
        nodes += perft(board, depth - 1);
        board.unmake_move(&undo);
    }

    nodes
}

/// Builds a sparse position from `(square, piece)` pairs and a side to move.
///
/// Technically: bypasses `Board::new()` so tests can state a position directly.
/// Squares are `(row, column)` with row 0 as rank 8, matching the rest of the
/// engine. Only the listed pieces exist; everything else is empty.
fn position(pieces: &[((usize, usize), Piece)], side_to_move: Color) -> Board {
    let mut board = Board::empty();
    for &(sq, piece) in pieces {
        board.grid[sq.0][sq.1] = Some(piece);
    }
    board.side_to_move = side_to_move;
    board
}

/// Shorthand for constructing a piece of a given color and kind.
fn p(color: Color, kind: PieceType) -> Piece {
    Piece { color, kind }
}

#[test]
fn start_position_perft_matches_published_counts() {
    let mut board = Board::new();

    for (depth, expected) in START_POSITION_PERFT {
        assert_eq!(perft(&mut board, depth), expected, "perft({depth})");
    }

    // The recursion pairs every make with an unmake. If that pairing is wrong
    // anywhere, the board drifts and this catches it even when counts match.
    assert_eq!(board, Board::new(), "perft left the board mutated");
}

#[test]
fn generate_legal_moves_leaves_the_board_untouched() {
    let mut board = Board::new();
    let before = board.clone();

    let legal = board.generate_legal_moves();

    assert_eq!(board, before, "filter mutated the board");
    // From the starting position every pseudo-legal move is also legal.
    assert_eq!(legal.len(), 20);
    assert_eq!(legal.len(), generate_all_moves(&before, Color::White).len());
}

#[test]
fn an_absolutely_pinned_knight_cannot_move() {
    // White Ke1, White Ne2, Black Re8, Black Ka8. The knight shields the king
    // on the e-file, so every knight move would expose the king to the rook and
    // must be filtered out.
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((6, 4), p(Color::White, PieceType::Knight)),
            ((0, 4), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );

    assert!(!in_check(&board, Color::White), "the knight blocks the rook");

    let legal = board.generate_legal_moves();

    assert!(
        !legal.iter().any(|m| m.from == (6, 4)),
        "pinned knight was allowed to move: {legal:?}"
    );
    // Only the king may move: d1, f1, d2, f2. Stepping to e2 is blocked by the
    // knight itself.
    assert_eq!(legal.len(), 4, "expected 4 king moves, got {legal:?}");
}

#[test]
fn while_in_check_only_check_resolving_moves_are_legal() {
    // White Ke1, Black Re8, Black Ka8 -- open e-file, so White is in check.
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((0, 4), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );

    assert!(in_check(&board, Color::White));

    let legal = board.generate_legal_moves();

    // e2 is the one king step that stays on the checking ray, so it must be
    // removed. The other four steps leave the file and are legal.
    assert!(
        !legal.iter().any(|m| m.to == (6, 4)),
        "king stepped along the checking ray: {legal:?}"
    );
    assert_eq!(legal.len(), 4, "expected 4 king moves, got {legal:?}");
}

#[test]
fn checkmate_position_has_no_legal_moves() {
    // Back-rank mate: Black Kg8 walled in by its own f7/g7/h7 pawns, White
    // Re8 checking along the eighth rank. The king has no escape and the pawns
    // cannot block, so the legal list must be empty while the pseudo-legal
    // list is not.
    let mut board = position(
        &[
            ((0, 4), p(Color::White, PieceType::Rook)),
            ((0, 6), p(Color::Black, PieceType::King)),
            ((1, 5), p(Color::Black, PieceType::Pawn)),
            ((1, 6), p(Color::Black, PieceType::Pawn)),
            ((1, 7), p(Color::Black, PieceType::Pawn)),
            ((7, 0), p(Color::White, PieceType::King)),
        ],
        Color::Black,
    );

    assert!(in_check(&board, Color::Black));
    assert!(
        !generate_all_moves(&board, Color::Black).is_empty(),
        "the position must have pseudo-legal moves, otherwise this proves nothing"
    );
    assert_eq!(board.generate_legal_moves().len(), 0, "checkmate must have no legal moves");
}

#[test]
fn stalemate_position_has_no_legal_moves_but_is_not_check() {
    // Black Ka8, White Qb6, White Kc6. The queen covers a7, b7 and b8 without
    // attacking a8, so Black is not in check yet has nowhere to go. This is the
    // case the engine could not distinguish before `side_to_move` existed.
    let mut board = position(
        &[
            ((0, 0), p(Color::Black, PieceType::King)),
            ((2, 1), p(Color::White, PieceType::Queen)),
            ((2, 2), p(Color::White, PieceType::King)),
        ],
        Color::Black,
    );

    assert!(!in_check(&board, Color::Black), "stalemate must not be check");
    assert_eq!(board.generate_legal_moves().len(), 0, "stalemate must have no legal moves");
}

#[test]
fn a_friendly_blocker_stops_a_ray() {
    // White rook a1 with a white pawn on a3: the rook may reach a2 but must not
    // generate a3 or anything beyond it. Guards the ray-walk termination rule.
    let board = position(
        &[
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((5, 0), p(Color::White, PieceType::Pawn)),
            ((7, 4), p(Color::White, PieceType::King)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );

    let rook_moves: Vec<_> = generate_all_moves(&board, Color::White)
        .into_iter()
        .filter(|m| m.from == (7, 0))
        .collect();

    assert!(rook_moves.iter().all(|m| m.to != (5, 0)), "rook captured its own pawn");
    assert!(rook_moves.iter().all(|m| m.to.0 >= 6), "rook passed through a blocker");
    assert!(rook_moves.iter().any(|m| m.to == (6, 0)), "rook should reach a2");
}
