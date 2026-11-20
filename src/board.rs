use std::fmt;

use crate::moves::Move;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Color { White, Black }

impl Color {
    /// Returns the opposite color.
    ///
    /// Technically: a two-variant exhaustive match, so it is total and cannot
    /// fail. Used to swap `side_to_move` after a move and to name the attacking
    /// side when asking whether a square is defended.
    pub fn opponent(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }

    /// Returns the zero-based index used to address per-color tables.
    ///
    /// Technically: the piece arrays are `[[u64; 6]; 2]`, so White is row 0 and
    /// Black is row 1. Keeping the mapping in one function means a table cannot
    /// silently swap sides.
    pub fn index(self) -> usize {
        match self {
            Color::White => 0,
            Color::Black => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PieceType { Pawn, Knight, Bishop, Rook, Queen, King }

impl PieceType {
    /// Returns the zero-based index used to address per-kind tables.
    ///
    /// Technically: the order is pawn, knight, bishop, rook, queen, king, which
    /// is also the order pawns are promoted through. It must match the layout of
    /// `Board::pieces` and nothing else -- no persisted format depends on it.
    pub fn index(self) -> usize {
        match self {
            PieceType::Pawn => 0,
            PieceType::Knight => 1,
            PieceType::Bishop => 2,
            PieceType::Rook => 3,
            PieceType::Queen => 4,
            PieceType::King => 5,
        }
    }

    /// Returns every kind a pawn may promote into.
    ///
    /// Technically: the order is the one the generator emits, so a promotion
    /// move list is stable. The king and pawn are excluded because the rules
    /// forbid them, not because the board cannot represent them.
    pub const PROMOTIONS: [PieceType; 4] = [
        PieceType::Queen,
        PieceType::Rook,
        PieceType::Bishop,
        PieceType::Knight,
    ];
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceType,
}

/// Which castling rights are still available.
///
/// Technically: four independent flags rather than a bit field, because a hobby
/// engine values debuggability over four bytes. A right disappears permanently
/// once the corresponding king or rook moves, or once that rook is captured --
/// there is no way to regain one. The whole struct is copied into `Undo` so
/// unmake can restore the exact previous state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct CastlingRights {
    pub white_kingside: bool,
    pub white_queenside: bool,
    pub black_kingside: bool,
    pub black_queenside: bool,
}

impl CastlingRights {
    /// No side may castle. The correct value for an empty board.
    pub const NONE: CastlingRights = CastlingRights {
        white_kingside: false,
        white_queenside: false,
        black_kingside: false,
        black_queenside: false,
    };

    /// Every side may castle. The correct value for the starting position.
    pub const ALL: CastlingRights = CastlingRights {
        white_kingside: true,
        white_queenside: true,
        black_kingside: true,
        black_queenside: true,
    };

    /// Returns whether `color` may castle toward the h-file.
    ///
    /// Technically: a match over two variants, resolving the color-specific flag
    /// so callers never index a table or write a `match` of their own.
    pub fn kingside(self, color: Color) -> bool {
        match color {
            Color::White => self.white_kingside,
            Color::Black => self.black_kingside,
        }
    }

    /// Returns whether `color` may castle toward the a-file.
    ///
    /// Technically: the queenside mirror of `kingside`, kept separate so callers
    /// cannot accidentally swap the two flags.
    pub fn queenside(self, color: Color) -> bool {
        match color {
            Color::White => self.white_queenside,
            Color::Black => self.black_queenside,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidMove {
    OutOfBounds,
    NoPieceAtFrom,
    WrongColor,
    OwnPieceAtTo,
    MustPromote,
    CannotPromote,
    IllegalPromotionPiece,
    BadGeometry,
    PathBlocked,
    PawnBlocked,
    PawnDiagonalNeedsCapture,
    PawnDoublePushFromWrongRank,
    CastlingUnavailable,
    EnPassantUnavailable,
}

/// Everything `make_move` must remember to reverse itself.
///
/// Technically: this is "what happened", not "what was asked". `Move` is the
/// request; `Undo` is the receipt. It therefore has to record facts that the
/// request cannot express -- which square the captured piece actually stood on
/// (en passant captures land on a different square than the piece occupied),
/// which piece ended up on `to` (a promotion changes the kind in flight), and
/// where the rook started and ended (castling moves two pieces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undo {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub moved: Piece,
    /// The piece that now stands on `to`; differs from `moved` on a promotion.
    pub placed: Piece,
    pub captured: Option<Piece>,
    /// Square the captured piece came from; equals `to` except for en passant.
    pub captured_square: (usize, usize),
    /// `(from, to)` for the rook, when the move was a castle.
    pub castling_rook: Option<((usize, usize), (usize, usize))>,
    pub castling: CastlingRights,
    pub en_passant: Option<(usize, usize)>,
    /// Halfmove clock before the move; restored verbatim by `unmake_move`.
    pub halfmove_clock: u32,
    pub side_to_move: Color,
}

/// Returns `true` only when both coordinates index a real 8x8 board square.
///
/// Technically: the coordinates are `usize`, so negative values are not
/// representable. This exists because generators compute coordinates in `isize`
/// and a value below zero cast back to `usize` wraps to a huge number, which
/// would index out of bounds. Rejecting is always preferable to panicking.
fn in_bounds(sq: (usize, usize)) -> bool {
    sq.0 < 8 && sq.1 < 8
}

/// Converts `(row, column)` into a bit index in `0..64`.
///
/// Technically: the mapping is `row * 8 + column`, so bit 0 is a8 and bit 63 is
/// h1. Ascending bit order is therefore the same row-major order the old
/// mailbox scan used, which keeps generated move lists familiar.
pub fn square_index((row, column): (usize, usize)) -> usize {
    row * 8 + column
}

/// Converts a bit index in `0..64` back into `(row, column)`.
///
/// Technically: the exact inverse of `square_index`. Callers that already hold a
/// `Move` use tuple coordinates; this is for bit scans, which yield indices.
pub fn square_of(index: usize) -> (usize, usize) {
    (index / 8, index % 8)
}

/// Returns the single-bit mask for a square.
///
/// Technically: `1 << index` as a `u64`. Every bitboard operation is built from
/// these masks plus `&`, `|`, and `!`; there is no arithmetic on squares.
pub fn square_bit(square: (usize, usize)) -> u64 {
    1u64 << square_index(square)
}

const KNIGHT_STEPS: [(isize, isize); 8] = [
    (2, 1), (2, -1), (-2, 1), (-2, -1),
    (1, 2), (1, -2), (-1, 2), (-1, -2),
];

const KING_STEPS: [(isize, isize); 8] = [
    (0, 1), (0, -1), (1, 0), (-1, 0),
    (1, 1), (1, -1), (-1, 1), (-1, -1),
];

/// Builds a per-square attack table for a fixed-step piece.
///
/// Technically: evaluates at compile time, so the binary carries 64 `u64`s per
/// piece and pays nothing at startup. Offsets are applied in `isize` so a jump
/// off the board is rejected before it can wrap; the bit index is computed only
/// after the bounds check.
const fn build_step_table(steps: &[(isize, isize); 8]) -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut square = 0usize;
    while square < 64 {
        let row = (square / 8) as isize;
        let column = (square % 8) as isize;
        let mut step = 0usize;
        while step < 8 {
            let r = row + steps[step].0;
            let c = column + steps[step].1;
            if r >= 0 && r < 8 && c >= 0 && c < 8 {
                table[square] |= 1u64 << ((r as usize) * 8 + (c as usize));
            }
            step += 1;
        }
        square += 1;
    }
    table
}

/// Builds a per-square attack table for a pawn of one color.
///
/// Technically: White attacks toward decreasing rows because row 0 is rank 8.
/// The table is indexed `[color][square]` and answers "which squares does a pawn
/// of this color standing here attack?", not "which pawns attack here?". The
/// reverse lookup is the same table for the opposite color, which is why
/// `is_attacked` swaps the color.
const fn build_pawn_table(color: usize) -> [u64; 64] {
    let forward: isize = if color == 0 { -1 } else { 1 };
    let mut table = [0u64; 64];
    let mut square = 0usize;
    while square < 64 {
        let row = (square / 8) as isize;
        let column = (square % 8) as isize;
        let r = row + forward;
        if r >= 0 && r < 8 {
            if column > 0 {
                table[square] |= 1u64 << ((r as usize) * 8 + (column as usize - 1));
            }
            if column < 7 {
                table[square] |= 1u64 << ((r as usize) * 8 + (column as usize + 1));
            }
        }
        square += 1;
    }
    table
}

/// The eight slider directions, in the order the ray tables store them.
///
/// Technically: indices 0..4 are orthogonal (north, south, east, west) and
/// 4..8 are diagonal (northeast, northwest, southeast, southwest). Splitting the
/// ranges is what lets `rook_attacks` and `bishop_attacks` share one ray builder.
const DIRECTIONS: [(isize, isize); 8] = [
    (-1, 0), (1, 0), (0, 1), (0, -1),
    (-1, 1), (-1, -1), (1, 1), (1, -1),
];

/// Whether each direction moves toward higher bit indices.
///
/// Technically: north is `-8`, south is `+8`, east is `+1`, west is `-1`;
/// diagonals combine those deltas. A ray's nearest blocker is the *lowest* set
/// bit when the direction ascends and the *highest* when it descends, so this
/// table selects between `trailing_zeros` and `leading_zeros`.
const DIRECTION_ASCENDS: [bool; 8] = [
    false, true, true, false,
    false, false, true, true,
];

/// Builds the ray table: every square strictly beyond `square` in each direction.
///
/// Technically: rays exclude the origin and include the board edge. Attack
/// generation later intersects a ray with occupancy and keeps everything up to
/// the first blocker, which is why storing the full ray is useful even though it
/// usually gets truncated.
const fn build_rays() -> [[u64; 64]; 8] {
    let mut rays = [[0u64; 64]; 8];
    let mut direction = 0usize;
    while direction < 8 {
        let (dr, dc) = DIRECTIONS[direction];
        let mut square = 0usize;
        while square < 64 {
            let mut row = (square / 8) as isize + dr;
            let mut column = (square % 8) as isize + dc;
            while row >= 0 && row < 8 && column >= 0 && column < 8 {
                rays[direction][square] |= 1u64 << ((row as usize) * 8 + column as usize);
                row += dr;
                column += dc;
            }
            square += 1;
        }
        direction += 1;
    }
    rays
}

/// Builds the "all bits strictly below this one" masks.
///
/// Technically: `BELOW[s]` is `(1 << s) - 1`. Truncating a descending ray at its
/// nearest blocker is then a single `& !BELOW[blocker]`, which keeps the blocker
/// itself so the capture square survives.
const fn build_below() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut square = 0usize;
    while square < 64 {
        table[square] = (1u64 << square) - 1;
        square += 1;
    }
    table
}

/// Builds the "all bits strictly above this one" masks.
///
/// Technically: `ABOVE[s]` is `u64::MAX << (s + 1)`, with the `s == 63` case
/// special-cased to zero because shifting by 64 is undefined. Ascending rays are
/// truncated with `& !ABOVE[blocker]` for the same reason as `BELOW`.
const fn build_above() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut square = 0usize;
    while square < 64 {
        table[square] = if square == 63 { 0 } else { u64::MAX << (square + 1) };
        square += 1;
    }
    table
}

/// Builds the mask of squares whose `(row + column)` is even.
///
/// Technically: used only by insufficient-material detection, where the question
/// is whether every bishop stands on one color complex. Which complex is "even"
/// does not matter, only that the two sets partition the board.
const fn build_even_squares() -> u64 {
    let mut mask = 0u64;
    let mut square = 0usize;
    while square < 64 {
        if (square / 8 + square % 8) % 2 == 0 {
            mask |= 1u64 << square;
        }
        square += 1;
    }
    mask
}

static KNIGHT_ATTACKS: [u64; 64] = build_step_table(&KNIGHT_STEPS);
static KING_ATTACKS: [u64; 64] = build_step_table(&KING_STEPS);
static PAWN_ATTACKS: [[u64; 64]; 2] = [build_pawn_table(0), build_pawn_table(1)];
static RAYS: [[u64; 64]; 8] = build_rays();
static BELOW: [u64; 64] = build_below();
static ABOVE: [u64; 64] = build_above();
static EVEN_SQUARES: u64 = build_even_squares();

/// Returns the squares a knight on `square` attacks.
///
/// Technically: one table lookup. Knights jump, so occupancy is irrelevant and
/// there is no path to block.
pub fn knight_attacks(square: (usize, usize)) -> u64 {
    KNIGHT_ATTACKS[square_index(square)]
}

/// Returns the squares a king on `square` attacks.
///
/// Technically: one table lookup over the eight adjacent squares. The same table
/// makes "kings may not touch" fall out of check detection.
pub fn king_attacks(square: (usize, usize)) -> u64 {
    KING_ATTACKS[square_index(square)]
}

/// Returns the squares a pawn of `color` on `square` attacks.
///
/// Technically: pawn captures are the only attacks that depend on color. A
/// straight push is not an attack, so it is not in this table.
pub fn pawn_attacks(color: Color, square: (usize, usize)) -> u64 {
    PAWN_ATTACKS[color.index()][square_index(square)]
}

/// Returns the squares reachable from `square` along one ray before a blocker.
///
/// Technically: intersects the precomputed ray with occupancy, then keeps the
/// segment from the origin through the *nearest* blocker. The nearest blocker is
/// the lowest set bit on an ascending ray and the highest on a descending one,
/// because bits are laid out row-major. The blocker itself is included so a
/// capture of an enemy piece survives; the caller masks off friendly pieces.
fn ray_attacks(square: usize, occupied: u64, direction: usize) -> u64 {
    let ray = RAYS[direction][square];
    let blockers = ray & occupied;
    if blockers == 0 {
        return ray;
    }

    if DIRECTION_ASCENDS[direction] {
        let blocker = blockers.trailing_zeros() as usize;
        ray & !ABOVE[blocker]
    } else {
        let blocker = 63 - blockers.leading_zeros() as usize;
        ray & !BELOW[blocker]
    }
}

/// Returns the squares a rook on `square` attacks given the full occupancy.
///
/// Technically: four orthogonal ray computations, one per direction, OR-ed
/// together. Occupancy is passed in rather than read from a board so the same
/// function serves attack detection, move generation, and child-board probes.
pub fn rook_attacks(square: (usize, usize), occupied: u64) -> u64 {
    let index = square_index(square);
    ray_attacks(index, occupied, 0)
        | ray_attacks(index, occupied, 1)
        | ray_attacks(index, occupied, 2)
        | ray_attacks(index, occupied, 3)
}

/// Returns the squares a bishop on `square` attacks given the full occupancy.
///
/// Technically: the diagonal half of the ray table. A bishop is exactly these
/// four rays, which is why queens reuse both this and `rook_attacks`.
pub fn bishop_attacks(square: (usize, usize), occupied: u64) -> u64 {
    let index = square_index(square);
    ray_attacks(index, occupied, 4)
        | ray_attacks(index, occupied, 5)
        | ray_attacks(index, occupied, 6)
        | ray_attacks(index, occupied, 7)
}

/// Returns the squares a queen on `square` attacks given the full occupancy.
///
/// Technically: rook plus bishop. The union is exact because the two ray sets
/// are disjoint and together cover all eight directions.
pub fn queen_attacks(square: (usize, usize), occupied: u64) -> u64 {
    rook_attacks(square, occupied) | bishop_attacks(square, occupied)
}

/// Returns `true` if `color` has a piece attacking `square`.
///
/// Technically: the bitboard mirror of the old outward scan, inverted. Instead
/// of probing neighbours one at a time, it asks "does the set of squares this
/// piece type attacks overlap the enemy pieces?". Pawn, knight, and king tests
/// are single table ANDs; sliders are four ray computations each.
///
/// This deliberately implements the FIDE definition of "attacked" (3.1.3): a
/// piece attacks a square even if it is pinned and could not legally move
/// there. That makes it correct both for check detection and for the castling
/// rule, which forbids crossing an attacked square.
pub fn is_attacked(board: &Board, square: (usize, usize), by: Color) -> bool {
    if knight_attacks(square) & board.pieces_of(by, PieceType::Knight) != 0 {
        return true;
    }
    if king_attacks(square) & board.pieces_of(by, PieceType::King) != 0 {
        return true;
    }
    // A pawn of `by` attacks `square` exactly when `square` is one of the
    // squares a pawn of the *opposite* color would attack from there. The
    // mirror is why this swaps the color instead of using the table directly.
    if pawn_attacks(by.opponent(), square) & board.pieces_of(by, PieceType::Pawn) != 0 {
        return true;
    }

    let occupied = board.occupied();
    let rooks_and_queens =
        board.pieces_of(by, PieceType::Rook) | board.pieces_of(by, PieceType::Queen);
    if rook_attacks(square, occupied) & rooks_and_queens != 0 {
        return true;
    }

    let bishops_and_queens =
        board.pieces_of(by, PieceType::Bishop) | board.pieces_of(by, PieceType::Queen);
    bishop_attacks(square, occupied) & bishops_and_queens != 0
}

/// Returns `true` if `color`'s king stands on an attacked square.
///
/// Technically: finds the king with a single trailing-zero scan of its bitboard
/// and delegates to `is_attacked`. A missing king reports "not in check" rather
/// than panicking, which keeps hand-built test positions without a king usable.
pub fn in_check(board: &Board, color: Color) -> bool {
    let king = board.pieces_of(color, PieceType::King);
    if king == 0 {
        return false;
    }
    let square = square_of(king.trailing_zeros() as usize);
    is_attacked(board, square, color.opponent())
}

/// Returns `true` if this move is a castle: a king advancing two files.
///
/// Technically: derived rather than stored, because no other legal move moves a
/// king two columns, so the test is exact. That keeps `Move` a plain
/// `{from, to, promotion}` and avoids a new field at every construction site.
fn is_castling_move(moving: Piece, mv: &Move) -> bool {
    moving.kind == PieceType::King && (mv.to.1 as isize - mv.from.1 as isize).abs() == 2
}

/// Returns `true` if this move is an en passant capture.
///
/// Technically: also derived. A pawn moving diagonally onto an *empty* square
/// can only be en passant, since a normal diagonal pawn move requires a piece to
/// capture. Must be called after validation, which is what guarantees the target
/// square really is the recorded en passant square.
fn is_en_passant_move(board: &Board, moving: Piece, mv: &Move) -> bool {
    moving.kind == PieceType::Pawn
        && mv.to.1 != mv.from.1
        && board.piece_at(mv.to).is_none()
}

/// The part of a position that determines repetition identity.
///
/// Technically: two positions are the same for threefold repetition when the
/// piece placement, side to move, castling rights, and legal en passant
/// possibility all match. The halfmove clock is deliberately excluded -- FIDE
/// 9.2.2 compares possible moves, not how long the position has existed.
///
/// This is a full structural key rather than a Zobrist hash: twelve bitboards
/// plus the position flags, compared exactly. Hashing would be smaller and
/// faster, but a collision here silently turns a win into a draw; the bitboard
/// copy is already cheap enough that the safer trade costs almost nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PositionKey {
    pieces: [[u64; 6]; 2],
    side_to_move: Color,
    castling: CastlingRights,
    en_passant: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    /// One bitboard per color and piece kind: `pieces[color.index()][kind.index()]`.
    pieces: [[u64; 6]; 2],
    /// Occupancy per color, cached so attack and generation code does not rebuild
    /// it from six bitboards on every query. Kept in lockstep by `put_piece`,
    /// `take_piece`, and `set_piece`, which are the only mutators.
    occupied_by_color: [u64; 2],
    /// Both colors' occupancy together; slider rays read this single word.
    occupied: u64,
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<(usize, usize)>,
    /// Halfmoves since the last pawn move or capture. 100 is a fifty-move draw.
    pub halfmove_clock: u32,
}

