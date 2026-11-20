//! Targeted tests for castling and en passant.
//!
//! Perft proves the rules are *wired together* correctly, but a single missed
//! edge case can hide inside a six-figure node count. These tests isolate each
//! rule so a failure names the broken condition directly: rights, occupied
//! paths, the three king-safety squares, rook movement, en passant bookkeeping,
//! and the discovered-check capture that makes en passant special.

mod common;

use common::{board_from_fen, piece as p, position};
use yuyutsu::board::{CastlingRights, Color, InvalidMove, PieceType, in_check};
use yuyutsu::moves::{Move, generate_all_moves};

/// Builds a non-promotion move between two squares.
///
/// Technically: every move in this file is an ordinary move, so fixing
/// `promotion` to `None` removes six lines of noise from each assertion.
fn mv(from: (usize, usize), to: (usize, usize)) -> Move {
    Move { from, to, promotion: None }
}

/// Returns whether a move list contains a move between the two squares.
///
/// Technically: compares endpoints only, ignoring promotion. That is exact for
/// castling and en passant, neither of which can promote, and keeps the tests
/// from constructing a `Move` just to ask a question.
fn has(moves: &[Move], from: (usize, usize), to: (usize, usize)) -> bool {
    moves.iter().any(|m| m.from == from && m.to == to)
}

/// Confirms both White castles are legal in Kiwipete.
///
/// Technically: Kiwipete is the standard castling oracle. Both back ranks are
/// clear between king and rooks, all four rights are live, and the position is
/// not in check, so the legal list must contain e1g1 and e1c1. A depth-1 count
/// of 48 is not enough on its own -- one missing and one spurious move would
/// cancel out.
#[test]
fn both_castles_are_legal_in_kiwipete() {
    let kiwipete = "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -";
    let mut board = board_from_fen(kiwipete);

    assert!(!in_check(&board, Color::White), "Kiwipete must not start in check");

    let legal = board.generate_legal_moves();

    assert!(has(&legal, (7, 4), (7, 6)), "White lost the kingside castle: {legal:?}");
    assert!(has(&legal, (7, 4), (7, 2)), "White lost the queenside castle: {legal:?}");
}

/// Confirms an occupied path prevents castling on both sides.
///
/// Technically: kingside requires f1 and g1 empty; queenside requires b1, c1 and
/// d1. The b1 square is the trap -- the king never crosses it, but the rook
/// does, so a generator that checks only c1/d1 would wrongly allow the move.
/// The direct `make_move` assertions also prove the validator, not just the
/// generator, rejects the same castles.
#[test]
fn castling_needs_an_empty_path() {
    // Kingside: a White bishop on f1 blocks the king's transit square.
    let mut kingside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((7, 5), p(Color::White, PieceType::Bishop)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    kingside.castling.white_kingside = true;

    assert!(
        !has(&kingside.generate_legal_moves(), (7, 4), (7, 6)),
        "castled through an occupied f1"
    );
    let before = kingside.clone();
    assert_eq!(
        kingside.make_move(&mv((7, 4), (7, 6))),
        Err(InvalidMove::PathBlocked)
    );
    assert_eq!(kingside, before);

    // Queenside: a White knight on b1 blocks the rook's path, not the king's.
    let mut queenside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((7, 1), p(Color::White, PieceType::Knight)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    queenside.castling.white_queenside = true;

    assert!(
        !has(&queenside.generate_legal_moves(), (7, 4), (7, 2)),
        "castled through an occupied b1"
    );
    let before = queenside.clone();
    assert_eq!(
        queenside.make_move(&mv((7, 4), (7, 2))),
        Err(InvalidMove::PathBlocked)
    );
    assert_eq!(queenside, before);
}

/// Confirms a king cannot castle out of check.
///
/// Technically: the pseudo-legal generator still emits the castle, because
/// safety is deliberately not its job. The legality filter must remove it. The
/// pseudo-legal assertion is what distinguishes "the filter rejected it" from
/// "the generator never offered it".
#[test]
fn castling_out_of_check_is_forbidden() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 4), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    board.castling.white_kingside = true;

    assert!(in_check(&board, Color::White), "the e-file must give check");
    assert!(
        has(&generate_all_moves(&board, Color::White), (7, 4), (7, 6)),
        "the pseudo-legal generator should offer the castle"
    );
    assert!(
        !has(&board.generate_legal_moves(), (7, 4), (7, 6)),
        "castled while in check"
    );
}

