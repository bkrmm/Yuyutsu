use std::fmt;

use crate::moves::Move;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color { White, Black }

impl Color {
    pub fn opponent(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PieceType { Pawn, Knight, Bishop, Rook, Queen, King }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceType,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Undo {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub moved: Piece,
    pub captured: Option<Piece>,
    pub side_to_move: Color,
}

fn in_bounds(sq: (usize, usize)) -> bool {
    sq.0 < 8 && sq.1 < 8
}

fn offset_ok(from: (usize, usize), to: (usize, usize), steps: &[(isize, isize)]) -> bool {
    steps.iter().any(|&(dr, dc)| {
        let r = from.0 as isize + dr;
        let c = from.1 as isize + dc;
        r >= 0 && r < 8 && c >= 0 && c < 8 && (r as usize, c as usize) == to
    })
}

pub fn in_check(board: &Board, color: Color) -> bool {
    let mut king_pos = None;
    'outer: for r in 0..8 {
        for c in 0..8 {
            if let Some(piece) = board.grid[r][c] {
                if piece.color == color && piece.kind == PieceType::King {
                    king_pos = Some((r, c));
                    break 'outer;
                }
            }
        }
    }

    let king_pos = match king_pos {
        Some(pos) => pos,
        None => return false,
    };

    let opponent_moves = crate::moves::generate_all_moves(board, color.opponent());
    opponent_moves.iter().any(|mv| mv.to == king_pos)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub grid: [[Option<Piece>; 8]; 8],
    pub side_to_move: Color,
}

impl Board {
    pub fn empty() -> Self {
        Board { grid: [[None; 8]; 8], side_to_move: Color::White }
    }

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

        board
    }

    fn validate_slide(&self, mv: &Move, dirs: &[(isize, isize)]) -> Result<(), InvalidMove> {
        let drt = mv.to.0 as isize - mv.from.0 as isize;
        let dct = mv.to.1 as isize - mv.from.1 as isize;
        if drt == 0 && dct == 0 { return Err(InvalidMove::BadGeometry); }

        for &(dr, dc) in dirs {
            let kr = if dr == 0 { 0 } else if drt % dr == 0 { drt / dr } else { continue };
            let kc = if dc == 0 { 0 } else if dct % dc == 0 { dct / dc } else { continue };
            if kr <= 0 && kc <= 0 { continue; }
            if dr != 0 && dc != 0 && kr != kc { continue; }
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
            Some(t) if t.color != p.color => Ok(()),
            _ => Err(InvalidMove::PawnDiagonalNeedsCapture),
        }
    }

    fn validate(&self, mv: &Move) -> Result<(), InvalidMove> {
        if !in_bounds(mv.from) || !in_bounds(mv.to) { return Err(InvalidMove::OutOfBounds); }

        let moving = self.grid[mv.from.0][mv.from.1].ok_or(InvalidMove::NoPieceAtFrom)?;
        if moving.color != self.side_to_move { return Err(InvalidMove::WrongColor); }
        if let Some(target) = self.grid[mv.to.0][mv.to.1]
            && target.color == moving.color
        {
            return Err(InvalidMove::OwnPieceAtTo);
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

    /// Applies `mv`, returning the record needed to reverse it.
    ///
    /// Validates before mutating, so a rejected move leaves the board untouched.
    /// This checks that the move is coherent with the movement rules and this
    /// position; whether it is *safe* is the legality filter's job, not this
    /// function's -- the filter works by making a move and asking.
    pub fn make_move(&mut self, mv: &Move) -> Result<Undo, InvalidMove> {
        self.validate(mv)?;

        let prev_side = self.side_to_move;
        let moved = self.grid[mv.from.0][mv.from.1].expect("validated above");
        let captured = self.grid[mv.to.0][mv.to.1];

        self.grid[mv.from.0][mv.from.1] = None;
        self.grid[mv.to.0][mv.to.1] = Some(match mv.promotion {
            Some(kind) => Piece { color: moved.color, kind },
            None => moved,
        });
        self.side_to_move = prev_side.opponent();

        Ok(Undo { from: mv.from, to: mv.to, moved, captured, side_to_move: prev_side })
    }

    pub fn unmake_move(&mut self, undo: &Undo) {
        self.grid[undo.from.0][undo.from.1] = Some(undo.moved);
        self.grid[undo.to.0][undo.to.1] = undo.captured;
        self.side_to_move = undo.side_to_move;
    }
}

impl fmt::Display for Board {
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