impl Default for Board {
    /// Creates the standard starting position.
    ///
    /// Technically: delegates to `new` so there is one definition of "the
    /// starting position". `Default` is what collection and test helpers reach
    /// for when they want a usable board rather than an empty one.
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    /// Creates a board with no pieces, no castling rights and White to move.
    ///
    /// Technically: the canonical zero value for hand-built test and puzzle
    /// positions. Every bitboard starts empty, which is why this is the one
    /// constructor that does not need to reason about square layout.
    pub fn empty() -> Self {
        Board {
            pieces: [[0u64; 6]; 2],
            occupied_by_color: [0u64; 2],
            occupied: 0,
            side_to_move: Color::White,
            castling: CastlingRights::NONE,
            en_passant: None,
            halfmove_clock: 0,
        }
    }

    /// Creates the standard starting position with White to move.
    ///
    /// Technically: two back ranks and two pawn ranks, with row 0 as rank 8
    /// (Black's home) and row 7 as rank 1 (White's home). The back-rank table is
    /// written king-and-queen-inward from the outside so both colors share one
    /// definition instead of two mirrored literals that could drift apart. All
    /// castling rights are live and there is no en passant target.
    pub fn new() -> Self {
        let back_rank = [
            PieceType::Rook, PieceType::Knight, PieceType::Bishop,
            PieceType::Queen, PieceType::King,
            PieceType::Bishop, PieceType::Knight, PieceType::Rook,
        ];

        let mut board = Board::empty();
        for (file, kind) in back_rank.iter().enumerate() {
            board.set_piece((0, file), Some(Piece { color: Color::Black, kind: *kind }));
            board.set_piece((7, file), Some(Piece { color: Color::White, kind: *kind }));
        }
        for file in 0..8 {
            board.set_piece((1, file), Some(Piece { color: Color::Black, kind: PieceType::Pawn }));
            board.set_piece((6, file), Some(Piece { color: Color::White, kind: PieceType::Pawn }));
        }

        board.castling = CastlingRights::ALL;
        board
    }