/// Confirms a king cannot castle across an attacked transit square.
///
/// Technically: after the move the king is no longer on the transit square, so
/// "make the move, then ask if the king is attacked" cannot detect this. Both
/// sides are tested because the transit square differs: f1 for kingside, d1 for
/// queenside. In each position the king is not in check and the destination is
/// not attacked, isolating the transit rule.
#[test]
fn castling_through_an_attacked_square_is_forbidden() {
    // Kingside: a Black rook on f8 attacks f1, the square the king crosses.
    let mut kingside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 5), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    kingside.castling.white_kingside = true;

    assert!(!in_check(&kingside, Color::White));
    assert!(
        has(&generate_all_moves(&kingside, Color::White), (7, 4), (7, 6)),
        "the pseudo-legal generator should offer the kingside castle"
    );
    assert!(
        !has(&kingside.generate_legal_moves(), (7, 4), (7, 6)),
        "castled across an attacked f1"
    );

    // Queenside: a Black rook on d8 attacks d1, the queenside transit square.
    let mut queenside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((0, 3), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    queenside.castling.white_queenside = true;

    assert!(!in_check(&queenside, Color::White));
    assert!(
        has(&generate_all_moves(&queenside, Color::White), (7, 4), (7, 2)),
        "the pseudo-legal generator should offer the queenside castle"
    );
    assert!(
        !has(&queenside.generate_legal_moves(), (7, 4), (7, 2)),
        "castled across an attacked d1"
    );
}

/// Confirms a king cannot castle onto an attacked destination square.
///
/// Technically: this rule is enforced by the generic filter -- make the move,
/// then ask whether the king is attacked -- rather than by
/// `castling_is_forbidden`, because the destination is visible after the move.
/// A rook on g8 attacks g1 without attacking e1 or f1, so the castle must be
/// removed for the destination alone.
#[test]
fn castling_into_check_is_forbidden() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 6), p(Color::Black, PieceType::Rook)),
            ((0, 0), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    board.castling.white_kingside = true;

    assert!(!in_check(&board, Color::White));
    assert!(
        has(&generate_all_moves(&board, Color::White), (7, 4), (7, 6)),
        "the pseudo-legal generator should offer the castle"
    );
    assert!(
        !has(&board.generate_legal_moves(), (7, 4), (7, 6)),
        "castled onto an attacked g1"
    );
}

/// Confirms castling moves the rook too and that unmake reverses both pieces.
///
/// Technically: castling is the only move that relocates two pieces, so it is
/// the only move where a piece-placement bug can leave a spare rook behind.
/// Both directions are covered, and the full-board comparison after unmake
/// checks castling rights and the en passant field as well as the pieces.
#[test]
fn castling_moves_the_rook_and_round_trips() {
    // Kingside: king e1 -> g1, rook h1 -> f1.
    let mut kingside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    kingside.castling.white_kingside = true;
    let before = kingside.clone();

    let undo = kingside.make_move(&mv((7, 4), (7, 6))).expect("kingside castle");

    assert_eq!(kingside.piece_at((7, 6)), Some(p(Color::White, PieceType::King)));
    assert_eq!(kingside.piece_at((7, 5)), Some(p(Color::White, PieceType::Rook)));
    assert_eq!(kingside.piece_at((7, 4)), None, "king left e1");
    assert_eq!(kingside.piece_at((7, 7)), None, "rook left h1");
    assert!(!kingside.castling.white_kingside && !kingside.castling.white_queenside);

    kingside.unmake_move(&undo);
    assert_eq!(kingside, before, "kingside unmake did not restore the position");

    // Queenside: king e1 -> c1, rook a1 -> d1.
    let mut queenside = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    queenside.castling.white_queenside = true;
    let before = queenside.clone();

    let undo = queenside.make_move(&mv((7, 4), (7, 2))).expect("queenside castle");

    assert_eq!(queenside.piece_at((7, 2)), Some(p(Color::White, PieceType::King)));
    assert_eq!(queenside.piece_at((7, 3)), Some(p(Color::White, PieceType::Rook)));
    assert_eq!(queenside.piece_at((7, 4)), None, "king left e1");
    assert_eq!(queenside.piece_at((7, 0)), None, "rook left a1");

    queenside.unmake_move(&undo);
    assert_eq!(queenside, before, "queenside unmake did not restore the position");
}

