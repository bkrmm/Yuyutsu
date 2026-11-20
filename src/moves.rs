//! Pseudo-legal move generation on the bitboard board.
//!
//! Every generator here answers "which moves does this piece have if king safety
//! is ignored?". `Board::generate_legal_moves` filters the result by playing each
//! candidate and asking whether the mover's king is attacked. Keeping that split
//! means this layer stays free of recursion and of the rules that depend on the
//! resulting position.
//!
//! Each public generator returns a fresh `Vec`; the matching private `_into`
//! function appends to an existing buffer. `generate_all_moves` uses the `_into`
//! form, so one move list costs one allocation instead of seven.

use crate::board::{
    Board, Color, Piece, PieceType, bishop_attacks, king_attacks, knight_attacks,
    pawn_attacks, queen_attacks, rook_attacks, square_index, square_of,
};

/// A single move, independent of the board it was generated from.
///
/// Technically: `from` and `to` are `(row, column)` pairs on the 8x8 grid, and
/// `promotion` is `Some` only when a pawn lands on the far rank. `Copy` keeps it
/// cheap to pass around a search; `PartialEq`/`Eq` let tests assert on moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub promotion : Option<PieceType>, //none for non-pawn moves
}

/// Dispatches to the generator matching this piece's kind.
///
/// Technically: the moving color is read from `piece.color` rather than taken as
/// a separate argument, so a move list can never disagree with the piece it was
/// generated from. This convenience wrapper allocates; the hot path uses
/// `generate_pseudo_moves_into`.
pub fn generate_pseudo_moves(board: &Board, row: usize, column: usize, piece: Piece) -> Vec<Move> {
    let mut moves = Vec::with_capacity(8);
    generate_pseudo_moves_into(board, row, column, piece, &mut moves);
    moves
}

/// Appends the moves of one piece to an existing buffer.
///
/// Technically: the allocation-free form of `generate_pseudo_moves`. Every
/// per-piece generator below has the same shape, which is what lets
/// `generate_all_moves` build one list for the whole side.
fn generate_pseudo_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    piece: Piece,
    moves: &mut Vec<Move>,
) {
    match piece.kind {
        PieceType::Pawn => generate_pawn_moves_into(board, row, column, piece.color, moves),
        PieceType::Knight => generate_knight_moves_into(board, row, column, piece.color, moves),
        PieceType::Bishop => generate_bishop_moves_into(board, row, column, piece.color, moves),
        PieceType::Rook => generate_rook_moves_into(board, row, column, piece.color, moves),
        PieceType::King => generate_king_moves_into(board, row, column, piece.color, moves),
        PieceType::Queen => generate_queen_moves_into(board, row, column, piece.color, moves),
    }
}

/// Generates every pseudo-legal move for `color`.
///
/// Technically: walks the six piece bitboards in kind order and every set bit
/// within each, lowest square first. Bit order is row-major, so the resulting
/// list is ordered by piece kind and then by board position. The order is an
/// implementation detail -- never index into this list expecting a particular
/// move -- but it is stable for a fixed position, which makes failure output
/// reproducible.
pub fn generate_all_moves(board: &Board, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(48);

    for kind in [
        PieceType::Pawn,
        PieceType::Knight,
        PieceType::Bishop,
        PieceType::Rook,
        PieceType::Queen,
        PieceType::King,
    ] {
        let mut bits = board.pieces_of(color, kind);
        while bits != 0 {
            let square = square_of(bits.trailing_zeros() as usize);
            bits &= bits - 1;

            let piece = Piece { color, kind };
            generate_pseudo_moves_into(board, square.0, square.1, piece, &mut moves);
        }
    }

    moves
}

/// Generates pseudo-legal pawn moves: single push, double push, and captures.
///
/// Technically: `forward` is -1 for White because row 0 is rank 8, so advancing
/// decreases the row. The single push requires an empty landing square and the
/// double push additionally requires the pawn to sit on its home rank and the
/// intermediate square to be empty -- two separate tests, because checking only
/// the landing square lets a pawn jump over a blocker.
///
/// Captures are found with the pawn attack table intersected with the enemy
/// occupancy, which enumerates every diagonal target in one operation. En passant
/// is a third case: the target square is empty, so it cannot appear in the enemy
/// occupancy and must be added from `board.en_passant` explicitly.
pub fn generate_pawn_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(4);
    generate_pawn_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one pawn's moves to `moves`.