    /// Returns the piece standing on a square, if any.
    ///
    /// Technically: checks the total occupancy first, so an empty square costs
    /// one test. A set square can only belong to one color, which halves the
    /// twelve-way scan to a six-way kind search. Out-of-bounds coordinates
    /// report `None` rather than panicking, so a bad hand-built fixture cannot
    /// crash a formatter or a probe.
    pub fn piece_at(&self, square: (usize, usize)) -> Option<Piece> {
        if !in_bounds(square) {
            return None;
        }

        let mask = square_bit(square);
        if self.occupied & mask == 0 {
            return None;
        }

        // Only one color can own the square, so this halves the twelve-way scan
        // to a six-way one before the kind search.
        let color = if self.occupied_by_color[Color::White.index()] & mask != 0 {
            Color::White
        } else {
            Color::Black
        };

        for kind in [
            PieceType::Pawn,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Rook,
            PieceType::Queen,
            PieceType::King,
        ] {
            if self.pieces[color.index()][kind.index()] & mask != 0 {
                return Some(Piece { color, kind });
            }
        }
        None
    }

    /// Places a piece on a square, replacing whatever was there.
    ///
    /// Technically: removes the existing piece through the same typed helpers
    /// move making uses, then adds the new one. Going through `take_piece` is
    /// what keeps the cached occupancy in lockstep with the piece bitboards; a
    /// direct bitboard write here would desynchronise them.
    pub fn set_piece(&mut self, square: (usize, usize), piece: Option<Piece>) {
        if !in_bounds(square) {
            return;
        }

        if let Some(existing) = self.piece_at(square) {
            self.take_piece(square, existing);
        }
        if let Some(piece) = piece {
            self.put_piece(square, piece);
        }
    }