/// Confirms moving the king permanently revokes both of its castling rights.
///
/// Technically: a king can never regain the right to castle, even if it returns
/// to e1. The rights move is tested together with the physical move, and the
/// other side's rights are asserted untouched to catch an over-broad clear.
#[test]
fn moving_the_king_revokes_both_castling_rights() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 6), p(Color::Black, PieceType::King)),
            ((0, 0), p(Color::Black, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::Rook)),
        ],
        Color::White,
    );
    board.castling = CastlingRights::ALL;
    let before = board.clone();

    let undo = board.make_move(&mv((7, 4), (6, 4))).expect("king step");

    assert!(!board.castling.white_kingside, "White retained kingside rights");
    assert!(!board.castling.white_queenside, "White retained queenside rights");
    assert!(board.castling.black_kingside, "Black lost kingside rights");
    assert!(board.castling.black_queenside, "Black lost queenside rights");

    board.unmake_move(&undo);
    assert_eq!(board, before, "unmake did not restore the rights");
}

/// Confirms moving a rook revokes only the right attached to that corner.
///
/// Technically: a1 owns the queenside right and h1 owns the kingside right, so
/// moving one must leave the other live. Both directions are tested on the same
/// board, unwinding between them so each assertion starts from a known state.
#[test]
fn moving_a_rook_revokes_only_that_side() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Rook)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 6), p(Color::Black, PieceType::King)),
            ((0, 0), p(Color::Black, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::Rook)),
        ],
        Color::White,
    );
    board.castling = CastlingRights::ALL;
    let before = board.clone();

    let undo = board.make_move(&mv((7, 0), (6, 0))).expect("a1 -> a2");
    assert!(!board.castling.white_queenside, "a1 move left queenside rights alive");
    assert!(board.castling.white_kingside, "a1 move wrongly cleared kingside rights");
    board.unmake_move(&undo);
    assert_eq!(board, before);

    let undo = board.make_move(&mv((7, 7), (6, 7))).expect("h1 -> h2");
    assert!(!board.castling.white_kingside, "h1 move left kingside rights alive");
    assert!(board.castling.white_queenside, "h1 move wrongly cleared queenside rights");
    board.unmake_move(&undo);
    assert_eq!(board, before);
}

/// Confirms capturing a rook on its home square revokes the captured side's right.
///
/// Technically: the right belongs to the corner, not the piece, so it must
/// disappear when that corner's rook is captured -- even though the capturing
/// piece did not start on any home square. A White queen captures on a8 so the
/// capture is the only possible cause; the untouched h8 rook keeps Black's
/// kingside right alive as a control.
#[test]
fn capturing_a_home_rook_revokes_that_right() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 0), p(Color::White, PieceType::Queen)),
            ((0, 4), p(Color::Black, PieceType::King)),
            ((0, 0), p(Color::Black, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::Rook)),
        ],
        Color::White,
    );
    board.castling.black_kingside = true;
    board.castling.black_queenside = true;
    let before = board.clone();

    let undo = board.make_move(&mv((7, 0), (0, 0))).expect("Qa1xa8");

    assert_eq!(undo.captured, Some(p(Color::Black, PieceType::Rook)));
    assert_eq!(undo.captured_square, (0, 0));
    assert!(!board.castling.black_queenside, "captured a8 rook left queenside rights");
    assert!(board.castling.black_kingside, "a8 capture wrongly cleared kingside rights");

    board.unmake_move(&undo);
    assert_eq!(board, before, "unmake did not restore the captured rook or rights");
}

/// Confirms `make_move` rejects a castle that lacks a right or a rook.
///
/// Technically: the generator only offers structurally possible castles, so
/// these conditions would otherwise be exercised only through hand-built
/// `Move`s -- exactly what a search or a UCI front end can produce. Both
/// failure modes return `CastlingUnavailable`, and both must leave the board
/// untouched.
#[test]
fn castling_requires_a_right_and_a_rook() {
    // Rook present, right absent.
    let mut no_right = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((7, 7), p(Color::White, PieceType::Rook)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    let before = no_right.clone();
    assert_eq!(
        no_right.make_move(&mv((7, 4), (7, 6))),
        Err(InvalidMove::CastlingUnavailable)
    );
    assert_eq!(no_right, before);

    // Right present, rook absent.
    let mut no_rook = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((0, 7), p(Color::Black, PieceType::King)),
        ],
        Color::White,
    );
    no_rook.castling.white_kingside = true;
    let before = no_rook.clone();
    assert_eq!(
        no_rook.make_move(&mv((7, 4), (7, 6))),
        Err(InvalidMove::CastlingUnavailable)
    );
    assert_eq!(no_rook, before);
}

