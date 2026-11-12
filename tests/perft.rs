//! Perft: the standard correctness oracle for a chess move generator.
//!
//! `perft(board, d)` counts the number of distinct legal move sequences of
//! exactly `d` plies. The published counts are non-negotiable, so any deviation
//! points at a concrete defect in generation, in legality filtering, or in the
//! make/unmake pair.
//!
//! Two positions are used. The starting position exercises ordinary moves and
//! promotion at depth 5+. Kiwipete is the standard second oracle: its castling
//! rights are live for both sides and its pawn structure produces en passant
//! captures early, so depth 1 already proves those two rules are wired into the
//! generator, the validator, and the `Undo` record.

mod common;

use common::{board_from_fen, perft, piece as p, position};
use yuyutsu::board::{Board, Color, InvalidMove, PieceType, in_check};
use yuyutsu::moves::{Move, generate_all_moves};

/// Published perft values for the starting position, per the Chess Programming
/// Wiki.
const START_POSITION_PERFT: [(u32, u64); 5] = [
    (0, 1),
    (1, 20),
    (2, 400),
    (3, 8_902),
    (4, 197_281),
];

/// FEN for Kiwipete, the standard position for castling and en passant.
const KIWIPETE_FEN: &str = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";

/// Published perft values for Kiwipete. Depth 4 is release-only.
const KIWIPETE_PERFT: [(u32, u64); 4] = [(0, 1), (1, 48), (2, 2_039), (3, 97_862)];

/// Verifies the shallow starting-position counts, which every build must pass.
///
/// Technically: loops from depth 0 to 4 so the trivial leaf case is covered too.
/// Afterwards the board is compared with a fresh `Board::new()`, which catches a
/// make/unmake pairing bug even if the node totals happen to match.
#[test]
fn start_position_perft_matches_published_counts() {
    let mut board = Board::new();

    for (depth, expected) in START_POSITION_PERFT {
        assert_eq!(perft(&mut board, depth), expected, "perft({depth})");
    }

    assert_eq!(board, Board::new(), "perft left the board mutated");
}

/// Verifies Kiwipete's shallow counts, including castling and en passant.
///
/// Technically: depth 1 is the strongest single assertion here -- Kiwipete's
/// 48 legal moves include both castles for White and no false ones, so a
/// castling-rights or transit-square bug changes the count immediately. Depths
/// 2 and 3 then exercise the rules inside a tree rather than at the root.
#[test]
fn kiwipete_perft_matches_published_counts() {
    let mut board = board_from_fen(KIWIPETE_FEN);

    for (depth, expected) in KIWIPETE_PERFT {
        assert_eq!(perft(&mut board, depth), expected, "perft({depth})");
    }

    assert_eq!(
        board,
        board_from_fen(KIWIPETE_FEN),
        "perft left the Kiwipete board mutated"
    );
}

/// Verifies the deep starting-position counts. Release builds only.
///
/// Technically: depth 6 visits 119,060,324 leaves, which is tolerable in an
/// optimized build and unacceptable in a debug one. The `cfg` removes the test
/// from debug binaries entirely rather than making it silently return, so a
/// debug run cannot be mistaken for coverage of these depths.
#[cfg(not(debug_assertions))]
#[test]
fn start_position_deep_perft_matches_published_counts() {
    let mut board = Board::new();

    for (depth, expected) in [(5, 4_865_609u64), (6, 119_060_324)] {
        assert_eq!(perft(&mut board, depth), expected, "perft({depth})");
    }

    assert_eq!(board, Board::new(), "perft left the board mutated");
}

/// Verifies Kiwipete at depth 4, where castling and en passant are unavoidable.
///
/// Technically: release-only for the same reason as the deep start count. The
/// expected value already includes the en passant discovered-check cases that
/// make Kiwipete valuable -- a filter that ignores them overcounts here.
#[cfg(not(debug_assertions))]
#[test]
fn kiwipete_deep_perft_matches_published_counts() {
    let mut board = board_from_fen(KIWIPETE_FEN);

    assert_eq!(perft(&mut board, 4), 4_085_603, "Kiwipete perft(4)");
    assert_eq!(board, board_from_fen(KIWIPETE_FEN), "perft left the board mutated");
}

/// Confirms the legality filter restores the board exactly.
///
/// Technically: `generate_legal_moves` makes and unmakes every candidate, so a
/// missing unmake would corrupt the caller's position even when the returned
/// move list looks right. Comparing the whole `Board` catches castling rights
/// and the en passant square too, not just piece placement.
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

/// Confirms an absolutely pinned knight cannot move.
///
/// Technically: White Ke1, White Ne2, Black Re8, Black Ka8. The knight shields
/// the king on the e-file, so every knight move exposes the king and must be
/// filtered out. Also pins the count at four, which rules out the king stepping
/// onto the occupied e2 square.
#[test]
fn an_absolutely_pinned_knight_cannot_move() {
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

/// Confirms that while in check only check-resolving moves are legal.
///
/// Technically: White Ke1, Black Re8, Black Ka8 -- open e-file, so White is in
/// check. The e2 step stays on the checking ray and must be removed; the four
/// off-file steps are legal.
#[test]
fn while_in_check_only_check_resolving_moves_are_legal() {
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

    assert!(
        !legal.iter().any(|m| m.to == (6, 4)),
        "king stepped along the checking ray: {legal:?}"
    );
    assert_eq!(legal.len(), 4, "expected 4 king moves, got {legal:?}");
}

/// Confirms checkmate produces an empty legal list while pseudo-legal moves exist.
///
/// Technically: back-rank mate. Black Kg8 is walled in by its own f7/g7/h7
/// pawns, White Re8 checks along the eighth rank. Asserting the pseudo-legal
/// list is non-empty proves the empty result came from the legality filter, not
/// from a broken generator.
#[test]
fn checkmate_position_has_no_legal_moves() {
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

/// Confirms stalemate is an empty legal list with no check.
///
/// Technically: Black Ka8, White Qb6, White Kc6. The queen covers a7, b7 and b8
/// without attacking a8, so Black has no move while not being in check. This is
/// the case that distinguishes `checkmate` from `stalemate` in the API.
#[test]
fn stalemate_position_has_no_legal_moves_but_is_not_check() {
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

/// Confirms a friendly blocker terminates a sliding ray.
///
/// Technically: White rook a1 with a white pawn on a3. The rook may reach a2
/// but must not generate a3 or anything beyond it. Guards the ray-walk
/// termination rule independently of perft, where a single blocked square is
/// easy to miss among millions of nodes.
#[test]
fn a_friendly_blocker_stops_a_ray() {
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

/// Confirms the validator rejects a rook move with perpendicular displacement.
///
/// Technically: a rook on a1 cannot reach c3. The ray walk tests only the axis
/// it is walking, so without an explicit perpendicular-zero check the target's
/// off-axis coordinate is silently ignored. Generators never emit such a move,
/// which is exactly why the validator needs its own test.
#[test]
fn a_rook_cannot_move_off_its_rank_or_file() {
    let mut board = position(
        &[
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((7, 4), p(Color::White, PieceType::King)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    let before = board.clone();

    let mv = Move { from: (7, 0), to: (5, 2), promotion: None };

    assert_eq!(board.make_move(&mv), Err(InvalidMove::BadGeometry));
    assert_eq!(board, before, "a rejected move must not mutate the board");
}