    /// Returns the bitboard of every piece of one kind and color.
    ///
    /// Technically: a direct array read. This is the raw material for move
    /// generation and evaluation; callers that want "all pieces of a color" use
    /// `occupied_by` instead.
    pub fn pieces_of(&self, color: Color, kind: PieceType) -> u64 {
        self.pieces[color.index()][kind.index()]
    }

    /// Returns the bitboard of every piece belonging to `color`.
    ///
    /// Technically: a cached field read. `put_piece`, `take_piece`, and
    /// `set_piece` maintain it alongside the piece bitboards, so move generation
    /// does not pay six ORs every time it needs a friendly-destination mask.
    pub fn occupied_by(&self, color: Color) -> u64 {
        self.occupied_by_color[color.index()]
    }

    /// Returns the bitboard of every piece on the board.
    ///
    /// Technically: a cached field read. Slider attacks need this single word,
    /// and it is the most frequently read value in the whole search.
    pub fn occupied(&self) -> u64 {
        self.occupied
    }

    /// Sets one bitboard bit and updates both occupancy caches.
    ///
    /// Technically: the fast path used by `make_move`, where the caller has
    /// already established that `square` is empty and knows the exact piece. It
    /// does not clear the square, so it must be paired with `take_piece` on the
    /// origin.
    fn put_piece(&mut self, square: (usize, usize), piece: Piece) {
        let mask = square_bit(square);
        self.pieces[piece.color.index()][piece.kind.index()] |= mask;
        self.occupied_by_color[piece.color.index()] |= mask;
        self.occupied |= mask;
    }