/// Confirms a double push sets the en passant target and any other move clears it.
///
/// Technically: the target is the skipped square, not the pawn's landing square
/// -- e3 after e2e4 and d6 after d7d5. The sequence also proves the target
/// expires immediately: a Black single push clears White's target, and a White
/// king move clears Black's. Unwinding in reverse checks that `Undo` restores
/// the field at every ply.
#[test]
fn double_push_sets_en_passant_and_other_moves_clear_it() {
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((6, 4), p(Color::White, PieceType::Pawn)),
            ((6, 3), p(Color::White, PieceType::Pawn)),
            ((0, 4), p(Color::Black, PieceType::King)),
            ((1, 0), p(Color::Black, PieceType::Pawn)),
            ((1, 3), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    let before = board.clone();

    let u1 = board.make_move(&mv((6, 4), (4, 4))).expect("e2e4");
    assert_eq!(board.en_passant, Some((5, 4)), "e2e4 must target e3");

    let u2 = board.make_move(&mv((1, 0), (2, 0))).expect("a7a6");
    assert_eq!(board.en_passant, None, "a single push must clear the target");

    let u3 = board.make_move(&mv((6, 3), (5, 3))).expect("d2d3");
    assert_eq!(board.en_passant, None, "a non-double push must not set a target");

    let u4 = board.make_move(&mv((1, 3), (3, 3))).expect("d7d5");
    assert_eq!(board.en_passant, Some((2, 3)), "d7d5 must target d6");

    let u5 = board.make_move(&mv((7, 4), (7, 5))).expect("Kf1");
    assert_eq!(board.en_passant, None, "a king move must clear the target");

    board.unmake_move(&u5);
    assert_eq!(board.en_passant, Some((2, 3)));
    board.unmake_move(&u4);
    assert_eq!(board.en_passant, None);
    board.unmake_move(&u3);
    board.unmake_move(&u2);
    board.unmake_move(&u1);
    assert_eq!(board, before, "unmake did not restore the en passant field");
}

/// Confirms an en passant capture removes the pawn beside the mover.
///
/// Technically: the captured pawn is not on the destination square -- it stands
/// on the mover's original rank -- which is why `Undo` needs
/// `captured_square`. Both colors are tested because the geometry mirrors, and
/// the full-board comparison after unmake proves the removed pawn is restored
/// exactly where it stood.
#[test]
fn en_passant_capture_removes_the_pawn_beside_the_mover() {
    // White pawn e5 captures Black's d5 pawn, landing on the empty d6 square.
    let mut white = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((3, 4), p(Color::White, PieceType::Pawn)),
            ((0, 4), p(Color::Black, PieceType::King)),
            ((3, 3), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    white.en_passant = Some((2, 3));
    let before = white.clone();

    let capture = mv((3, 4), (2, 3));
    assert!(
        has(&white.generate_legal_moves(), (3, 4), (2, 3)),
        "White's en passant capture was not generated"
    );
    let undo = white.make_move(&capture).expect("White en passant");

    assert_eq!(undo.captured, Some(p(Color::Black, PieceType::Pawn)));
    assert_eq!(undo.captured_square, (3, 3), "capture must be recorded on d5");
    assert_eq!(white.piece_at((3, 3)), None, "the d5 pawn must be removed");
    assert_eq!(white.piece_at((3, 4)), None, "the e5 pawn must leave its square");
    assert_eq!(white.piece_at((2, 3)), Some(p(Color::White, PieceType::Pawn)));

    white.unmake_move(&undo);
    assert_eq!(white, before, "White en passant unmake did not restore the position");

    // Black pawn d4 captures White's e4 pawn, landing on the empty e3 square.
    let mut black = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((0, 4), p(Color::Black, PieceType::King)),
            ((4, 3), p(Color::Black, PieceType::Pawn)),
            ((4, 4), p(Color::White, PieceType::Pawn)),
        ],
        Color::Black,
    );
    black.en_passant = Some((5, 4));
    let before = black.clone();

    let capture = mv((4, 3), (5, 4));
    assert!(
        has(&black.generate_legal_moves(), (4, 3), (5, 4)),
        "Black's en passant capture was not generated"
    );
    let undo = black.make_move(&capture).expect("Black en passant");

    assert_eq!(undo.captured, Some(p(Color::White, PieceType::Pawn)));
    assert_eq!(undo.captured_square, (4, 4), "capture must be recorded on e4");
    assert_eq!(black.piece_at((4, 4)), None, "the e4 pawn must be removed");
    assert_eq!(black.piece_at((4, 3)), None, "the d4 pawn must leave its square");
    assert_eq!(black.piece_at((5, 4)), Some(p(Color::Black, PieceType::Pawn)));

    black.unmake_move(&undo);
    assert_eq!(black, before, "Black en passant unmake did not restore the position");
}

