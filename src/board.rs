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
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PieceType { Pawn, Knight, Bishop, Rook, Queen, King }

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
/// (en passant captures land on a different square than the piece occupied) and
/// where the rook started and ended (castling moves two pieces).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undo {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub moved: Piece,
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

/// Returns `true` when `to` is exactly one fixed step away from `from`.
///
/// Technically: the arithmetic happens in `isize` so a negative result cannot
/// wrap, and the destination is compared as a tuple. Used for knight and king
/// moves, which have no path to clear -- only bounds and a single offset apply.
fn offset_ok(from: (usize, usize), to: (usize, usize), steps: &[(isize, isize)]) -> bool {
    steps.iter().any(|&(dr, dc)| {
        let r = from.0 as isize + dr;
        let c = from.1 as isize + dc;
        r >= 0 && r < 8 && c >= 0 && c < 8 && (r as usize, c as usize) == to
    })
}

/// Finds `color`'s king, if it is on the board.
///
/// Technically: a linear scan of 64 squares. Doing this once per query is fine
/// at the current scale; an engine that caches king squares per color would
/// replace this with two indexed lookups.
fn find_king(board: &Board, color: Color) -> Option<(usize, usize)> {
    for r in 0..8 {
        for c in 0..8 {
            if let Some(piece) = board.grid[r][c]
                && piece.color == color
                && piece.kind == PieceType::King
            {
                return Some((r, c));
            }
        }
    }
    None
}

/// Returns `true` if `color` has a piece attacking `square`.
///
/// Technically: scans *outward from the target* rather than generating moves,
/// so it allocates nothing. Pawns, knights and kings are checked as fixed
/// offsets; sliders walk each ray and stop at the first piece, because only the
/// nearest piece on a ray can attack.
///
/// This deliberately implements the FIDE definition of "attacked" (3.1.3): a
/// piece attacks a square even if it is pinned and could not legally move
/// there. That makes it correct both for check detection and for the castling
/// rule, which forbids crossing an attacked square.
pub fn is_attacked(board: &Board, square: (usize, usize), by: Color) -> bool {
    let r = square.0 as isize;
    let c = square.1 as isize;

    // Pawns. White attacks one row above itself (row - 1), so a White attacker
    // sits one row *below* the target. Black is the mirror image.
    let pawn_row = if by == Color::White { r + 1 } else { r - 1 };
    for dc in [-1isize, 1] {
        let pc = c + dc;
        if pawn_row >= 0 && pawn_row < 8 && pc >= 0 && pc < 8
            && let Some(piece) = board.grid[pawn_row as usize][pc as usize]
            && piece.color == by
            && piece.kind == PieceType::Pawn
        {
            return true;
        }
    }

    // Knights: fixed jumps, nothing can block them.
    const KNIGHT: [(isize, isize); 8] = [
        (2, 1), (2, -1), (-2, 1), (-2, -1),
        (1, 2), (1, -2), (-1, 2), (-1, -2),
    ];
    for (dr, dc) in KNIGHT {
        let nr = r + dr;
        let nc = c + dc;
        if nr >= 0 && nr < 8 && nc >= 0 && nc < 8
            && let Some(piece) = board.grid[nr as usize][nc as usize]
            && piece.color == by
            && piece.kind == PieceType::Knight
        {
            return true;
        }
    }

    // Kings: adjacent squares. This also makes "kings may not touch" fall out
    // of check detection instead of needing a separate rule.
    const KING: [(isize, isize); 8] = [
        (0, 1), (0, -1), (1, 0), (-1, 0),
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];
    for (dr, dc) in KING {
        let nr = r + dr;
        let nc = c + dc;
        if nr >= 0 && nr < 8 && nc >= 0 && nc < 8
            && let Some(piece) = board.grid[nr as usize][nc as usize]
            && piece.color == by
            && piece.kind == PieceType::King
        {
            return true;
        }
    }

    // Sliding pieces. Walk each ray from the target outward and stop at the
    // first occupied square: a piece behind a blocker cannot attack the target.
    const ORTHO: [(isize, isize); 4] = [(0, 1), (0, -1), (1, 0), (-1, 0)];
    for (dr, dc) in ORTHO {
        let mut nr = r + dr;
        let mut nc = c + dc;
        while nr >= 0 && nr < 8 && nc >= 0 && nc < 8 {
            if let Some(piece) = board.grid[nr as usize][nc as usize] {
                if piece.color == by && matches!(piece.kind, PieceType::Rook | PieceType::Queen) {
                    return true;
                }
                break;
            }
            nr += dr;
            nc += dc;
        }
    }

    const DIAG: [(isize, isize); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    for (dr, dc) in DIAG {
        let mut nr = r + dr;
        let mut nc = c + dc;
        while nr >= 0 && nr < 8 && nc >= 0 && nc < 8 {
            if let Some(piece) = board.grid[nr as usize][nc as usize] {
                if piece.color == by && matches!(piece.kind, PieceType::Bishop | PieceType::Queen) {
                    return true;
                }
                break;
            }
            nr += dr;
            nc += dc;
        }
    }

    false
}

/// Returns `true` if `color`'s king stands on an attacked square.
///
/// Technically: locates the king and delegates to `is_attacked`. A missing king
/// reports "not in check" rather than panicking, which keeps hand-built test
/// positions without a king usable.
pub fn in_check(board: &Board, color: Color) -> bool {
    match find_king(board, color) {
        Some(king) => is_attacked(board, king, color.opponent()),
        None => false,
    }
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
        && board.grid[mv.to.0][mv.to.1].is_none()
}

/// The part of a position that determines repetition identity.
///
/// Technically: two positions are the same for threefold repetition when the
/// piece placement, side to move, castling rights, and legal en passant
/// possibility all match. The halfmove clock is deliberately excluded -- FIDE
/// 9.2.2 compares possible moves, not how long the position has existed.
///
/// This is a full structural key rather than a Zobrist hash. Hashing would be
/// faster, but a collision here silently turns a win into a draw; at the current
/// scale an exact 8x8 copy is the safer trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PositionKey {
    grid: [[Option<Piece>; 8]; 8],
    side_to_move: Color,
    castling: CastlingRights,
    en_passant: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub grid: [[Option<Piece>; 8]; 8],
    pub side_to_move: Color,
    pub castling: CastlingRights,
    pub en_passant: Option<(usize, usize)>,
    /// Halfmoves since the last pawn move or capture. 100 is a fifty-move draw.
    pub halfmove_clock: u32,
}

