//! Draw rules: fifty-move, threefold repetition, and insufficient material.
//!
//! These tests exercise both layers: `Board` owns the halfmove clock and the
//! material check, while `Game` owns repetition history. The repetition key is
//! deliberately exact, so the tests also pin the FIDE detail that an en passant
//! target only distinguishes a position when the capture is actually legal.

mod common;

use common::{board_from_fen, piece as p, position};
use yuyutsu::board::{Board, Color, Piece, PieceType};
use yuyutsu::game::{DrawReason, Game, GameState};
use yuyutsu::moves::Move;

/// Builds a non-promotion move between two squares.
///
/// Technically: every move in this file is ordinary except the one promotion
/// test, so fixing `promotion` to `None` removes noise from the fixtures.
fn mv(from: (usize, usize), to: (usize, usize)) -> Move {
    Move { from, to, promotion: None }
}

/// Builds a position with both kings plus the supplied extra pieces.
///
/// Technically: the two kings occupy e1 and e8, which are far enough apart to
/// avoid adjacency and keep every material case readable. The side to move is
/// White; insufficient material does not depend on whose turn it is.
fn with_kings(extra: &[((usize, usize), Piece)]) -> Board {
    let mut pieces = vec![
        ((7, 4), p(Color::White, PieceType::King)),
        ((0, 4), p(Color::Black, PieceType::King)),
    ];
    pieces.extend_from_slice(extra);
    position(&pieces, Color::White)
}

/// Confirms the halfmove clock resets on pawn moves and captures, advances on
/// quiet moves, and is restored exactly by unmake.
///
/// Technically: a single sequence exercises all three transitions. The final
/// board comparison catches a clock restored to the wrong ply even if every
/// intermediate value looked plausible.
#[test]
fn halfmove_clock_updates_and_unmake_restores_it() {
    let mut board = Board::new();
    let before = board.clone();

    let e4 = board.make_move(&mv((6, 4), (4, 4))).expect("e2e4");
    assert_eq!(board.halfmove_clock, 0, "a pawn move must reset the clock");

    let nf6 = board.make_move(&mv((0, 6), (2, 5))).expect("...Nf6");
    assert_eq!(board.halfmove_clock, 1, "a quiet move must advance the clock");

    let nc3 = board.make_move(&mv((7, 1), (5, 2))).expect("Nc3");
    assert_eq!(board.halfmove_clock, 2);

    let ne4 = board.make_move(&mv((2, 5), (4, 4))).expect("...Nxe4");
    assert_eq!(board.halfmove_clock, 0, "a capture must reset the clock");

    board.unmake_move(&ne4);
    assert_eq!(board.halfmove_clock, 2, "unmake must restore the capture clock");
    board.unmake_move(&nc3);
    assert_eq!(board.halfmove_clock, 1);
    board.unmake_move(&nf6);
    assert_eq!(board.halfmove_clock, 0);
    board.unmake_move(&e4);
    assert_eq!(board, before, "unwind did not restore the starting position");
}

/// Confirms a promotion resets the halfmove clock because it is a pawn move.
///
/// Technically: the moved piece before the move is the pawn, so the reset must
/// not depend on the promotion piece. Unmake restores both the pawn and its
/// previous clock value.
#[test]
fn promotion_resets_the_halfmove_clock_and_unmake_restores_it() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((1, 4), p(Color::White, PieceType::Pawn)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    board.halfmove_clock = 42;
    let before = board.clone();

    let promotion = Move { from: (1, 4), to: (0, 4), promotion: Some(PieceType::Queen) };
    let undo = board.make_move(&promotion).expect("e7e8=Q");

    assert_eq!(board.halfmove_clock, 0, "promotion must reset the clock");
    assert_eq!(board.grid[0][4], Some(p(Color::White, PieceType::Queen)));

    board.unmake_move(&undo);
    assert_eq!(board, before, "unmake did not restore the pawn or the clock");
}