    /// Clears one bitboard bit and updates both occupancy caches.
    ///
    /// Technically: the inverse of `put_piece`, and equally trusting. Unmake
    /// depends on `Undo` naming the exact piece that was on the square, which is
    /// why the receipt records both `moved` and `placed`. Clearing occupancy
    /// unconditionally is safe because a square holds at most one piece: if this
    /// piece was there, no other piece of either color can be.
    fn take_piece(&mut self, square: (usize, usize), piece: Piece) {
        let mask = square_bit(square);
        self.pieces[piece.color.index()][piece.kind.index()] &= !mask;
        self.occupied_by_color[piece.color.index()] &= !mask;
        self.occupied &= !mask;
    }

    /// Returns the structural identity used for threefold repetition.
    ///
    /// Technically: copies the twelve piece bitboards, side to move, castling
    /// rights, and the en passant target *only if a legal en passant capture
    /// exists*. The last condition is FIDE 9.2.2: two placements are different
    /// only when a pawn could actually be captured en passant, so a stale target
    /// or a target blocked by a pin must not split the repetition. The halfmove
    /// clock is excluded because repetition is about the possible moves, not age.
    pub fn position_key(&self) -> PositionKey {
        PositionKey {
            pieces: self.pieces,
            side_to_move: self.side_to_move,
            castling: self.castling,
            en_passant: if self.has_legal_en_passant_capture() {
                self.en_passant
            } else {
                None
            },
        }
    }

    /// Returns `true` when the side to move has a legal en passant capture.
    ///
    /// Technically: the recorded target is the square the double-pushed pawn
    /// skipped. The captured pawn and any capturing pawn stand on the same row:
    /// `target.0 + 1` for White and `target.0 - 1` for Black. Candidate attackers
    /// are found by intersecting the target square's reverse pawn attack set
    /// with the mover's pawns, then each is played on a copy and checked with
    /// `in_check`, so a pinned en passant pawn does not count. That distinction
    /// matters only for repetition identity; move generation already filters
    /// legality later.
    fn has_legal_en_passant_capture(&self) -> bool {
        let Some(target) = self.en_passant else { return false };
        if !in_bounds(target) {
            return false;
        }

        let captured_row = match self.side_to_move {
            Color::White => target.0 + 1,
            Color::Black => match target.0.checked_sub(1) {
                Some(row) => row,
                None => return false,
            },
        };
        if captured_row >= 8 {
            return false;
        }

        // The captured pawn must still be there, and it must be the opponent's.
        match self.piece_at((captured_row, target.1)) {
            Some(piece)
                if piece.color == self.side_to_move.opponent()
                    && piece.kind == PieceType::Pawn => {}
            _ => return false,
        }

        // A mover pawn attacks the target exactly when the target's reverse
        // pawn-attack set contains its square -- the same mirror trick as check
        // detection.
        let candidates = pawn_attacks(self.side_to_move.opponent(), target)
            & self.pieces_of(self.side_to_move, PieceType::Pawn);

        let mut bits = candidates;
        while bits != 0 {
            let from = square_of(bits.trailing_zeros() as usize);
            bits &= bits - 1;

            let mv = Move { from, to: target, promotion: None };
            let mut probe = self.clone();
            if probe.make_move(&mv).is_ok() && !in_check(&probe, self.side_to_move) {
                return true;
            }
        }

        false
    }

    /// Returns `true` for the standard insufficient-material draws.
    ///
    /// Technically: K vs K, K plus one minor piece vs K, and bishop-only
    /// positions where every bishop stands on the same square-color complex are
    /// dead -- no sequence of legal moves can produce checkmate. K+N vs K+N and
    /// K+NN vs K are deliberately *not* included because mate is possible with
    /// the defender's cooperation. The rule is conservative: it does not detect
    /// blocked-pawn dead positions, so it can miss a draw but cannot invent one.
    ///
    /// The bitboard version answers "does any pawn, rook, or queen exist?" with
    /// three bitboard tests and counts minor pieces with `count_ones`, so the old
    /// 64-square scan is gone.
    pub fn is_insufficient_material(&self) -> bool {
        for color in [Color::White, Color::Black] {
            if self.pieces_of(color, PieceType::Pawn) != 0
                || self.pieces_of(color, PieceType::Rook) != 0
                || self.pieces_of(color, PieceType::Queen) != 0
            {
                return false;
            }
        }

        let knights = (self.pieces_of(Color::White, PieceType::Knight)
            | self.pieces_of(Color::Black, PieceType::Knight))
        .count_ones();
        let bishops = self.pieces_of(Color::White, PieceType::Bishop)
            | self.pieces_of(Color::Black, PieceType::Bishop);

        if knights == 0 {
            // No knight can cover the other color complex, so bishops restricted
            // to one complex can never construct a mating net.
            return !(bishops & EVEN_SQUARES != 0 && bishops & !EVEN_SQUARES != 0);
        }

        // A single knight with no bishop is also insufficient; anything more
        // can mate with cooperation and is therefore not a dead position.
        knights == 1 && bishops == 0
    }

    /// Returns `true` once the halfmove clock reaches the fifty-move threshold.
    ///
    /// Technically: FIDE counts 100 halfmoves (50 moves by each player) since
    /// the last pawn move or capture. `make_move` maintains the counter; this
    /// method only interprets it.
    pub fn is_fifty_move_draw(&self) -> bool {
        self.halfmove_clock >= 100
    }