///
/// Technically: the body behind `generate_pawn_moves`. Push and capture cases are
/// kept separate because a push may not capture and a capture may not land on an
/// empty square -- except for en passant, which is the one rule that breaks that
/// symmetry and therefore gets its own branch.
fn generate_pawn_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let forward: isize = if color == Color::White { -1 } else { 1 };
    let promo_row: isize = if color == Color::White { 0 } else { 7 };
    let home_row: isize = if color == Color::White { 6 } else { 1 };

    // Single push, and the double push that can only follow it.
    let single_row = row as isize + forward;
    if single_row >= 0 && single_row < 8 {
        let single = (single_row as usize, column);
        if board.piece_at(single).is_none() {
            if single_row == promo_row {
                for promo in PieceType::PROMOTIONS {
                    moves.push(Move { from, to: single, promotion: Some(promo) });
                }
            } else {
                moves.push(Move { from, to: single, promotion: None });

                let double_row = row as isize + 2 * forward;
                if row as isize == home_row && double_row >= 0 && double_row < 8 {
                    let double = (double_row as usize, column);
                    if board.piece_at(double).is_none() {
                        moves.push(Move { from, to: double, promotion: None });
                    }
                }
            }
        }
    }

    // Diagonal captures, including promotion captures.
    let mut captures = pawn_attacks(color, from) & board.occupied_by(color.opponent());
    while captures != 0 {
        let to = square_of(captures.trailing_zeros() as usize);
        captures &= captures - 1;

        if to.0 as isize == promo_row {
            for promo in PieceType::PROMOTIONS {
                moves.push(Move { from, to, promotion: Some(promo) });
            }
        } else {
            moves.push(Move { from, to, promotion: None });
        }
    }

    // En passant: the only capture whose destination is empty. The pawn attack
    // set is still the right test -- it names the square the capture would land
    // on -- but the target must be the recorded one.
    if let Some(target) = board.en_passant {
        let target_bit = 1u64 << square_index(target);
        if pawn_attacks(color, from) & target_bit != 0 {
            moves.push(Move { from, to: target, promotion: None });
        }
    }
}

/// Generates pseudo-legal knight moves from one square.
///
/// Technically: a table lookup for the eight jump targets, minus friendly
/// occupied squares. Knights jump, so occupancy never blocks them; only the
/// destination matters.
pub fn generate_knight_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(8);
    generate_knight_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one knight's moves to `moves`.
///
/// Technically: one table lookup and one mask. There is no path to clear and no
/// direction to walk, which is why knight generation is the cheapest in the
/// engine.
fn generate_knight_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let targets = knight_attacks(from) & !board.occupied_by(color);
    append_targets(moves, from, targets);
}

/// Generates pseudo-legal bishop moves along the four diagonals.
///
/// Technically: `bishop_attacks` already stops each ray at the first occupied
/// square and includes that square. Masking off friendly pieces therefore leaves
/// empty squares plus enemy captures, which is exactly the bishop's move set.
pub fn generate_bishop_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(8);
    generate_bishop_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one bishop's moves to `moves`.
///
/// Technically: four diagonal rays against the cached occupancy, with friendly
/// destinations removed. The blocker square survives the ray computation so that
/// an enemy piece there becomes a capture.
fn generate_bishop_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let targets = bishop_attacks(from, board.occupied()) & !board.occupied_by(color);
    append_targets(moves, from, targets);
}

/// Generates pseudo-legal rook moves along the four orthogonals.
///
/// Technically: the same ray-stop rule as the bishop with the orthogonal ray
/// table. A friendly blocker terminates the ray and is masked out; an enemy
/// blocker terminates the ray and remains as a capture.
pub fn generate_rook_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(8);
    generate_rook_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one rook's moves to `moves`.
