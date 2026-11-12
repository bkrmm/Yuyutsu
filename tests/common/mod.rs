//! Helpers shared by the integration tests.
//!
//! Cargo compiles every top-level file in `tests/` as its own crate, so code
//! shared between them lives in this subdirectory module and each test file
//! declares it with `mod common;`. Being in a subdirectory is what stops Cargo
//! from also building it as a test binary of its own.

// Each test crate uses a different subset of these helpers, so the unused ones
// in any given compilation are expected rather than a defect.
#![allow(dead_code)]

use yuyutsu::board::{Board, CastlingRights, Color, Piece, PieceType};

/// Builds a piece literal.
pub fn piece(color: Color, kind: PieceType) -> Piece {
    Piece { color, kind }
}

/// Builds a sparse position from `(square, piece)` pairs plus a side to move.
///
/// Technically: starts from `Board::empty()` so castling rights default to none
/// and `en_passant` to absent; a test that needs either sets it explicitly. This
/// is deliberate -- a hand-built position should not silently inherit the
/// starting position's rights.
pub fn position(pieces: &[((usize, usize), Piece)], side_to_move: Color) -> Board {
    let mut board = Board::empty();
    for &(sq, p) in pieces {
        board.grid[sq.0][sq.1] = Some(p);
    }
    board.side_to_move = side_to_move;
    board
}

/// Parses algebraic square text such as `"e3"` into `(row, column)`.
pub fn parse_square(text: &str) -> (usize, usize) {
    let bytes = text.as_bytes();
    let col = (bytes[0] - b'a') as usize;
    let rank = (bytes[1] - b'0') as usize;
    (8 - rank, col)
}

/// Builds a board from a FEN string.
///
/// Technically: parses the four fields the engine currently models -- placement,
/// side to move, castling rights and the en passant square. Halfmove clock and
/// fullmove number are accepted and ignored because `Board` does not track them
/// yet. Ranks are `/`-separated from rank 8 down, which matches row 0 upward.
pub fn board_from_fen(fen: &str) -> Board {
    let mut fields = fen.split_whitespace();

    let placement = fields.next().expect("fen: placement");
    let side = fields.next().expect("fen: side to move");
    let castling = fields.next().expect("fen: castling");
    let en_passant = fields.next().expect("fen: en passant");

    let mut board = Board::empty();

    for (row, rank_text) in placement.split('/').enumerate() {
        let mut col = 0usize;
        for ch in rank_text.chars() {
            if let Some(skip) = ch.to_digit(10) {
                col += skip as usize;
                continue;
            }

            let color = if ch.is_ascii_uppercase() { Color::White } else { Color::Black };
            let kind = match ch.to_ascii_lowercase() {
                'p' => PieceType::Pawn,
                'n' => PieceType::Knight,
                'b' => PieceType::Bishop,
                'r' => PieceType::Rook,
                'q' => PieceType::Queen,
                'k' => PieceType::King,
                other => panic!("unknown piece {other}"),
            };

            board.grid[row][col] = Some(Piece { color, kind });
            col += 1;
        }
        assert_eq!(col, 8, "rank {row} did not describe eight files");
    }

    board.side_to_move = match side {
        "w" => Color::White,
        "b" => Color::Black,
        other => panic!("unknown side {other}"),
    };

    board.castling = CastlingRights::NONE;
    for flag in castling.chars() {
        match flag {
            'K' => board.castling.white_kingside = true,
            'Q' => board.castling.white_queenside = true,
            'k' => board.castling.black_kingside = true,
            'q' => board.castling.black_queenside = true,
            '-' => {}
            other => panic!("unknown castling flag {other}"),
        }
    }

    board.en_passant = match en_passant {
        "-" => None,
        sq => Some(parse_square(sq)),
    };

    board
}

/// Counts legal move sequences of exactly `depth` plies starting from `board`.
///
/// Technically: depth 0 is the leaf case and counts 1. Bulk counting at depth 1
/// returns the legal move count instead of recursing, which yields the same leaf
/// count while skipping a make/unmake pair per leaf. Every recursive step is
/// paired with an unmake, so the caller's board is unchanged on return.
pub fn perft(board: &mut Board, depth: u32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let moves = board.generate_legal_moves();
    if depth == 1 {
        return moves.len() as u64;
    }

    let mut nodes = 0;
    for mv in moves {
        let undo = board.make_move(&mv).expect("legal move rejected by make_move");
        nodes += perft(board, depth - 1);
        board.unmake_move(&undo);
    }

    nodes
}