    /// Checks a bishop/rook/queen move against a fixed set of ray directions.
    ///
    /// Technically: splits the displacement into a step count per axis and
    /// requires both axes to agree on that count. That `kr != kc` test is what
    /// stops an off-diagonal target from being accepted, since a single axis
    /// alone cannot distinguish a real ray from an unrelated square. An
    /// orthogonal direction additionally requires zero displacement on the
    /// perpendicular axis -- without that test the ignored axis contributes a
    /// step count of zero and a rook could be "validated" toward a square on a
    /// different rank. Squares strictly between `from` and `to` are then checked
    /// for occupancy; the destination itself is validated by the caller.
    fn validate_slide(&self, mv: &Move, dirs: &[(isize, isize)]) -> Result<(), InvalidMove> {
        let drt = mv.to.0 as isize - mv.from.0 as isize;
        let dct = mv.to.1 as isize - mv.from.1 as isize;
        if drt == 0 && dct == 0 { return Err(InvalidMove::BadGeometry); }

        for &(dr, dc) in dirs {
            let kr = if dr == 0 { 0 } else if drt % dr == 0 { drt / dr } else { continue };
            let kc = if dc == 0 { 0 } else if dct % dc == 0 { dct / dc } else { continue };
            if kr <= 0 && kc <= 0 { continue; }
            if dr != 0 && dc != 0 && kr != kc { continue; }
            if dr == 0 && drt != 0 { continue; }
            if dc == 0 && dct != 0 { continue; }
            if kr < 0 || kc < 0 { continue; }

            let k = if dr != 0 { kr } else { kc };
            for i in 1..k {
                let r = (mv.from.0 as isize + dr * i) as usize;
                let c = (mv.from.1 as isize + dc * i) as usize;
                if self.piece_at((r, c)).is_some() {
                    return Err(InvalidMove::PathBlocked);
                }
            }
            return Ok(());
        }

        Err(InvalidMove::BadGeometry)
    }

    /// Checks a castle: rights, rook presence, and a clear path.
    ///
    /// Technically: structural only. It deliberately does not ask whether the
    /// king is in check or crosses an attacked square -- `make_move` is not
    /// allowed to reason about safety, because the legality filter asks its
    /// questions *by making moves*. Those two rules are enforced there instead.
    fn validate_castling(&self, mv: &Move, king: Piece) -> Result<(), InvalidMove> {
        let home_row = if king.color == Color::White { 7 } else { 0 };

        // Castling is only ever generated from and to the home rank.
        if mv.from != (home_row, 4) || mv.to.0 != home_row {
            return Err(InvalidMove::BadGeometry);
        }

        let (allowed, rook_col, gap): (bool, usize, &[usize]) = match mv.to.1 {
            6 => (self.castling.kingside(king.color), 7, &[5, 6]),
            2 => (self.castling.queenside(king.color), 0, &[1, 2, 3]),
            _ => return Err(InvalidMove::BadGeometry),
        };

        if !allowed { return Err(InvalidMove::CastlingUnavailable); }

        match self.piece_at((home_row, rook_col)) {
            Some(piece) if piece.color == king.color && piece.kind == PieceType::Rook => {}
            _ => return Err(InvalidMove::CastlingUnavailable),
        }

        for &col in gap {
            if self.piece_at((home_row, col)).is_some() {
                return Err(InvalidMove::PathBlocked);
            }
        }

        Ok(())
    }

    /// Checks a pawn move: single push, double push, diagonal captures and en passant.
    ///
    /// Technically: pawns are the only piece whose direction depends on color
    /// and whose capture squares are not their push squares. A straight move
    /// rejects any occupant (a pawn cannot capture forward); a diagonal move
    /// requires either an enemy occupant or a legitimate en passant target. The
    /// en passant branch additionally verifies that the pawn to be removed is
    /// really there, so a stale `en_passant` field cannot produce a phantom
    /// capture.
    fn validate_pawn(&self, mv: &Move, p: Piece) -> Result<(), InvalidMove> {
        let forward: isize = if p.color == Color::White { -1 } else { 1 };
        let home: usize = if p.color == Color::White { 6 } else { 1 };
        let dr = mv.to.0 as isize - mv.from.0 as isize;
        let dc = mv.to.1 as isize - mv.from.1 as isize;

        if dc == 0 {
            if dr != forward && dr != 2 * forward { return Err(InvalidMove::BadGeometry); }
            if dr == 2 * forward {
                if mv.from.0 != home { return Err(InvalidMove::PawnDoublePushFromWrongRank); }
                let mid = (mv.from.0 as isize + forward) as usize;
                if self.piece_at((mid, mv.from.1)).is_some() {
                    return Err(InvalidMove::PathBlocked);
                }
            }
            if self.piece_at(mv.to).is_some() { return Err(InvalidMove::PawnBlocked); }
            return Ok(());
        }

        if dc.abs() != 1 || dr != forward { return Err(InvalidMove::BadGeometry); }

        match self.piece_at(mv.to) {
            Some(target) if target.color != p.color => Ok(()),
            None if self.en_passant == Some(mv.to) => {
                // The captured pawn stands beside the mover, not on `to`.
                match self.piece_at((mv.from.0, mv.to.1)) {
                    Some(t) if t.color != p.color && t.kind == PieceType::Pawn => Ok(()),
                    _ => Err(InvalidMove::EnPassantUnavailable),
                }
            }
            None => Err(InvalidMove::EnPassantUnavailable),
            _ => Err(InvalidMove::PawnDiagonalNeedsCapture),
        }
    }