/// Confirms the fifty-move rule fires at exactly 100 halfmoves.
///
/// Technically: 99 is still ongoing, the quiet rook move reaches 100 and draws,
/// and unplay drops back to 99. The rook keeps material sufficient, so the only
/// rule that can fire is the clock.
#[test]
fn fifty_move_draw_at_one_hundred_halfmoves() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    board.halfmove_clock = 99;
    let mut game = Game::from_board(board);

    assert_eq!(game.state(), GameState::Ongoing, "99 halfmoves is not a draw");

    let undo = game.play(&mv((7, 7), (6, 7))).expect("Rh2");
    assert_eq!(game.board().halfmove_clock, 100);
    assert_eq!(game.state(), GameState::Draw(DrawReason::FiftyMove));

    game.unplay(&undo);
    assert_eq!(game.board().halfmove_clock, 99);
    assert_eq!(game.state(), GameState::Ongoing, "unplay must remove the draw");
}

/// Confirms a pawn move or capture resets the clock and prevents the draw.
///
/// Technically: both are irreversible for the fifty-move rule. Each fixture sits
/// at 99 and performs the irreversible move, so a reset to zero is the only way
/// the state can remain ongoing.
#[test]
fn pawn_moves_and_captures_reset_the_fifty_move_counter() {
    // Pawn move at 99.
    let mut pawn_board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((6, 4), p(Color::White, PieceType::Pawn)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    pawn_board.halfmove_clock = 99;
    let mut pawn_game = Game::from_board(pawn_board);

    pawn_game.play(&mv((6, 4), (4, 4))).expect("e2e4");
    assert_eq!(pawn_game.board().halfmove_clock, 0);
    assert_eq!(pawn_game.state(), GameState::Ongoing);

    // Capture at 99.
    let mut capture_board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    capture_board.halfmove_clock = 99;
    let mut capture_game = Game::from_board(capture_board);

    capture_game.play(&mv((7, 0), (0, 0))).expect("Rxa8");
    assert_eq!(capture_game.board().halfmove_clock, 0);
    assert_eq!(capture_game.state(), GameState::Ongoing);
}

/// Confirms checkmate and stalemate outrank the fifty-move rule.
///
/// Technically: a checkmating move ends the game before any claim, and stalemate
/// is already terminal. Both fixtures set the clock to 100, so a wrong ordering
/// would report a fifty-move draw instead.
#[test]
fn checkmate_and_stalemate_take_precedence_over_fifty_move() {
    let mut mate_board = position(
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
    mate_board.halfmove_clock = 100;
    let mate_game = Game::from_board(mate_board);
    assert_eq!(mate_game.state(), GameState::Checkmate);

    let mut stale_board = position(
        &[
            ((0, 0), p(Color::Black, PieceType::King)),
            ((2, 1), p(Color::White, PieceType::Queen)),
            ((2, 2), p(Color::White, PieceType::King)),
        ],
        Color::Black,
    );
    stale_board.halfmove_clock = 100;
    let stale_game = Game::from_board(stale_board);
    assert_eq!(stale_game.state(), GameState::Stalemate);
}

/// Confirms threefold repetition requires three occurrences, and that unplay
/// removes the last one from history.
///
/// Technically: four knight moves restore the initial position, giving the
/// second occurrence; eight give the third. The midpoint assertion is what
/// separates "threefold" from "any repetition". The final unplay proves the
/// history is a stack paired with make/unmake, not an append-only log.
#[test]
fn threefold_repetition_after_knights_cycle() {
    let cycle = [
        ((7, 6), (5, 5)), // Ng1-f3
        ((0, 6), (2, 5)), // Ng8-f6
        ((5, 5), (7, 6)), // Nf3-g1
        ((2, 5), (0, 6)), // Nf6-g8
    ];

    let mut game = Game::new();
    let mut undos = Vec::new();

    for ply in 0..8 {
        let (from, to) = cycle[ply % cycle.len()];
        undos.push(game.play(&mv(from, to)).expect("knight cycle move"));

        if ply == 3 {
            assert_eq!(
                game.state(),
                GameState::Ongoing,
                "two occurrences are not a draw"
            );
        }
    }

    assert!(game.is_threefold_repetition());
    assert_eq!(game.state(), GameState::Draw(DrawReason::ThreefoldRepetition));

    let last = undos.pop().expect("the eighth move must have an undo");
    game.unplay(&last);

    assert!(!game.is_threefold_repetition());
    assert_eq!(
        game.state(),
        GameState::Ongoing,
        "unplay must remove the third occurrence"
    );
}

/// Confirms the repetition key is sensitive to side and castling rights but not
/// to the halfmove clock.
///
/// Technically: FIDE 9.2.2 compares the possible moves. A different side to move
/// or a lost castling right changes those moves; the clock does not. This is a
/// direct key test, independent of the history machinery.
#[test]
fn repetition_key_includes_side_and_castling_but_not_the_clock() {
    let base = Board::new();

    let mut black_to_move = Board::new();
    black_to_move.side_to_move = Color::Black;
    assert_ne!(
        base.position_key(),
        black_to_move.position_key(),
        "side to move must distinguish positions"
    );

    let mut no_kingside = Board::new();
    no_kingside.castling.white_kingside = false;
    assert_ne!(
        base.position_key(),
        no_kingside.position_key(),
        "castling rights must distinguish positions"
    );

    let mut old_clock = Board::new();
    old_clock.halfmove_clock = 99;
    assert_eq!(
        base.position_key(),
        old_clock.position_key(),
        "the halfmove clock must not distinguish positions"
    );
}

/// Confirms an en passant target enters the key only when the capture is legal.
///
/// Technically: FIDE 9.2.2 says positions differ only if a pawn could actually
/// have been captured en passant. The three cases are: a legal capture (must
/// differ), a stale target with no adjacent pawn (must match), and a pinned
/// capture that would expose its own king (must match).
#[test]
fn repetition_key_includes_en_passant_only_when_the_capture_is_legal() {
    // Legal: White pawn e5 can capture the Black pawn on d5 via d6.
    let mut available = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((3, 4), p(Color::White, PieceType::Pawn)),
            ((0, 7), p(Color::Black, PieceType::King)),
            ((3, 3), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    available.en_passant = Some((2, 3));
    let mut available_without_target = available.clone();
    available_without_target.en_passant = None;
    assert_ne!(
        available.position_key(),
        available_without_target.position_key(),
        "a legal en passant capture must distinguish positions"
    );

    // Stale: the target exists, but the only White pawn is on a4.
    let mut stale = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((4, 0), p(Color::White, PieceType::Pawn)),
            ((0, 7), p(Color::Black, PieceType::King)),
            ((3, 3), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    stale.en_passant = Some((2, 3));
    let mut stale_without_target = stale.clone();
    stale_without_target.en_passant = None;
    assert_eq!(
        stale.position_key(),
        stale_without_target.position_key(),
        "an uncapturable target must not split the position"
    );

    // Pinned: b5xc6 would clear the fifth rank and expose the White king on a5
    // to the Black rook on h5, so the capture is not legal.
    let mut pinned = position(
        &[
            ((3, 0), p(Color::White, PieceType::King)),
            ((3, 1), p(Color::White, PieceType::Pawn)),
            ((0, 7), p(Color::Black, PieceType::King)),
            ((3, 7), p(Color::Black, PieceType::Rook)),
            ((3, 2), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    pinned.en_passant = Some((2, 2));
    let mut pinned_without_target = pinned.clone();
    pinned_without_target.en_passant = None;
    assert_eq!(
        pinned.position_key(),
        pinned_without_target.position_key(),
        "a pinned en passant capture must not split the position"
    );
}

/// Confirms the standard insufficient-material cases and their boundaries.
///
/// Technically: the expected results are the common dead-position set, not every
/// FIDE dead position. K+N vs K+N and K+NN vs K are intentionally false because
/// checkmate is possible with the defender's help; blocked-pawn dead positions
/// are out of scope and are documented as a conservative limitation.
#[test]
fn insufficient_material_variants() {
    assert!(
        with_kings(&[]).is_insufficient_material(),
        "K vs K is dead"
    );
    assert!(
        with_kings(&[((7, 2), p(Color::White, PieceType::Bishop))]).is_insufficient_material(),
        "K+B vs K is dead"
    );
    assert!(
        with_kings(&[((7, 1), p(Color::White, PieceType::Knight))]).is_insufficient_material(),
        "K+N vs K is dead"
    );

    assert!(
        !with_kings(&[((7, 0), p(Color::White, PieceType::Rook))]).is_insufficient_material(),
        "K+R vs K can mate"
    );
    assert!(
        !with_kings(&[((7, 3), p(Color::White, PieceType::Queen))]).is_insufficient_material(),
        "K+Q vs K can mate"
    );
    assert!(
        !with_kings(&[((6, 0), p(Color::White, PieceType::Pawn))]).is_insufficient_material(),
        "a pawn can promote"
    );
    assert!(
        !with_kings(&[
            ((7, 1), p(Color::White, PieceType::Knight)),
            ((0, 6), p(Color::Black, PieceType::Knight)),
        ])
        .is_insufficient_material(),
        "K+N vs K+N can mate with cooperation"
    );
    assert!(
        !with_kings(&[
            ((7, 1), p(Color::White, PieceType::Knight)),
            ((7, 6), p(Color::White, PieceType::Knight)),
        ])
        .is_insufficient_material(),
        "K+NN vs K can mate with cooperation"
    );
    assert!(
        !with_kings(&[
            ((7, 2), p(Color::White, PieceType::Bishop)),
            ((0, 6), p(Color::Black, PieceType::Knight)),
        ])
        .is_insufficient_material(),
        "K+B vs K+N can mate with cooperation"
    );

    // c1 and f8 are both on the even (a8) square complex.
    assert!(
        with_kings(&[
            ((7, 2), p(Color::White, PieceType::Bishop)),
            ((0, 5), p(Color::Black, PieceType::Bishop)),
        ])
        .is_insufficient_material(),
        "bishops on one color complex cannot mate"
    );
    // c1 is even, c8 is odd: opposite complexes can cooperate into mate.
    assert!(
        !with_kings(&[
            ((7, 2), p(Color::White, PieceType::Bishop)),
            ((0, 2), p(Color::Black, PieceType::Bishop)),
        ])
        .is_insufficient_material(),
        "opposite-colored bishops are not insufficient"
    );
    // Two bishops for one side, both on the same complex, still cannot mate.
    assert!(
        with_kings(&[
            ((7, 2), p(Color::White, PieceType::Bishop)),
            ((6, 1), p(Color::White, PieceType::Bishop)),
        ])
        .is_insufficient_material(),
        "two same-complex bishops cannot mate"
    );
    // The same pair on opposite complexes can mate.
    assert!(
        !with_kings(&[
            ((7, 2), p(Color::White, PieceType::Bishop)),
            ((7, 5), p(Color::White, PieceType::Bishop)),
        ])
        .is_insufficient_material(),
        "two opposite-complex bishops can mate"
    );
}

/// Confirms insufficient material is reported as a game draw.
///
/// Technically: K+B vs K has legal moves, so the result must come from the
/// material rule rather than from stalemate or checkmate.
#[test]
fn insufficient_material_is_a_game_state() {
    let game = Game::from_board(with_kings(&[
        ((7, 2), p(Color::White, PieceType::Bishop)),
    ]));

    assert_eq!(
        game.state(),
        GameState::Draw(DrawReason::InsufficientMaterial)
    );
}

/// Confirms the shared FEN parser reads the halfmove clock and defaults it.
///
/// Technically: FEN field five is the halfmove clock; field six is the fullmove
/// number and is still ignored. A four-field FEN is accepted for older fixtures.
#[test]
fn fen_parser_reads_the_halfmove_clock() {
    let board = board_from_fen("8/8/8/8/8/8/4K3/7k w - - 73 42");
    assert_eq!(board.halfmove_clock, 73);

    let old_style = board_from_fen("8/8/8/8/8/8/4K3/7k w - -");
    assert_eq!(old_style.halfmove_clock, 0);
}
