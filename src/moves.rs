#![allow(dead_code)]
#![allow(unused)]

use crate::board::{Board, Color, PieceType};

pub struct Move {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

//pawn
pub fn generate_pawn_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let forward: isize = if color == Color::White {-1} else {1};
    let promo_rank: isize  = if color == Color::White {0} else {7};
    let home_rank: isize = if color == Color::White {6} else {1};
    let mut moves: Vec<Move> = vec![];

    //Single Push
    let nextrow = (row as isize + forward) as usize;
    let double_push = (row as isize + 2 * forward) as usize;

    //collision detection
    if board.grid[nextrow][column].is_none() {
        //promotion
        if nextrow == promo_rank as usize {
            for promo in [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight] {
                moves.push(Move {from: (row, column), to: (nextrow, column)});
            }
        }
        else {
                //Normal Single Push
                moves.push(Move {from: (row, column), to: (nextrow, column) });

                //Double Push
                if row == home_rank as usize {
                    moves.push(Move {from: (row, column), to: (double_push, column) });
                }
        }
    }

    //Captures (diagonal)
    for dc in [-1, 1] {
        let nc = column as isize + dc;
        if nc < 0 || nc >= 8 { continue; }
        let (nc, nr) = (nc as usize, nextrow);

        if let Some(piece) = &board.grid[nr][nc] {
            if piece.color != color {
                moves.push(Move { from: (row, column), to: (nr, nc) });
            }
        }
    }

    moves
}

//Knight
pub fn generate_knight_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    const STEPS: [(isize, isize); 8] = [
        (2, 1), (2, -1), (-2, 1), (-2, -1),
        (1, 2), (1, -2), (-1, 2), (-1, -2),
    ];

    let mut moves = vec![];

    for (dr, dc) in STEPS {
        let nr = row as isize + dr;
        let nc = column as isize + dc;

        if nr < 0 || nr >= 8 || nc < 0 || nc >= 8 {
            continue;
        }

        let (nr, nc) = (nr as usize, nc as usize);

        match board.grid[nr][nc] {
            Some(piece) if piece.color == color => {}
            _ => moves.push(Move { from: (row, column), to: (nr, nc) }),
        }
    }

    moves
}

//Bishop
pub fn generate_bishop_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    const DIRS: [(isize, isize); 4] = [
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];

    let mut moves = vec![];

    for (dr, dc) in DIRS {
        let mut nr = row as isize + dr;
        let mut nc = column as isize + dc;

        while nr >= 0 && nr < 8 && nc >= 0 && nc < 8 {
            let (r, c) = (nr as usize, nc as usize);
            match board.grid[r][c] {
                None => {
                    moves.push(Move { from: (row, column), to: (r, c) });
                }
                Some(piece) => {
                    if piece.color != color {
                        moves.push(Move { from: (row, column), to: (r, c) });
                    }
                    break;
                }
            }
            nr += dr;
            nc += dc;
        }
    }

    moves
}

//Rook
pub fn generate_rook_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    const DIRS: [(isize, isize); 4] = [
        (0, 1), (0, -1), (1, 0), (-1, 0),
    ];

    let mut moves = vec![];

    for (dr, dc) in DIRS {
        let mut nr = row as isize + dr;
        let mut nc = column as isize + dc;

        while nr >= 0 && nr < 8 && nc >= 0 && nc < 8 {
            let (r, c) = (nr as usize, nc as usize);
            match board.grid[r][c] {
                None => {
                    moves.push(Move { from: (row, column), to: (r, c) });
                }
                Some(piece) => {
                    if piece.color != color {
                        moves.push(Move { from: (row, column), to: (r, c) });
                    }
                    break;
                }
            }
            nr += dr;
            nc += dc;
        }
    }

    moves
}

//Queen
pub fn generate_queen_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = generate_bishop_moves(board, row, column, color);
    moves.extend(generate_rook_moves(board, row, column, color));
    moves
}

//King
pub fn generate_king_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    const STEPS: [(isize, isize); 8] = [
        (0, 1), (0, -1), (1, 0), (-1, 0),
        (1, 1), (1, -1), (-1, 1), (-1, -1),
    ];

    let mut moves = vec![];

    for (dr, dc) in STEPS {
        let nr = row as isize + dr;
        let nc = column as isize + dc;

        if nr < 0 || nr >= 8 || nc < 0 || nc >= 8 {
            continue;
        }

        let (nr, nc) = (nr as usize, nc as usize);

        match board.grid[nr][nc] {
            Some(piece) if piece.color == color => {}
            _ => moves.push(Move { from: (row, column), to: (nr, nc) }),
        }
    }

    moves
}