    /// Checks that `mv` is coherent with the movement rules for this position.
    ///
    /// Technically: a pure function -- it borrows `self` immutably and never
    /// mutates. That purity is precisely what lets `make_move` validate fully
    /// before touching any state, so a rejected move provably leaves the board
    /// untouched. It checks promotion coherence too, because `Option<PieceType>`
    /// can express illegal promotions (`Some(Pawn)`, `Some(King)`). It
    /// deliberately says nothing about king safety; that belongs to
    /// `generate_legal_moves`.
    fn validate(&self, mv: &Move) -> Result<(), InvalidMove> {
        if !in_bounds(mv.from) || !in_bounds(mv.to) { return Err(InvalidMove::OutOfBounds); }

        let moving = self.piece_at(mv.from).ok_or(InvalidMove::NoPieceAtFrom)?;
        if moving.color != self.side_to_move { return Err(InvalidMove::WrongColor); }
        if let Some(target) = self.piece_at(mv.to)
            && target.color == moving.color
        {
            return Err(InvalidMove::OwnPieceAtTo);
        }

        // Castle before geometry: a two-file king move is not one of the eight
        // single-step offsets, so the normal king path would reject it.
        if is_castling_move(moving, mv) {
            return self.validate_castling(mv, moving);
        }

        let promo_rank = if moving.color == Color::White { 0 } else { 7 };
        let lands_on_promo = moving.kind == PieceType::Pawn && mv.to.0 == promo_rank;

        if lands_on_promo {
            match mv.promotion {
                None => return Err(InvalidMove::MustPromote),
                Some(kind)
                    if !matches!(
                        kind,
                        PieceType::Queen | PieceType::Rook | PieceType::Bishop | PieceType::Knight
                    ) =>
                {
                    return Err(InvalidMove::IllegalPromotionPiece);
                }
                _ => {}
            }
        } else if mv.promotion.is_some() {
            return Err(InvalidMove::CannotPromote);
        }

        match moving.kind {
            PieceType::Pawn => self.validate_pawn(mv, moving),
            PieceType::Knight => {
                if is_offset(mv.from, mv.to, &KNIGHT_STEPS) { Ok(()) } else { Err(InvalidMove::BadGeometry) }
            }
            PieceType::King => {
                if is_offset(mv.from, mv.to, &KING_STEPS) { Ok(()) } else { Err(InvalidMove::BadGeometry) }
            }
            PieceType::Bishop => self.validate_slide(mv, &[(1, 1), (1, -1), (-1, 1), (-1, -1)]),
            PieceType::Rook => self.validate_slide(mv, &[(0, 1), (0, -1), (1, 0), (-1, 0)]),
            PieceType::Queen => self.validate_slide(
                mv,
                &[(1, 1), (1, -1), (-1, 1), (-1, -1), (0, 1), (0, -1), (1, 0), (-1, 0)],
            ),
        }
    }

    /// Drops any castling right that depends on `sq` holding a rook.
    ///
    /// Technically: called for both the origin square (a rook leaving home) and
    /// the captured square (a rook removed from home). It is idempotent, so
    /// calling it for ordinary moves that touch nothing relevant is free.
    fn revoke_castling_for_square(&mut self, sq: (usize, usize)) {
        match sq {
            (7, 0) => self.castling.white_queenside = false,
            (7, 7) => self.castling.white_kingside = false,
            (0, 0) => self.castling.black_queenside = false,
            (0, 7) => self.castling.black_kingside = false,
            _ => {}
        }
    }

    /// Applies `mv`, returning the record needed to reverse it.
    ///
    /// Validates before mutating, so a rejected move leaves the board untouched.
    /// This checks that the move is coherent with the movement rules and this
    /// position; whether it is *safe* is the legality filter's job, not this
    /// function's -- the filter works by making a move and asking.
    ///
    /// Technically: handles the three ways a move is more than "pick up and put
    /// down" -- a capture may be on a different square than `to` (en passant), a
    /// promotion changes the piece kind in flight, and a castle moves a second
    /// piece. Every one of those facts is written into `Undo` here, because
    /// `unmake_move` is not permitted to re-derive them.
    pub fn make_move(&mut self, mv: &Move) -> Result<Undo, InvalidMove> {
        self.validate(mv)?;
        Ok(self.make_move_unchecked(mv))
    }

    /// Applies a move already known to be geometrically valid.
    ///
    /// Technically: the body of `make_move` without the `validate` call. The
    /// legality filter generated these moves itself, so re-validating every
    /// candidate would repeat work the generator already guaranteed. The public
    /// `make_move` keeps validation because it is the entry point external
    /// callers and tests use.
    ///
    /// The caller must have established that `mv.from` holds the side to move's
    /// piece and that `mv` is pseudo-legal; otherwise the bitboard updates can
    /// clear a bit that was never set.
    fn make_move_unchecked(&mut self, mv: &Move) -> Undo {
        let prev_side = self.side_to_move;
        let prev_castling = self.castling;
        let prev_en_passant = self.en_passant;
        let prev_halfmove_clock = self.halfmove_clock;

        let moved = self.piece_at(mv.from).expect("caller guaranteed a piece");

        // En passant removes a pawn that is beside the mover, not on `to`.
        // This must be decided before any mutation, because the test is
        // "diagonal pawn move onto an empty square".
        let captured_square = if is_en_passant_move(self, moved, mv) {
            (mv.from.0, mv.to.1)
        } else {
            mv.to
        };
        let captured = self.piece_at(captured_square);

        if let Some(piece) = captured {
            self.take_piece(captured_square, piece);
        }
        self.take_piece(mv.from, moved);

        let placed = match mv.promotion {
            Some(kind) => Piece { color: moved.color, kind },
            None => moved,
        };
        self.put_piece(mv.to, placed);

        // Castling also moves a rook. The corner is implied by the direction.
        let castling_rook = if is_castling_move(moved, mv) {
            let home_row = mv.from.0;
            let (rook_from, rook_to) = if mv.to.1 > mv.from.1 {
                ((home_row, 7), (home_row, 5))
            } else {
                ((home_row, 0), (home_row, 3))
            };
            let rook = Piece { color: moved.color, kind: PieceType::Rook };
            self.take_piece(rook_from, rook);
            self.put_piece(rook_to, rook);
            Some((rook_from, rook_to))
        } else {
            None
        };

        // Rights are lost by moving the king, moving a rook off its home square,
        // or capturing an enemy rook on its home square.
        if moved.kind == PieceType::King {
            match moved.color {
                Color::White => {
                    self.castling.white_kingside = false;
                    self.castling.white_queenside = false;
                }
                Color::Black => {
                    self.castling.black_kingside = false;
                    self.castling.black_queenside = false;
                }
            }
        }
        self.revoke_castling_for_square(mv.from);
        self.revoke_castling_for_square(captured_square);

        // The en passant target is the square a double-pushed pawn skipped over.
        self.en_passant = if moved.kind == PieceType::Pawn
            && (mv.to.0 as isize - mv.from.0 as isize).abs() == 2
        {
            Some(((mv.from.0 + mv.to.0) / 2, mv.from.1))
        } else {
            None
        };

        // A pawn move or a capture is irreversible for fifty-move purposes and
        // resets the clock. Everything else advances it; saturating_add keeps a
        // hand-built absurd position from overflowing in debug builds.
        self.halfmove_clock = if moved.kind == PieceType::Pawn || captured.is_some() {
            0
        } else {
            prev_halfmove_clock.saturating_add(1)
        };

        self.side_to_move = prev_side.opponent();

        Undo {
            from: mv.from,
            to: mv.to,
            moved,
            placed,
            captured,
            captured_square,
            castling_rook,
            castling: prev_castling,
            en_passant: prev_en_passant,
            halfmove_clock: prev_halfmove_clock,
            side_to_move: prev_side,
        }
    }