/// Confirms en passant is rejected without a live target or a capturable pawn.
///
/// Technically: an empty diagonal square is not enough. The board must name that
/// exact square as the en passant target *and* the pawn that would be removed
/// must actually be beside the mover. Both conditions are checked separately so
/// a stale target cannot manufacture a phantom capture.
#[test]
fn en_passant_requires_a_live_target_and_a_capturable_pawn() {
    // No target: the generator must not even offer the diagonal move.
    let mut board = position(
        &[
            ((7, 4), p(Color::White, PieceType::King)),
            ((3, 4), p(Color::White, PieceType::Pawn)),
            ((0, 4), p(Color::Black, PieceType::King)),
            ((3, 3), p(Color::Black, PieceType::Pawn)),
        ],
        Color::White,
    );
    let before = board.clone();
    let capture = mv((3, 4), (2, 3));

    assert!(
        !has(&generate_all_moves(&board, Color::White), (3, 4), (2, 3)),
        "generator emitted en passant without a target"
    );
    assert_eq!(board.make_move(&capture), Err(InvalidMove::EnPassantUnavailable));
    assert_eq!(board, before);

    // Target present, but no pawn on the captured square.
    board.en_passant = Some((2, 3));
    board.set_piece((3, 3), None);
    let before = board.clone();

    assert_eq!(board.make_move(&capture), Err(InvalidMove::EnPassantUnavailable));
    assert_eq!(board, before, "a rejected en passant must not mutate the board");
}

/// Confirms an en passant capture cannot expose its own king to a rook.
///
/// Technically: White Ka5 and Pb5 face Black Rh5 with Black's c5 pawn between
/// them. Capturing c5 en passant removes the Black pawn *and* vacates b5, so
/// the b-file is irrelevant and the fifth rank becomes clear -- exposing the
/// king to the rook. The move is pseudo-legal and geometrically valid;
/// `make_move` accepts it because safety is not its job, the legality filter
/// rejects it, and making it manually proves the exposure is real.
#[test]
fn en_passant_cannot_expose_the_king_to_a_rook() {
    let mut board = position(
        &[
            ((3, 0), p(Color::White, PieceType::King)),  // a5
            ((3, 1), p(Color::White, PieceType::Pawn)),  // b5
            ((0, 7), p(Color::Black, PieceType::King)),  // h8
            ((3, 7), p(Color::Black, PieceType::Rook)),  // h5
            ((3, 2), p(Color::Black, PieceType::Pawn)),  // c5
        ],
        Color::White,
    );
    board.en_passant = Some((2, 2)); // c6, after c7c5
    let before = board.clone();
    let capture = mv((3, 1), (2, 2));

    assert!(!in_check(&board, Color::White), "the two pawns block the rook");
    assert!(
        has(&generate_all_moves(&board, Color::White), (3, 1), (2, 2)),
        "the en passant capture should be pseudo-legal"
    );
    assert!(
        !has(&board.generate_legal_moves(), (3, 1), (2, 2)),
        "the filter allowed a capture that exposes the king"
    );

    // Prove the rejection is correct: physically play it and observe the check.
    let undo = board.make_move(&capture).expect("the move is geometrically valid");
    assert!(
        in_check(&board, Color::White),
        "removing both pawns should open the fifth rank"
    );
    board.unmake_move(&undo);
    assert_eq!(board, before);
}
