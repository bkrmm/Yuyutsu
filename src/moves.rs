#![allow(dead_code)]
#![allow(unused)]

use crate::board::{Board, Color, PieceType};

pub struct Move {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

//pawn
pub fn generate_pawn_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let foward: isize = if color == Color::White {-1} else {1};
    let promo_rank: isize  = if color == Color::White {0} else {7};
    let home_rank: isize = if color == Color::White {6} else {1};
    let mut moves: Vec<Move> = vec![];

    //Single Push
    let nextrow = (row as isize + foward) as usize;
    let double_push = (row as isize + 2*foward) as usize;

    //colision detection
    if board.grid[nextrow][column].is_none() {
        //promotion
        if nextrow == promo_rank as usize { 
            for promo in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                moves.push(Move {from: (row, column), to: (nextrow,column)});
            } 
        }      
        else {
                //Normal Single Push
                moves.push(Move {from: (row, column), to: (nextrow, column) });

                //Double Push 
                if row == home_rank as usize{
                    moves.push(Move {from: (row, column), to: (double_push, column) });
                }
        }
    }

    moves
}
