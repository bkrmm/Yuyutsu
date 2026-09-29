//! Agreement between the move generator in `moves.rs` and the validator
//! inside `Board::make_move`.
//!
//! These are two independent implementations of the same rules, so the real
//! risk is that they drift. Every test here either checks that they agree
//! (drift) or that make/unmake is a true inverse (round-trip).

use yuyutsu::board::{Board, Color, InvalidMove, Piece, PieceType};
use yuyutsu::moves::{Move, generate_all_moves};

const SEEDS: [u64; 4] = [
    0x2545_F491_4F6C_DD1D,
    0x9E37_79B9_7F4A_7C15,
    0xDEAD_BEEF_CAFE_BABE,
    0x1234_5678_9ABC_DEF0,
];

/// xorshift64. No dependency, deterministic, and a fixed seed means any failure
/// reproduces exactly. Must be seeded nonzero or the state collapses to zero.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

fn pawn_on(color: Color) -> Piece {
    Piece { color, kind: PieceType::Pawn }
}

fn pawn_on_board(from: (usize, usize), color: Color) -> Board {
    let mut b = Board::empty();
    b.grid[from.0][from.1] = Some(pawn_on(color));
    b.side_to_move = color;
    b
}

/// Every generated move must be accepted, and accepted moves must round-trip.
fn checked_moves(board: &Board) -> Vec<Move> {
    let generated = generate_all_moves(board, board.side_to_move);

    for mv in &generated {
        let mut scratch = board.clone();
        match scratch.make_move(mv) {
            Ok(undo) => {
                scratch.unmake_move(&undo);
                assert_eq!(scratch, *board, "round-trip failed for {mv:?}");
            }
            Err(e) => {
                panic!("generator/validator disagree: {mv:?} -> {e:?}")
            }
        }
    }

    generated
}

#[test]
fn start_position_all_moves_accepted() {
    let board = Board::new();
    let white = checked_moves(&board);
    assert_eq!(white.len(), 20, "perft(1) for White must be 20");

    let mut black_view = Board::new();
    black_view.side_to_move = Color::Black;
    checked_moves(&black_view);
}

#[test]
fn random_walk_agrees() {
    let mut checked = 0usize;
    let mut restarts = 0usize;

    for seed in SEEDS {
        let mut rng = Rng(seed);
        let mut board = Board::new();

        for _ in 0..20_000 {
            let generated = checked_moves(&board);
            checked += generated.len();

            // Unrestricted pseudo-legal play strips the board to a bare king
            // boxed in, and that position has no moves. Restart instead of
            // stopping: the goal is to sample many distinct positions, not to
            // finish one game.
            if generated.is_empty() {
                board = Board::new();
                restarts += 1;
                continue;
            }

            let mv = generated[rng.below(generated.len())];
            board
                .make_move(&mv)
                .unwrap_or_else(|e| panic!("seed {seed:#x}: walk took a rejected move {mv:?} -> {e:?}"));
        }
    }

    assert!(checked > 500_000, "only checked {checked} moves across {} seeds", SEEDS.len());
    assert!(restarts > 10, "only {restarts} restarts, walk is exploring one game");
}

#[test]
fn double_push_onto_occupied_landing_square_is_not_generated() {
    // Black pawn on d7, d6 clear, d5 occupied by a black piece. Reachable in
    // real play, and the generator used to emit d7-d5 because the outer
    // occupancy check only cleared the intermediate square.
    let mut b = pawn_on_board((1, 3), Color::Black);
    b.grid[3][3] = Some(Piece { color: Color::Black, kind: PieceType::Bishop });

    let generated = generate_all_moves(&b, Color::Black);
    assert!(
        !generated.iter().any(|m| m.from == (1, 3) && m.to == (3, 3)),
        "generated a double push onto its own piece: {generated:?}"
    );

    let single: Vec<_> = generated.iter().filter(|m| m.from == (1, 3)).copied().collect();
    assert_eq!(single.len(), 1, "d7-d6 is the only pawn move available: {single:?}");
}

#[test]
fn promotion_round_trips() {
    for color in [Color::White, Color::Black] {
        for kind in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
            let from = if color == Color::White { (1, 4) } else { (6, 4) };
            let to = if color == Color::White { (0, 4) } else { (7, 4) };
            let mv = Move { from, to, promotion: Some(kind) };

            let mut b = pawn_on_board(from, color);
            let undo = b.make_move(&mv).expect("legal promotion rejected");
            assert_eq!(b.grid[to.0][to.1], Some(Piece { color, kind }));

            b.unmake_move(&undo);
            assert_eq!(b.grid[from.0][from.1], Some(pawn_on(color)));
            assert_eq!(b.grid[to.0][to.1], None);
            assert_eq!(b.side_to_move, color);
        }
    }
}

#[test]
fn promotion_capture_round_trips() {
    let mut b = pawn_on_board((1, 4), Color::White);
    let captured = Piece { color: Color::Black, kind: PieceType::Rook };
    b.grid[0][5] = Some(captured);

    let mv = Move { from: (1, 4), to: (0, 5), promotion: Some(PieceType::Knight) };
    let undo = b.make_move(&mv).expect("legal promotion-capture rejected");
    assert_eq!(b.grid[0][5], Some(Piece { color: Color::White, kind: PieceType::Knight }));

    b.unmake_move(&undo);
    assert_eq!(b.grid[0][5], Some(captured));
    assert_eq!(b.grid[1][4], Some(pawn_on(Color::White)));
    assert_eq!(b.side_to_move, Color::White);
}

#[test]
fn promotion_coherence_rejected_atomically() {
    let pristine = pawn_on_board((1, 4), Color::White);

    let cases = [
        (Move { from: (1, 4), to: (0, 4), promotion: None }, InvalidMove::MustPromote),
        (
            Move { from: (1, 4), to: (0, 4), promotion: Some(PieceType::Pawn) },
            InvalidMove::IllegalPromotionPiece,
        ),
        (
            Move { from: (1, 4), to: (0, 4), promotion: Some(PieceType::King) },
            InvalidMove::IllegalPromotionPiece,
        ),
    ];

    for (mv, expected) in cases {
        let mut b = pristine.clone();
        assert_eq!(b.make_move(&mv).unwrap_err(), expected, "for {mv:?}");
        assert_eq!(b, pristine, "rejection mutated the board for {mv:?}");
    }
}

#[test]
fn pawn_on_back_rank_does_not_panic() {
    // Unreachable through play now that promotions apply, but a hand-built
    // position creates it instantly. perft never visits these, so it needs
    // its own test.
    let b = pawn_on_board((0, 4), Color::White);
    let _ = generate_all_moves(&b, Color::White);
}
