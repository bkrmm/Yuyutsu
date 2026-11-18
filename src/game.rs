//! Game-level state: repetition history and terminal outcomes.
//!
//! `Board` is deliberately a single position so perft and search can make and
//! unmake moves without copying a history. `Game` wraps a board with the
//! position keys needed for threefold repetition and answers the question
//! "what is the state of this game?".

use crate::board::{Board, InvalidMove, PositionKey, Undo, in_check};
use crate::moves::Move;

/// Why a position is drawn.
///
/// Technically: threefold repetition and the fifty-move rule are claimable
/// under FIDE, while insufficient material is a dead position. This engine
/// reports all three as terminal when `Game::state` is asked; claim handling is
/// a user-interface concern, not a search concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawReason {
    FiftyMove,
    ThreefoldRepetition,
    InsufficientMaterial,
}

/// The state of a game at its current position.
///
/// Technically: `Checkmate` and `Stalemate` are checked before the draw rules,
/// because a checkmating move ends the game immediately even if the halfmove
/// clock or repetition count would also qualify. `Draw` carries the reason so a
/// caller can report it without re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameState {
    Ongoing,
    Checkmate,
    Stalemate,
    Draw(DrawReason),
}

/// A board plus the position history needed for threefold repetition.
///
/// Technically: `Board` stays a pure position so perft and search can make and
/// unmake moves without carrying a `Vec`. `Game::play` makes the move and then
/// records the resulting position key; `Game::unplay` pops that key and reverses
/// the move. The two operations must be paired, exactly like
/// `Board::make_move`/`Board::unmake_move`.
///
/// `play` does not reject moves after a draw or checkmate. Terminal state is a
/// query (`state`), not a gate, because a search must be able to explore
/// positions past a claimable draw if its caller chooses to.
#[derive(Debug, Clone)]
pub struct Game {
    board: Board,
    /// One key per position, including the initial and current positions.
    keys: Vec<PositionKey>,
}

impl Default for Game {
    /// Creates the standard starting position.
    ///
    /// Technically: delegates to `new` so there is one construction path for the
    /// initial position and its first history entry.
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// Creates a game in the standard starting position.
    ///
    /// Technically: wraps `Board::new()` and seeds the history with its key, so
    /// the initial position counts as the first occurrence for repetition.
    pub fn new() -> Self {
        Self::from_board(Board::new())
    }

    /// Creates a game from an arbitrary position, with the position as the first
    /// entry in its repetition history.
    ///
    /// Technically: a FEN does not carry the moves that produced it, so the
    /// caller cannot supply earlier occurrences. The game starts counting from
    /// here; repeated positions before this point are unknowable.
    pub fn from_board(board: Board) -> Self {
        let key = board.position_key();
        Game { board, keys: vec![key] }
    }

    /// Returns the current position.
    ///
    /// Technically: immutable, so callers cannot make a move that bypasses the
    /// repetition history. Use `play` for mutation.
    pub fn board(&self) -> &Board {
        &self.board
    }

    /// Returns every legal move in the current position.
    ///
    /// Technically: `Board::generate_legal_moves` needs `&mut self` because it
    /// plays and reverts each candidate to test for self-check, while `Game`
    /// deliberately exposes the board immutably. Cloning the board for the
    /// duration of the call keeps the game's history and clock untouched and
    /// costs one 64-square copy — irrelevant outside a hot search loop, which
    /// will call the board layer directly.
    pub fn legal_moves(&self) -> Vec<Move> {
        let mut probe = self.board.clone();
        probe.generate_legal_moves()
    }

    /// Plays a move and records the resulting position.
    ///
    /// Technically: validates and applies the move through `Board::make_move`,
    /// then pushes the new position key. On error the board and history are both
    /// untouched, because `make_move` validates before mutating and the push is
    /// after the successful call.
    pub fn play(&mut self, mv: &Move) -> Result<Undo, InvalidMove> {
        let undo = self.board.make_move(mv)?;
        self.keys.push(self.board.position_key());
        Ok(undo)
    }

    /// Reverses a move played by `play` and removes its history entry.
    ///
    /// Technically: the inverse of `play`. It checks in debug builds that the
    /// recorded current key matches the board before popping, which catches a
    /// caller pairing `unplay` with the wrong `Undo`; release builds keep only
    /// the mechanical pop and unmake.
    pub fn unplay(&mut self, undo: &Undo) {
        debug_assert_eq!(
            self.keys.last().copied(),
            Some(self.board.position_key()),
            "unplay called with a stale undo"
        );

        self.keys.pop();
        self.board.unmake_move(undo);
    }

    /// Returns whether the current position has occurred at least three times.
    ///
    /// Technically: counts matching structural keys in the full history. Keys
    /// from before an irreversible move cannot match the current position --
    /// a pawn move, capture, or castling-rights loss changes the key -- so
    /// scanning all of them is correct, just not maximally fast. The current
    /// position is the last key and must be counted; two occurrences are not a
    /// draw.
    pub fn is_threefold_repetition(&self) -> bool {
        let Some(current) = self.keys.last() else { return false };
        self.keys.iter().filter(|key| *key == current).count() >= 3
    }

    /// Returns the state of the current position.
    ///
    /// Technically: legal moves are generated first, because no-move positions
    /// take precedence over every draw rule. A checkmating move wins even on the
    /// hundredth halfmove or the third occurrence. After mate/stalemate, the
    /// checks run cheapest first: material, then the halfmove clock, then the
    /// repetition scan. Legality filtering mutates only a cloned probe board, so
    /// this query is `&self` and leaves the game and its history unchanged.
    pub fn state(&self) -> GameState {
        let legal = self.legal_moves();

        if legal.is_empty() {
            return if in_check(&self.board, self.board.side_to_move) {
                GameState::Checkmate
            } else {
                GameState::Stalemate
            };
        }

        if self.board.is_insufficient_material() {
            return GameState::Draw(DrawReason::InsufficientMaterial);
        }

        if self.board.is_fifty_move_draw() {
            return GameState::Draw(DrawReason::FiftyMove);
        }

        if self.is_threefold_repetition() {
            return GameState::Draw(DrawReason::ThreefoldRepetition);
        }

        GameState::Ongoing
    }
}
