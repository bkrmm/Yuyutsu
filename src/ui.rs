//! Terminal user interface: command parsing, session state, and presentation.
//!
//! `main` owns stdin and stdout; this module owns everything that can be tested
//! without a terminal. A line becomes a `Command`, `Session` applies it to a
//! `Game`, and an `Action` describes what the caller should print. Keeping those
//! three steps separate means the REPL's rules are integration-testable even
//! though the input loop itself is not.
//!
//! Moves use UCI coordinate notation: `e2e4` for an ordinary move, `e7e8q` for a
//! promotion, and `e1g1` for castling. Castling is written as the king's move,
//! because the engine derives castling from the king moving two files.

use std::fmt;

use crate::board::{Color, PieceType, Undo};
use crate::game::{DrawReason, Game, GameState};
use crate::moves::Move;

/// A command parsed from one line of terminal input.
///
/// Technically: commands and moves share one namespace, so a line either matches
/// a command word or is parsed as coordinate notation. `Copy` is possible because
/// every payload is a `Move`, which is itself `Copy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Move(Move),
    Help,
    Board,
    Legal,
    State,
    Undo,
    New,
    Quit,
}

/// Why one line could not be executed.
///
/// Technically: this covers both syntax and game state. `parse_command` can only
/// produce the syntax variants; `Session::execute` adds the state-dependent ones
/// (`IllegalMove`, `PromotionRequired`, `GameOver`, `NothingToUndo`, and the
/// defensive `EngineRejected`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputError {
    Empty,
    UnknownCommand(String),
    BadMoveSyntax(String),
    BadPromotion(char),
    PromotionRequired(Move),
    IllegalMove(Move),
    GameOver(GameState),
    NothingToUndo,
    EngineRejected(Move),
}

/// What the caller should do after a command has been executed.
///
/// Technically: `Session::execute` mutates the game but prints nothing. The
/// binary maps each variant to output, while tests assert on the variant. That
/// keeps formatting decisions out of the session and makes the REPL testable
/// without capturing stdout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Played(Move),
    Undone,
    NewGame,
    Board,
    Legal(Vec<Move>),
    State(GameState),
    Help,
    Quit,
    Error(InputError),
}

/// A game plus the undo records needed to take moves back.
///
/// Technically: `Game` deliberately owns only the repetition history, not the
/// `Undo` receipts from each move. The terminal needs an undo stack, so `Session`
/// stores the `Undo` returned by `Game::play` and feeds it back to
/// `Game::unplay`. The two stacks therefore stay in lockstep by construction.
#[derive(Debug, Clone)]
pub struct Session {
    game: Game,
    undos: Vec<Undo>,
}

impl Default for Session {
    /// Creates a session in the standard starting position.
    ///
    /// Technically: delegates to `new` so there is a single construction path.
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Creates a session in the standard starting position.
    ///
    /// Technically: wraps `Game::new` with an empty undo stack.
    pub fn new() -> Self {
        Self::from_game(Game::new())
    }

    /// Creates a session around an arbitrary game, with no moves to undo.
    ///
    /// Technically: a FEN or hand-built position has no `Undo` receipts, so the
    /// history starts empty even though the board may not be the start position.
    /// This is also the testing entry point for custom positions.
    pub fn from_game(game: Game) -> Self {
        Session {
            game,
            undos: Vec::new(),
        }
    }

    /// Returns the current game so the caller can render the board and state.
    ///
    /// Technically: immutable. Mutation goes through `execute`, which keeps the
    /// undo stack and the game's repetition history consistent.
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// Parses and executes one line of input.
    ///
    /// Technically: this is the whole REPL vocabulary in one place. Parsing and
    /// execution are separate because syntax depends only on the text, while
    /// legality and terminal state depend on the current game.
    pub fn execute(&mut self, input: &str) -> Action {
        match parse_command(input) {
            Ok(Command::Move(mv)) => self.play(mv),
            Ok(Command::Help) => Action::Help,
            Ok(Command::Board) => Action::Board,
            Ok(Command::Legal) => Action::Legal(self.game.legal_moves()),
            Ok(Command::State) => Action::State(self.game.state()),
            Ok(Command::Undo) => self.undo(),
            Ok(Command::New) => self.new_game(),
            Ok(Command::Quit) => Action::Quit,
            Err(error) => Action::Error(error),
        }
    }