impl Board {
    /// Creates a board with no pieces, no castling rights and White to move.
    ///
    /// Technically: the canonical zero value for hand-built test and puzzle
    /// positions. `new()` is built on top of this so only one place constructs
    /// the empty grid.
    pub fn empty() -> Self {
        Board {
            grid: [[None; 8]; 8],
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
            board.grid[0][file] = Some(Piece { color: Color::Black, kind: *kind });
        }
        for file in 0..8 {
            board.grid[1][file] = Some(Piece { color: Color::Black, kind: PieceType::Pawn });
        }
        for file in 0..8 {
            board.grid[6][file] = Some(Piece { color: Color::White, kind: PieceType::Pawn });
        }
        for (file, kind) in back_rank.iter().enumerate() {
            board.grid[7][file] = Some(Piece { color: Color::White, kind: *kind });
        }

        board.castling = CastlingRights::ALL;
        board
    }

    /// Returns the structural identity used for threefold repetition.
    ///
    /// Technically: copies the piece placement, side to move, castling rights,
    /// and the en passant target *only if a legal en passant capture exists*.
    /// The last condition is FIDE 9.2.2: two placements are different only when
    /// a pawn could actually be captured en passant, so a stale target or a
    /// target blocked by a pin must not split the repetition. The halfmove clock
    /// is excluded because repetition is about the possible moves, not age.
    pub fn position_key(&self) -> PositionKey {
        PositionKey {
            grid: self.grid,
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
    /// `target.0 + 1` for White and `target.0 - 1` for Black. Each adjacent
    /// candidate is played on a clone and checked with `in_check`, so a pinned
    /// en passant pawn does not count. That distinction matters only for
    /// repetition identity; move generation already filters legality later.
    fn has_legal_en_passant_capture(&self) -> bool {
        let Some(target) = self.en_passant else { return false };
        if target.0 >= 8 || target.1 >= 8 { return false; }

        let captured_row = match self.side_to_move {
            Color::White => target.0 + 1,
            Color::Black => match target.0.checked_sub(1) {
                Some(row) => row,
                None => return false,
            },
        };
        if captured_row >= 8 { return false; }

        // The captured pawn must still be there, and it must be the opponent's.
        match self.grid[captured_row][target.1] {
            Some(piece)
                if piece.color == self.side_to_move.opponent()
                    && piece.kind == PieceType::Pawn => {}
            _ => return false,
        }

        for dc in [-1isize, 1] {
            let from_col = target.1 as isize + dc;
            if from_col < 0 || from_col >= 8 { continue; }
            let from = (captured_row, from_col as usize);

            match self.grid[from.0][from.1] {
                Some(piece) if piece.color == self.side_to_move && piece.kind == PieceType::Pawn => {
                    let mv = Move { from, to: target, promotion: None };
                    let mut probe = self.clone();
                    if probe.make_move(&mv).is_ok() && !in_check(&probe, self.side_to_move) {
                        return true;
                    }
                }
                _ => {}
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
    pub fn is_insufficient_material(&self) -> bool {
        let mut knights = 0u32;
        let mut bishops = 0u32;
        let mut even_square_bishop = false;
        let mut odd_square_bishop = false;

        for r in 0..8 {
            for c in 0..8 {
                let Some(piece) = self.grid[r][c] else { continue };

                match piece.kind {
                    PieceType::Pawn | PieceType::Rook | PieceType::Queen => return false,
                    PieceType::Knight => knights += 1,
                    PieceType::Bishop => {
                        bishops += 1;
                        if (r + c) % 2 == 0 {
                            even_square_bishop = true;
                        } else {
                            odd_square_bishop = true;
                        }
                    }
                    PieceType::King => {}
                }
            }
        }

        if knights == 0 {
            // No knight can cover the other color complex, so bishops restricted
            // to one complex can never construct a mating net.
            return !(even_square_bishop && odd_square_bishop);
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
                if self.grid[r][c].is_some() { return Err(InvalidMove::PathBlocked); }
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

        match self.grid[home_row][rook_col] {
            Some(piece) if piece.color == king.color && piece.kind == PieceType::Rook => {}
            _ => return Err(InvalidMove::CastlingUnavailable),
        }

        for &col in gap {
            if self.grid[home_row][col].is_some() {
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
                if self.grid[mid][mv.from.1].is_some() { return Err(InvalidMove::PathBlocked); }
            }
            if self.grid[mv.to.0][mv.to.1].is_some() { return Err(InvalidMove::PawnBlocked); }
            return Ok(());
        }

        if dc.abs() != 1 || dr != forward { return Err(InvalidMove::BadGeometry); }

        match self.grid[mv.to.0][mv.to.1] {
            Some(target) if target.color != p.color => Ok(()),
            None if self.en_passant == Some(mv.to) => {
                // The captured pawn stands beside the mover, not on `to`.
                let captured = self.grid[mv.from.0][mv.to.1];
                match captured {
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

        let moving = self.grid[mv.from.0][mv.from.1].ok_or(InvalidMove::NoPieceAtFrom)?;
        if moving.color != self.side_to_move { return Err(InvalidMove::WrongColor); }
        if let Some(target) = self.grid[mv.to.0][mv.to.1]
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
                const STEPS: [(isize, isize); 8] = [
                    (2, 1), (2, -1), (-2, 1), (-2, -1),
                    (1, 2), (1, -2), (-1, 2), (-1, -2),
                ];
                if offset_ok(mv.from, mv.to, &STEPS) { Ok(()) } else { Err(InvalidMove::BadGeometry) }
            }
            PieceType::King => {
                const STEPS: [(isize, isize); 8] = [
                    (0, 1), (0, -1), (1, 0), (-1, 0),
                    (1, 1), (1, -1), (-1, 1), (-1, -1),
                ];
                if offset_ok(mv.from, mv.to, &STEPS) { Ok(()) } else { Err(InvalidMove::BadGeometry) }
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

        let prev_side = self.side_to_move;
        let prev_castling = self.castling;
        let prev_en_passant = self.en_passant;
        let prev_halfmove_clock = self.halfmove_clock;

        let moved = self.grid[mv.from.0][mv.from.1].expect("validated above");

        // En passant removes a pawn that is beside the mover, not on `to`.
        let captured_square = if is_en_passant_move(self, moved, mv) {
            (mv.from.0, mv.to.1)
        } else {
            mv.to
        };
        let captured = self.grid[captured_square.0][captured_square.1];

        self.grid[captured_square.0][captured_square.1] = None;
        self.grid[mv.from.0][mv.from.1] = None;
        self.grid[mv.to.0][mv.to.1] = Some(match mv.promotion {
            Some(kind) => Piece { color: moved.color, kind },
            None => moved,
        });

        // Castling also moves a rook. The corner is implied by the direction.
        let castling_rook = if is_castling_move(moved, mv) {
            let home_row = mv.from.0;
            let (rook_from, rook_to) = if mv.to.1 > mv.from.1 {
                ((home_row, 7), (home_row, 5))
            } else {
                ((home_row, 0), (home_row, 3))
            };
            self.grid[rook_to.0][rook_to.1] = self.grid[rook_from.0][rook_from.1].take();
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

        Ok(Undo {
            from: mv.from,
            to: mv.to,
            moved,
            captured,
            captured_square,
            castling_rook,
            castling: prev_castling,
            en_passant: prev_en_passant,
            halfmove_clock: prev_halfmove_clock,
            side_to_move: prev_side,
        })
    }

    /// Reverses a move previously applied by `make_move`.
    ///
    /// Technically: a mechanical inverse. It restores the moving piece, the
    /// captured piece **on the square it actually occupied**, the rook if the
    /// move was a castle, and the castling/en passant bookkeeping, then hands
    /// the turn back. It contains no chess knowledge -- no rule checks, no
    /// "was this a pawn". Any chess reasoning unmake appears to need should have
    /// been captured in the record at make time instead.
    pub fn unmake_move(&mut self, undo: &Undo) {
        self.grid[undo.to.0][undo.to.1] = None;
        self.grid[undo.captured_square.0][undo.captured_square.1] = undo.captured;
        self.grid[undo.from.0][undo.from.1] = Some(undo.moved);

        if let Some((rook_from, rook_to)) = undo.castling_rook {
            self.grid[rook_from.0][rook_from.1] = self.grid[rook_to.0][rook_to.1].take();
        }

        self.castling = undo.castling;
        self.en_passant = undo.en_passant;
        self.halfmove_clock = undo.halfmove_clock;
        self.side_to_move = undo.side_to_move;
    }

    /// Returns `true` if this castle must be rejected on safety grounds.
    ///
    /// Technically: the two castling rules that cannot be expressed by "make the
    /// move, then ask if the king is attacked". The king may not castle out of
    /// check, and may not cross an attacked square -- the latter is invisible
    /// after the move because the king has already passed through it. Checked
    /// against the square the king steps over: f1/g1 or d1/c1 depending on side.
    fn castling_is_forbidden(&self, mv: &Move) -> bool {
        let Some(moving) = self.grid[mv.from.0][mv.from.1] else { return false };
        if !is_castling_move(moving, mv) { return false; }

        if in_check(self, moving.color) { return true; }

        let transit = (mv.from.0, (mv.from.1 + mv.to.1) / 2);
        is_attacked(self, transit, moving.color.opponent())
    }

    /// Generates every move the side to move may legally play.
    ///
    /// Technically: takes the pseudo-legal list and keeps only moves that do not
    /// leave the mover's own king attacked. Each candidate is played with
    /// `make_move`, interrogated with `in_check`, then reverted with
    /// `unmake_move`, so the board is byte-identical on entry and exit. This
    /// loop is the entire reason the `Undo` record exists: answering "is this
    /// move safe?" requires making it, and cloning the board per candidate would
    /// allocate once per move -- fatal inside a search tree.
    ///
    /// Castling needs one extra gate before that loop, because two of its rules
    /// concern squares the king has already left by the time the board is
    /// inspected: it may not start in check or pass through an attacked square.
    ///
    /// Takes `&mut self` rather than `&self` for that same reason. Takes no
    /// `color` argument because `make_move` validates against
    /// `self.side_to_move`; a parameter could disagree with that field and cause
    /// every candidate to be rejected as `WrongColor`. When a parameter must
    /// always equal a field, derive it from the field.
    pub fn generate_legal_moves(&mut self) -> Vec<Move> {
        let color = self.side_to_move;
        let pseudo = crate::moves::generate_all_moves(self, color);
        let mut legal = Vec::with_capacity(pseudo.len());

        for mv in pseudo {
            if self.castling_is_forbidden(&mv) {
                continue;
            }

            // A rejected candidate never mutated the board, because `make_move`
            // validates before writing. Skipping without unmaking is therefore
            // safe, and is the payoff for keeping validation pure.
            let undo = match self.make_move(&mv) {
                Ok(undo) => undo,
                Err(_) => continue,
            };

            if !in_check(self, color) {
                legal.push(mv);
            }

            self.unmake_move(&undo);
        }

        legal
    }
}

impl fmt::Display for Board {
    /// Renders the position as an 8x8 grid with Unicode pieces, rank 8 first.
    ///
    /// Technically: row 0 is printed as rank 8 so the output reads like a real
    /// board. The trailing file labels are emitted without a newline so the
    /// result composes cleanly with `println!`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for rank in 0..8 {
            write!(f, "{} ", 8 - rank)?;
            for file in 0..8 {
                match self.grid[rank][file] {
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