///
/// Technically: four orthogonal rays against the cached occupancy. The only
/// difference from the bishop is the half of the ray table being read.
fn generate_rook_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let targets = rook_attacks(from, board.occupied()) & !board.occupied_by(color);
    append_targets(moves, from, targets);
}

/// Generates pseudo-legal queen moves.
///
/// Technically: a queen is exactly a rook plus a bishop, so this ORs the two
/// attack sets instead of duplicating the ray-walking code. Any future fix to
/// ray handling lands in one place.
pub fn generate_queen_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(16);
    generate_queen_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one queen's moves to `moves`.
///
/// Technically: one occupancy word feeds both the orthogonal and diagonal ray
/// sets, so the union costs eight ray computations rather than two passes over
/// the board.
fn generate_queen_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let targets = queen_attacks(from, board.occupied()) & !board.occupied_by(color);
    append_targets(moves, from, targets);
}

/// Generates pseudo-legal king moves from one square, including castling.
///
/// Technically: a single-step table lookup minus friendly squares, plus castling
/// when it is structurally possible -- rights intact, rook still on its corner,
/// and every square between king and rook empty. Castling is emitted as an
/// ordinary `Move` from e1 to g1 (or c1), because a two-file king move is
/// unambiguous.
///
/// Two rules are deliberately *not* checked here: the king may not castle out of
/// check, and may not cross an attacked square. Those belong to
/// `Board::generate_legal_moves`, because this generator must stay free of
/// king-safety logic -- if it filtered for safety, the legality filter would
/// have nothing left to do and could never be tested independently.
pub fn generate_king_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = Vec::with_capacity(8);
    generate_king_moves_into(board, row, column, color, &mut moves);
    moves
}

/// Appends one king's moves to `moves`, including castling.
///
/// Technically: the step table gives the ordinary moves; the castling branches
/// then test rights, rook presence, and the gap by square rather than by ray,
/// because the rook's home corner is not derivable from the king's target.
fn generate_king_moves_into(
    board: &Board,
    row: usize,
    column: usize,
    color: Color,
    moves: &mut Vec<Move>,
) {
    let from = (row, column);
    let targets = king_attacks(from) & !board.occupied_by(color);
    append_targets(moves, from, targets);

    // Castling is only possible from the king's home square.
    let home_row = if color == Color::White { 7 } else { 0 };
    if row != home_row || column != 4 {
        return;
    }

    // A right is only meaningful if the rook it refers to is actually there.
    // Rights are revoked when the rook leaves or is captured, so this is a
    // defensive check for hand-built positions rather than a normal case.
    let rook_home = |col: usize| {
        matches!(
            board.piece_at((home_row, col)),
            Some(piece) if piece.color == color && piece.kind == PieceType::Rook
        )
    };
    let empty = |col: usize| board.piece_at((home_row, col)).is_none();

    // King side: f1 and g1 must be clear.
    if board.castling.kingside(color) && rook_home(7) && empty(5) && empty(6) {
        moves.push(Move { from, to: (home_row, 6), promotion: None });
    }

    // Queen side: b1, c1 and d1 must be clear. b1 is easy to forget -- it is
    // empty in the diagram but the rook passes through it.
    if board.castling.queenside(color) && rook_home(0) && empty(1) && empty(2) && empty(3) {
        moves.push(Move { from, to: (home_row, 2), promotion: None });
    }
}

/// Appends one `Move` per set bit in `targets` to `moves`.
///
/// Technically: iterates set bits with the `bits &= bits - 1` idiom, which
/// clears the lowest set bit each step, so the loop runs exactly once per target
/// and needs no index counter. Targets emerge in ascending square order. The
/// promotion slot is always `None` because no step-target piece promotes.
fn append_targets(moves: &mut Vec<Move>, from: (usize, usize), mut targets: u64) {
    while targets != 0 {
        let to = square_of(targets.trailing_zeros() as usize);
        targets &= targets - 1;
        moves.push(Move { from, to, promotion: None });
    }
}