    /// Plays a move if it is legal in the current position.
    ///
    /// Technically: checks the generated legal list rather than calling
    /// `Game::play` directly. `Board::make_move` validates geometry, turn order,
    /// and castling safety, but the generic "does this expose my own king?" test
    /// lives in `generate_legal_moves`. Without this membership check the terminal
    /// would accept moves that leave the mover in check.
    ///
    /// Terminal states are refused before the move is considered, because a human
    /// UI should not let play continue past checkmate, stalemate, or a declared
    /// draw. Search can still use `Game::play` directly; that freedom belongs to
    /// the engine layer, not the interface.
    fn play(&mut self, mv: Move) -> Action {
        let state = self.game.state();
        if !matches!(state, GameState::Ongoing) {
            return Action::Error(InputError::GameOver(state));
        }

        let legal = self.game.legal_moves();
        if !legal.contains(&mv) {
            // A bare pawn push to the last rank is syntactically valid but has
            // no matching legal move: the generator emits one move per promotion
            // piece. Detecting that case gives a better message than "illegal".
            let missing_promotion = mv.promotion.is_none()
                && legal
                    .iter()
                    .any(|candidate| candidate.from == mv.from && candidate.to == mv.to);

            return Action::Error(if missing_promotion {
                InputError::PromotionRequired(mv)
            } else {
                InputError::IllegalMove(mv)
            });
        }

        match self.game.play(&mv) {
            Ok(undo) => {
                self.undos.push(undo);
                Action::Played(mv)
            }
            // The move came from the legal list, so rejection means the generator
            // and validator disagree. Report it instead of panicking the REPL; the
            // agreement tests are the place where that defect should fail loudly.
            Err(_) => Action::Error(InputError::EngineRejected(mv)),
        }
    }

    /// Takes back the most recent move.
    ///
    /// Technically: pops the paired `Undo` and asks `Game` to reverse it, which
    /// also removes the corresponding repetition-history entry. An empty stack is
    /// a user error, not a panic.
    fn undo(&mut self) -> Action {
        let Some(undo) = self.undos.pop() else {
            return Action::Error(InputError::NothingToUndo);
        };

        self.game.unplay(&undo);
        Action::Undone
    }

    /// Starts a fresh standard game and clears the undo stack.
    ///
    /// Technically: replaces the whole `Game`, so repetition history and the
    /// halfmove clock reset together. Clearing `undos` is essential: receipts
    /// from the old game cannot be applied to the new one.
    fn new_game(&mut self) -> Action {
        self.game = Game::new();
        self.undos.clear();
        Action::NewGame
    }
}

impl fmt::Display for InputError {
    /// Formats the error as a lowercase sentence fragment for `error: {error}`.
    ///
    /// Technically: messages name the offending input and, where useful, the
    /// command that will show the user the way forward. The trailing period is
    /// supplied by the caller so display code owns punctuation, not the parser.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InputError::Empty => write!(f, "type a command or a move; type `help` for the list"),
            InputError::UnknownCommand(text) => {
                write!(f, "unknown command `{text}`; type `help` for the list")
            }
            InputError::BadMoveSyntax(text) => {
                write!(f, "`{text}` is not a coordinate move like e2e4")
            }
            InputError::BadPromotion(ch) => {
                write!(f, "`{ch}` is not a promotion piece; use q, r, b, or n")
            }
            InputError::PromotionRequired(mv) => {
                write!(f, "`{}` needs a promotion piece: q, r, b, or n", format_move(mv))
            }
            InputError::IllegalMove(mv) => write!(
                f,
                "`{}` is not legal here; type `legal` to list legal moves",
                format_move(mv)
            ),
            InputError::GameOver(state) => write!(
                f,
                "the game is over ({}); type `undo`, `new`, or `quit`",
                state_label(*state)
            ),
            InputError::NothingToUndo => write!(f, "there is no move to undo"),
            InputError::EngineRejected(mv) => write!(
                f,
                "internal error: the engine rejected `{}` after listing it as legal",
                format_move(mv)
            ),
        }
    }
}

/// Parses one line into a command or a coordinate move.
///
/// Technically: syntax only; it never looks at the board. Command words are
/// case-insensitive and may be abbreviated. A line that is not a command is
/// parsed as a move, and a purely alphabetic failure is reported as an unknown
/// command rather than a malformed move, which is friendlier for typos like
/// `resign`.
pub fn parse_command(input: &str) -> Result<Command, InputError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(InputError::Empty);
    }

    let lowered = trimmed.to_ascii_lowercase();
    match lowered.as_str() {
        "help" | "h" | "?" => Ok(Command::Help),
        "board" | "b" => Ok(Command::Board),
        "legal" | "l" | "moves" => Ok(Command::Legal),
        "state" | "s" => Ok(Command::State),
        "undo" | "u" => Ok(Command::Undo),
        "new" | "n" => Ok(Command::New),
        "quit" | "q" | "exit" => Ok(Command::Quit),
        _ => match parse_move(trimmed) {
            Ok(mv) => Ok(Command::Move(mv)),
            Err(InputError::BadMoveSyntax(_))
                if trimmed.chars().all(|ch| ch.is_ascii_alphabetic()) =>
            {
                Err(InputError::UnknownCommand(trimmed.to_string()))
            }
            Err(error) => Err(error),
        },
    }
}

