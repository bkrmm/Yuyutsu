use crate::board::{Board, Color, PieceType, Piece};

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
/// generated from.
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

/// Generates every pseudo-legal move for `color`.
///
/// Technically: scans all 64 squares in row-major order and extends the result
/// with each matching piece's moves. The resulting order is an implementation
/// detail of this scan -- never index into this list expecting a particular
/// move, because a change to the loop or a generator's push order silently
/// reorders it.
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

/// Generates pseudo-legal pawn moves: single push, double push, and captures.
///
/// Technically: `forward` is -1 for White because row 0 is rank 8, so advancing
/// decreases the row. The target rank is computed in `isize` and bounds-checked
/// *before* conversion, because a pawn already sitting on the last rank would
/// otherwise wrap to a huge `usize` and index out of bounds. The double push
/// needs two separate occupancy tests: the intermediate square and the landing
/// square are different squares, and checking only one of them was a real bug.
/// Promotion fans out into four pieces for both pushes and captures.
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

/// Generates pseudo-legal knight moves from one square.
///
/// Technically: knights jump, so no path check is needed -- only board bounds
/// and whether the destination holds a friendly piece. The eight offsets are
/// listed exhaustively rather than derived, which keeps the rule visible.
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

/// Generates pseudo-legal bishop moves along the four diagonals.
///
/// Technically: walks each ray outward until it leaves the board, stopping after
/// the first occupied square. That occupant is appended only when it is an enemy
/// piece -- the capture case -- which is what makes the ray terminate correctly
/// instead of passing through a blocker.
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

/// Generates pseudo-legal rook moves along the four orthogonals.
///
/// Technically: the same ray walk as the bishop with a different direction
/// table. It stops after the first occupied square and captures only enemy
/// pieces, so a friendly blocker terminates the ray without emitting a move.
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

/// Generates pseudo-legal queen moves.
///
/// Technically: a queen is exactly a rook plus a bishop, so this concatenates
/// the two generators instead of duplicating the ray-walking code. Any future
/// fix to ray handling lands in one place.
pub fn generate_queen_moves(board: &Board, row: usize, column: usize, color: Color) -> Vec<Move> {
    let mut moves = generate_bishop_moves(board, row, column, color);
    moves.extend(generate_rook_moves(board, row, column, color));
    moves
}

/// Generates pseudo-legal king moves from one square.
///
/// Technically: a single step in each of eight directions. Two deliberate gaps:
/// no castling yet, and no check-safety -- a king may be generated onto an
/// attacked square. Both are resolved elsewhere: castling by a future generator,
/// check-safety by `Board::generate_legal_moves`, which removes those moves.
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
