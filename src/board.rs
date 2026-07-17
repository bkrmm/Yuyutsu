use std::fmt;

use crate::moves::Move;

#[derive(Clone, Copy, PartialEq)]
pub enum Color { White, Black }

#[derive(Clone, Copy, PartialEq)]
pub enum PieceType { Pawn, Knight, Bishop, Rook, Queen, King }

#[derive(Clone, Copy, PartialEq)]
pub struct Piece {
    pub color: Color,
    pub kind: PieceType,
}

pub struct Board {
    pub grid: [[Option<Piece>; 8]; 8],
}

impl Board {
    pub fn new() -> Self {
        let mut grid = [[None; 8]; 8];

        let back_rank = [
            PieceType::Rook, PieceType::Knight, PieceType::Bishop,
            PieceType::Queen, PieceType::King,
            PieceType::Bishop, PieceType::Knight, PieceType::Rook,
        ];

        for (file, kind) in back_rank.iter().enumerate() {
            grid[0][file] = Some(Piece { color: Color::Black, kind: *kind });
        }
        for file in 0..8 {
            grid[1][file] = Some(Piece { color: Color::Black, kind: PieceType::Pawn });
        }
        for file in 0..8 {
            grid[6][file] = Some(Piece { color: Color::White, kind: PieceType::Pawn });
        }
        for (file, kind) in back_rank.iter().enumerate() {
            grid[7][file] = Some(Piece { color: Color::White, kind: *kind });
        }

        Board { grid }
    }

    pub fn make_move(&mut self, mv: &Move) -> bool {
        // This line MODIFIES the board (via &mut self):
        self.grid[mv.to.0][mv.to.1] = self.grid[mv.from];
        self.grid[mv.from] = None;
        // The bool is just a status:
        if something_went_wrong {
          return false;  // move failed
        }
        true  // move succeeded
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