/// Parses coordinate notation into a `Move`, without checking legality.
///
/// Technically: accepts exactly four characters (`e2e4`) or five with a
/// promotion suffix (`e7e8q`). Input is lowercased first so `E7E8Q` works. The
/// promotion piece is the only part that can be wrong in a different way from
/// the squares, hence the dedicated `BadPromotion` error.
pub fn parse_move(input: &str) -> Result<Move, InputError> {
    let trimmed = input.trim();
    let lowered = trimmed.to_ascii_lowercase();
    let bytes = lowered.as_bytes();

    if bytes.len() != 4 && bytes.len() != 5 {
        return Err(InputError::BadMoveSyntax(trimmed.to_string()));
    }

    let Some(from) = parse_square(bytes[0], bytes[1]) else {
        return Err(InputError::BadMoveSyntax(trimmed.to_string()));
    };
    let Some(to) = parse_square(bytes[2], bytes[3]) else {
        return Err(InputError::BadMoveSyntax(trimmed.to_string()));
    };

    let promotion = match bytes.get(4).copied() {
        None => None,
        Some(b'q') => Some(PieceType::Queen),
        Some(b'r') => Some(PieceType::Rook),
        Some(b'b') => Some(PieceType::Bishop),
        Some(b'n') => Some(PieceType::Knight),
        Some(other) => return Err(InputError::BadPromotion(other as char)),
    };

    Ok(Move {
        from,
        to,
        promotion,
    })
}

/// Converts one file byte and one rank byte into board coordinates.
///
/// Technically: files are `a..h` and ranks are `1..8`. The row is inverted
/// because row 0 is rank 8, matching the board's storage convention. Returning
/// `Option` keeps validation local; the caller decides which input error to
/// report.
fn parse_square(file: u8, rank: u8) -> Option<(usize, usize)> {
    if !(b'a'..=b'h').contains(&file) || !(b'1'..=b'8').contains(&rank) {
        return None;
    }

    let column = (file - b'a') as usize;
    let row = (b'8' - rank) as usize;
    Some((row, column))
}

/// Formats a `Move` back into coordinate notation.
///
/// Technically: the inverse of `parse_move` for legal moves. Castling remains a
/// king move and en passant remains an ordinary diagonal pawn move, because the
/// engine derives both from the squares.
pub fn format_move(mv: &Move) -> String {
    let mut text = format!("{}{}", format_square(mv.from), format_square(mv.to));
    if let Some(piece) = mv.promotion {
        text.push(promotion_letter(piece));
    }
    text
}

/// Converts board coordinates into a square name such as `e4`.
///
/// Technically: handles only `0..8` coordinates; an out-of-range square is
/// formatted as `(row,column)` instead of panicking. Every move produced by the
/// engine is in range, but formatting should not be the place a bad test fixture
/// crashes.
fn format_square((row, column): (usize, usize)) -> String {
    if row >= 8 || column >= 8 {
        return format!("({row},{column})");
    }

    let file = (b'a' + column as u8) as char;
    let rank = (b'8' - row as u8) as char;
    format!("{file}{rank}")
}

/// Returns the lowercase suffix used for a promotion piece.
///
/// Technically: only queen, rook, bishop, and knight are legal promotions. King
/// and pawn still get distinct letters so `format_move` is total; the parser and
/// validator are the layers that reject them.
fn promotion_letter(piece: PieceType) -> char {
    match piece {
        PieceType::Queen => 'q',
        PieceType::Rook => 'r',
        PieceType::Bishop => 'b',
        PieceType::Knight => 'n',
        PieceType::King => 'k',
        PieceType::Pawn => 'p',
    }
}

/// Describes a game state in words suitable for the terminal.
///
/// Technically: `GameState` is a domain enum; this mapping is presentation. It
/// deliberately spells out the draw reason instead of printing debug output.
pub fn state_label(state: GameState) -> String {
    match state {
        GameState::Ongoing => "ongoing".to_string(),
        GameState::Checkmate => "checkmate".to_string(),
        GameState::Stalemate => "stalemate".to_string(),
        GameState::Draw(DrawReason::FiftyMove) => "draw by fifty-move rule".to_string(),
        GameState::Draw(DrawReason::ThreefoldRepetition) => {
            "draw by threefold repetition".to_string()
        }
        GameState::Draw(DrawReason::InsufficientMaterial) => {
            "draw by insufficient material".to_string()
        }
    }
}

/// Returns the terminal's command reference.
///
/// Technically: a static string keeps help text out of the command parser. It
/// documents coordinate notation and the fact that both sides are entered by
/// hand, which is the one non-obvious limitation before search exists.
pub fn help_text() -> &'static str {
    "Commands:
  help, h, ?       show this help
  board, b         redraw the board
  legal, l, moves  list legal moves
  state, s         show the game state
  undo, u          take back the last move
  new, n           start a new game
  quit, q, exit    leave

Moves use coordinate notation:
  e2e4             move from e2 to e4
  e7e8q            promote on the last rank (q, r, b, or n)
  e1g1, e8g8      castle kingside
  e1c1, e8c8      castle queenside

Both sides are entered by hand. There is no engine opponent yet."
}

/// Returns the human-readable name of a side.
///
/// Technically: presentation belongs here rather than in the engine, so `Board`
/// and `Game` do not need to know how the terminal spells a color.
pub fn color_name(color: Color) -> &'static str {
    match color {
        Color::White => "White",
        Color::Black => "Black",
    }
}