    /// Reverses a move previously applied by `make_move`.
    ///
    /// Technically: a mechanical inverse. It clears the piece that ended up on
    /// `to` (which may be a promoted piece, hence `Undo::placed`), restores the
    /// captured piece **on the square it actually occupied**, restores the
    /// moving piece on its origin, moves the rook back if the move was a castle,
    /// and then restores the castling/en passant bookkeeping and the turn. It
    /// contains no chess knowledge -- any reasoning unmake appears to need was
    /// captured in the record at make time instead.
    pub fn unmake_move(&mut self, undo: &Undo) {
        self.take_piece(undo.to, undo.placed);
        if let Some(captured) = undo.captured {
            self.put_piece(undo.captured_square, captured);
        }
        self.put_piece(undo.from, undo.moved);

        if let Some((rook_from, rook_to)) = undo.castling_rook {
            let rook = Piece { color: undo.moved.color, kind: PieceType::Rook };
            self.take_piece(rook_to, rook);
            self.put_piece(rook_from, rook);
        }

        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.side_to_move = undo.side_to_move;

        // Debug-only invariant: the cached occupancy must still equal the union
        // of the piece bitboards. A desynchronised cache would not fail loudly
        // anywhere else -- it would just generate subtly wrong moves.
        #[cfg(debug_assertions)]
        {
            for color in [Color::White, Color::Black] {
                let mut expected = 0u64;
                for kind in 0..6 {
                    expected |= self.pieces[color.index()][kind];
                }
                debug_assert_eq!(
                    self.occupied_by_color[color.index()], expected,
                    "occupancy cache drifted for {color:?}"
                );
            }
        }
    }

    /// Returns `true` if this castle must be rejected on safety grounds.
    ///
    /// Technically: the two castling rules that cannot be expressed by "make the
    /// move, then ask if the king is attacked". The king may not castle out of
    /// check, and may not cross an attacked square -- the latter is invisible
    /// after the move because the king has already passed through it. Checked
    /// against the square the king steps over: f1/g1 or d1/c1 depending on side.
    fn castling_is_forbidden(&self, mv: &Move) -> bool {
        let Some(moving) = self.piece_at(mv.from) else { return false };
        if !is_castling_move(moving, mv) { return false; }

        if in_check(self, moving.color) { return true; }

        let transit = (mv.from.0, (mv.from.1 + mv.to.1) / 2);
        is_attacked(self, transit, moving.color.opponent())
    }

    /// Generates every move the side to move may legally play.
    ///
    /// Technically: takes the pseudo-legal list and keeps only moves that do not
    /// leave the mover's own king attacked. Each candidate is played with the
    /// unchecked make, interrogated with `in_check`, then reverted with
    /// `unmake_move`, so the board is bit-identical on entry and exit. The
    /// generator already guarantees geometry, so validation would be redundant
    /// here; it exists on the public `make_move` for external callers.
    ///
    /// Castling needs one extra gate before that loop, because two of its rules
    /// concern squares the king has already left by the time the board is
    /// inspected: it may not start in check or pass through an attacked square.
    ///
    /// Takes `&mut self` for the make/unmake probe. Takes no `color` argument
    /// because the board's own `side_to_move` is the only source of truth; a
    /// parameter could disagree with that field and cause every candidate to be
    /// rejected as `WrongColor`.
    pub fn generate_legal_moves(&mut self) -> Vec<Move> {
        let color = self.side_to_move;
        let pseudo = crate::moves::generate_all_moves(self, color);
        let mut legal = Vec::with_capacity(pseudo.len());

        for mv in pseudo {
            if self.castling_is_forbidden(&mv) {
                continue;
            }

            let undo = self.make_move_unchecked(&mv);
            if !in_check(self, color) {
                legal.push(mv);
            }
            self.unmake_move(&undo);
        }

        legal
    }
}

/// Returns `true` when `to` is exactly one fixed step away from `from`.
///
/// Technically: the arithmetic happens in `isize` so a negative result cannot
/// wrap, and the destination is compared as a tuple. Used for knight and king
/// moves, which have no path to clear -- only bounds and a single offset apply.
fn is_offset(from: (usize, usize), to: (usize, usize), steps: &[(isize, isize)]) -> bool {
    steps.iter().any(|&(dr, dc)| {
        let r = from.0 as isize + dr;
        let c = from.1 as isize + dc;
        r >= 0 && r < 8 && c >= 0 && c < 8 && (r as usize, c as usize) == to
    })
}

impl fmt::Display for Board {
    /// Renders the position as an 8x8 grid with Unicode pieces, rank 8 first.
    ///
    /// Technically: row 0 is printed as rank 8 so the output reads like a real
    /// board. Each cell is a `piece_at` lookup; the trailing file labels are
    /// emitted without a newline so the result composes cleanly with `println!`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 0..8 {
            write!(f, "{} ", 8 - rank)?;
            for file in 0..8 {
                match self.piece_at((rank, file)) {
                    Some(piece) => {
                        let ch = match (piece.color, piece.kind) {
                            (Color::White, PieceType::King)   => '♔',
                            (Color::White, PieceType::Queen)  => '♕',
                            (Color::White, PieceType::Rook)   => '♖',
                            (Color::White, PieceType::Bishop) => '♗',
                            (Color::White, PieceType::Knight) => '♘',
                            (Color::White, PieceType::Pawn)   => '♙',
                            (Color::Black, PieceType::King)   => '♚',
                            (Color::Black, PieceType::Queen)  => '♛',
                            (Color::Black, PieceType::Rook)   => '♜',
                            (Color::Black, PieceType::Bishop) => '♝',
                            (Color::Black, PieceType::Knight) => '♞',
                            (Color::Black, PieceType::Pawn)   => '♟',
                        };
                        write!(f, "{ch} ")?;
                    }
                    None => write!(f, ". ")?,
                }
            }
            writeln!(f)?;
        }
        write!(f, "  a b c d e f g h")
    }
}
