use crate::board::{Board, Color, PieceType, Piece};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Move {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub promotion : Option<PieceType>, //none for non-pawn moves
}

pub fn generate_pseudo_moves(board: &Board, row: usize, column: usize, piece: Piece) -> Vec<Move> {
    match piece.kind {
        PieceType::Pawn => generate_pawn_moves(board, row, column, piece.color),
        PieceType::Knight => generate_knight_moves(board, row, column, piece.color),
        PieceType::Bishop => generate_bishop_moves(board, row, column, piece.color),
        PieceType::Rook => generate_rook_moves(board, row, column, piece.color),
        PieceType::King => generate_king_moves(board, row, column, piece.color),
        PieceType::Queen => generate_queen_moves(board, row, column, piece.color)
    }
}

pub fn generate_all_moves(board: &Board, color: Color) -> Vec<Move> {
    let mut moves = vec![];
    for r in 0..8 {
        for c in 0..8 {
            if let Some(piece) = board.grid[r][c] {
                if piece.color == color {
                    moves.extend(generate_pseudo_moves(board, r, c, piece));
                }
            }
        }
    }
    moves
}

//pawn
pub fn generate_pawn_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let forward: isize = if color == Color::White {-1} else {1};
    let promo_rank: isize  = if color == Color::White {0} else {7};
    let home_rank: isize = if color == Color::White {6} else {1};
    let promos = [PieceType::Queen, PieceType::Rook, PieceType::Bishop, PieceType::Knight];
    let mut moves: Vec<Move> = vec![];

    //stay isize until the target rank is known to be on the board
    let nextrow_i = row as isize + forward;
    if nextrow_i < 0 || nextrow_i >= 8 { return moves; }
    let nextrow = nextrow_i as usize;

    //Single Push
    if board.grid[nextrow][column].is_none() {
        //promotion
        if nextrow as isize == promo_rank {
            for promo in promos {
                moves.push(Move {from: (row, column), to: (nextrow, column), promotion: Some(promo)});
            }
        }
        else {
                //Normal Single Push
                moves.push(Move {from: (row, column), to: (nextrow, column), promotion: None });

                //Double Push
                if row as isize == home_rank {
                    let double_push = (row as isize + 2 * forward) as usize;
                    //the outer check only clears the intermediate square, not the landing square
                    if board.grid[double_push][column].is_none() {
                        moves.push(Move {from: (row, column), to: (double_push, column), promotion: None});
                    }
                }
        }
    }

    //Captures (diagonal)
    for dc in [-1isize, 1] {
        let nc = column as isize + dc;
        if nc < 0 || nc >= 8 { continue; }
        let nc = nc as usize;

        if let Some(piece) = board.grid[nextrow][nc] {
            if piece.color == color { continue; }

            if nextrow as isize == promo_rank {
                for promo in promos {
                    moves.push(Move { from: (row, column), to: (nextrow, nc), promotion: Some(promo) });
                }
            } else {
                moves.push(Move { from: (row, column), to: (nextrow, nc), promotion: None });
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
            _ => moves.push(Move { from: (row, column), to: (nr, nc), promotion: None }),
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
                    moves.push(Move { from: (row, column), to: (r, c), promotion: None });
                }
                Some(piece) => {
                    if piece.color != color {
                        moves.push(Move { from: (row, column), to: (r, c), promotion: None});
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
                    moves.push(Move { from: (row, column), to: (r, c), promotion: None });
                }
                Some(piece) => {
                    if piece.color != color {
                        moves.push(Move { from: (row, column), to: (r, c), promotion: None });
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
            _ => moves.push(Move { from: (row, column), to: (nr, nc), promotion: None }),
        }
    }

    moves
}
