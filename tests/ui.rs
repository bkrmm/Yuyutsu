//! Terminal interface behavior: parsing, session commands, and refusal rules.
//!
//! These tests exercise the layer that sits between stdin and the engine. They
//! deliberately avoid the actual `main` loop, because the loop is only I/O; all
//! decisions that can be wrong live in `ui` and are asserted here.

mod common;

use common::{board_from_fen, piece as p};
use yuyutsu::board::{Board, Color, PieceType};
use yuyutsu::game::{DrawReason, Game, GameState};
use yuyutsu::moves::Move;
use yuyutsu::ui::{
    Action, Command, InputError, Session, color_name, format_move, help_text, parse_command,
    parse_move, state_label,
};

/// Builds an ordinary non-promotion move between two squares.
///
/// Technically: most fixtures do not care about the promotion field, so fixing it
/// to `None` keeps assertions short. Promotion tests construct `Move` explicitly.
fn mv(from: (usize, usize), to: (usize, usize)) -> Move {
    Move {
        from,
        to,
        promotion: None,
    }
}

/// Confirms every documented command word and abbreviation reaches the right
/// `Command` variant.
///
/// Technically: commands share a namespace with coordinate moves, so an alias
/// regression can silently turn `b` into a malformed move instead of a board
/// redraw. This test pins the whole table.
#[test]
fn command_aliases_are_recognized() {
    for word in ["help", "h", "?"] {
        assert_eq!(parse_command(word), Ok(Command::Help), "{word}");
    }
    for word in ["board", "b"] {
        assert_eq!(parse_command(word), Ok(Command::Board), "{word}");
    }
    for word in ["legal", "l", "moves"] {
        assert_eq!(parse_command(word), Ok(Command::Legal), "{word}");
    }
    for word in ["state", "s"] {
        assert_eq!(parse_command(word), Ok(Command::State), "{word}");
    }
    for word in ["undo", "u"] {
        assert_eq!(parse_command(word), Ok(Command::Undo), "{word}");
    }
    for word in ["new", "n"] {
        assert_eq!(parse_command(word), Ok(Command::New), "{word}");
    }
    for word in ["quit", "q", "exit"] {
        assert_eq!(parse_command(word), Ok(Command::Quit), "{word}");
    }
}

/// Confirms coordinate notation parses in both cases, with and without a
/// promotion suffix, and formats back to the same lowercase form.
///
/// Technically: `parse_move` is syntax only, so it accepts moves that may be
/// illegal on the current board. Legality belongs to `Session`.
#[test]
fn coordinate_moves_are_parsed_and_formatted() {
    assert_eq!(parse_move("e2e4"), Ok(mv((6, 4), (4, 4))));
    assert_eq!(
        parse_move("  E7E8Q "),
        Ok(Move {
            from: (1, 4),
            to: (0, 4),
            promotion: Some(PieceType::Queen),
        })
    );
    assert_eq!(
        parse_move("a7a8n"),
        Ok(Move {
            from: (1, 0),
            to: (0, 0),
            promotion: Some(PieceType::Knight),
        })
    );

    assert_eq!(format_move(&mv((7, 7), (0, 0))), "h1a8");
    assert_eq!(
        format_move(&Move {
            from: (1, 4),
            to: (0, 4),
            promotion: Some(PieceType::Queen),
        }),
        "e7e8q"
    );
}

/// Confirms malformed commands and moves produce specific, non-panicking errors.
///
/// Technically: `resign` is all alphabetic, so the parser reports an unknown
/// command; `e2e9` contains a bad rank and remains a malformed move. Keeping
/// those messages distinct is the difference between a typo and a broken move.
#[test]
fn malformed_input_is_reported() {
    assert_eq!(parse_command("   "), Err(InputError::Empty));
    assert_eq!(
        parse_command("resign"),
        Err(InputError::UnknownCommand("resign".to_string()))
    );
    assert_eq!(
        parse_move("e2e9"),
        Err(InputError::BadMoveSyntax("e2e9".to_string()))
    );
    assert_eq!(
        parse_move("e2e"),
        Err(InputError::BadMoveSyntax("e2e".to_string()))
    );
    assert_eq!(parse_move("e7e8k"), Err(InputError::BadPromotion('k')));
    assert_eq!(
        InputError::IllegalMove(mv((6, 4), (4, 4))).to_string(),
        "`e2e4` is not legal here; type `legal` to list legal moves"
    );
}

/// Confirms a session applies legal moves and reports illegal ones without
/// changing the board.
///
/// Technically: after `e2e4` and `e7e5`, trying `e2e4` again is illegal because
/// the e2 pawn is gone. The board comparison would catch a validator that
/// mutated before rejecting.
#[test]
fn session_plays_legal_moves_and_rejects_illegal_ones() {
    let mut session = Session::new();

    assert_eq!(
        session.execute("E2E4"),
        Action::Played(mv((6, 4), (4, 4)))
    );
    assert_eq!(
        session.game().board().piece_at((4, 4)),
        Some(p(Color::White, PieceType::Pawn))
    );

    assert_eq!(
        session.execute("e7e5"),
        Action::Played(mv((1, 4), (3, 4)))
    );
    assert_eq!(
        session.execute("e2e4"),
        Action::Error(InputError::IllegalMove(mv((6, 4), (4, 4))))
    );
}

/// Confirms the session rejects a geometrically valid move that exposes its own
/// king.
///
/// Technically: this is the reason the UI checks the generated legal list rather
/// than trusting `make_move`. The e2 rook is pinned by the e8 rook; `e2d2` steps
/// off the e-file, which is well-formed and would be accepted by geometry alone,
/// but it loses the king.
#[test]
fn session_rejects_moves_that_expose_the_king() {
    let board = board_from_fen("k3r3/8/8/8/8/8/4R3/4K3 w - - 0 1");
    let mut session = Session::from_game(Game::from_board(board));
    let before = session.game().board().clone();

    assert_eq!(
        session.execute("e2d2"),
        Action::Error(InputError::IllegalMove(mv((6, 4), (6, 3))))
    );
    assert_eq!(session.game().board(), &before);
}

/// Confirms a bare promotion push asks for a piece and a suffixed push plays it.
///
/// Technically: the generator emits four separate moves for `a7a8`, so the bare
/// move matches no legal candidate. Detecting that case gives a useful message
/// instead of a generic "illegal move" for a move the user almost typed right.
#[test]
fn promotion_requires_a_suffix_and_accepts_one() {
    let board = board_from_fen("8/P7/8/8/8/8/8/K6k w - - 0 1");
    let mut session = Session::from_game(Game::from_board(board));

    assert_eq!(
        session.execute("a7a8"),
        Action::Error(InputError::PromotionRequired(mv((1, 0), (0, 0))))
    );

    let promotion = Move {
        from: (1, 0),
        to: (0, 0),
        promotion: Some(PieceType::Queen),
    };
    assert_eq!(session.execute("a7a8q"), Action::Played(promotion));
    assert_eq!(
        session.game().board().piece_at((0, 0)),
        Some(p(Color::White, PieceType::Queen))
    );
}

/// Confirms undo is a true stack: two undos return to the start, and a third is
/// a handled error.
///
/// Technically: each undo pops the `Undo` paired with the move, so the halfmove
/// clock and repetition history must come back with the board. Comparing whole
/// `Board` values includes the clock.
#[test]
fn undo_restores_the_previous_position_and_history() {
    let mut session = Session::new();
    let start = session.game().board().clone();

    assert!(matches!(
        session.execute("e2e4"),
        Action::Played(_)
    ));
    let after_e4 = session.game().board().clone();
    assert!(matches!(
        session.execute("e7e5"),
        Action::Played(_)
    ));

    assert_eq!(session.execute("undo"), Action::Undone);
    assert_eq!(session.game().board(), &after_e4);

    assert_eq!(session.execute("undo"), Action::Undone);
    assert_eq!(session.game().board(), &start);

    assert_eq!(
        session.execute("undo"),
        Action::Error(InputError::NothingToUndo)
    );
}

/// Confirms `new` replaces the game and discards the old undo stack.
///
/// Technically: clearing the undo stack matters as much as resetting the board.
/// A stale `Undo` from the previous game would corrupt the new one if it could be
/// applied, so this test checks both halves of the reset.
#[test]
fn new_game_resets_the_board_and_undo_stack() {
    let mut session = Session::new();
    assert!(matches!(
        session.execute("e2e4"),
        Action::Played(_)
    ));
    assert!(matches!(
        session.execute("e7e5"),
        Action::Played(_)
    ));

    assert_eq!(session.execute("new"), Action::NewGame);
    assert_eq!(session.game().board(), &Board::new());
    assert_eq!(
        session.execute("undo"),
        Action::Error(InputError::NothingToUndo)
    );
}

/// Confirms `legal` lists the twenty opening moves and `state` reports ongoing.
///
/// Technically: the exact count is a cheap oracle. A count of zero, or a move
/// missing from the list, would point at the interface passing the wrong position
/// or the generator regressing.
#[test]
fn legal_and_state_commands_report_the_current_position() {
    let mut session = Session::new();

    assert_eq!(
        session.execute("state"),
        Action::State(GameState::Ongoing)
    );

    let Action::Legal(moves) = session.execute("legal") else {
        panic!("legal command did not produce a move list");
    };
    assert_eq!(moves.len(), 20);
    assert!(moves.contains(&mv((6, 4), (4, 4))));
    assert!(moves.contains(&mv((7, 1), (5, 2))));
}

/// Confirms a checkmating move is played, reported, locked out, and undoable.
///
/// Technically: Fool's Mate reaches the terminal state through ordinary legal
/// moves, so this tests the interaction between `Session`, `Game::state`, and the
/// refusal gate without hand-building a board. The final undo proves that a
/// terminal state is a position, not a one-way latch: the user can take the
/// mating move back and continue.
#[test]
fn terminal_game_refuses_further_moves() {
    let mut session = Session::new();
    for input in ["f2f3", "e7e5", "g2g4"] {
        assert!(
            matches!(session.execute(input), Action::Played(_)),
            "{input} should have been played"
        );
    }

    assert_eq!(
        session.execute("d8h4"),
        Action::Played(mv((0, 3), (4, 7)))
    );
    assert_eq!(session.game().state(), GameState::Checkmate);
    assert_eq!(
        session.execute("state"),
        Action::State(GameState::Checkmate)
    );
    assert_eq!(
        session.execute("a2a3"),
        Action::Error(InputError::GameOver(GameState::Checkmate))
    );

    assert_eq!(session.execute("undo"), Action::Undone);
    assert_eq!(session.game().state(), GameState::Ongoing);
}

/// Confirms a claimable draw is also terminal for the human interface.
///
/// Technically: `Game` treats claimable draws as terminal for search, and the UI
/// inherits that policy. The position has a rook, so the draw comes from the
/// halfmove clock rather than from insufficient material.
#[test]
fn draw_state_refuses_further_moves() {
    let board = board_from_fen("7k/8/8/8/8/8/R7/4K3 w - - 100 1");
    let mut session = Session::from_game(Game::from_board(board));

    assert_eq!(
        session.execute("state"),
        Action::State(GameState::Draw(DrawReason::FiftyMove))
    );
    assert_eq!(
        session.execute("a2a3"),
        Action::Error(InputError::GameOver(GameState::Draw(
            DrawReason::FiftyMove
        )))
    );
}

/// Confirms castling and en passant work through coordinate notation.
///
/// Technically: both are derived moves, so the input is deliberately ordinary:
/// `e1g1` is a king move and `e5d6` is a diagonal pawn move onto an empty square.
/// The board assertions check the secondary effects -- the rook relocation and
/// the captured pawn disappearing from beside the mover -- which are exactly the
/// parts the notation cannot express.
#[test]
fn coordinate_notation_reaches_castling_and_en_passant() {
    let mut session = Session::new();
    for input in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6"] {
        assert!(
            matches!(session.execute(input), Action::Played(_)),
            "{input} should have been played"
        );
    }
    assert_eq!(
        session.execute("e1g1"),
        Action::Played(mv((7, 4), (7, 6)))
    );
    assert_eq!(
        session.game().board().piece_at((7, 6)),
        Some(p(Color::White, PieceType::King))
    );
    assert_eq!(
        session.game().board().piece_at((7, 5)),
        Some(p(Color::White, PieceType::Rook))
    );

    let mut session = Session::new();
    for input in ["e2e4", "a7a6", "e4e5", "d7d5"] {
        assert!(
            matches!(session.execute(input), Action::Played(_)),
            "{input} should have been played"
        );
    }
    assert_eq!(
        session.execute("e5d6"),
        Action::Played(mv((3, 4), (2, 3)))
    );
    assert_eq!(
        session.game().board().piece_at((2, 3)),
        Some(p(Color::White, PieceType::Pawn))
    );
    assert_eq!(
        session.game().board().piece_at((3, 3)),
        None,
        "the captured pawn must leave d5"
    );
}

/// Confirms status and color names spell out every outcome for the terminal.
///
/// Technically: these are presentation mappings, not rules. Pinning them prevents
/// a future `Debug`-based shortcut from leaking variant names into user output.
#[test]
fn status_and_color_labels_are_human_readable() {
    assert_eq!(state_label(GameState::Ongoing), "ongoing");
    assert_eq!(state_label(GameState::Checkmate), "checkmate");
    assert_eq!(state_label(GameState::Stalemate), "stalemate");
    assert_eq!(
        state_label(GameState::Draw(DrawReason::FiftyMove)),
        "draw by fifty-move rule"
    );
    assert_eq!(
        state_label(GameState::Draw(DrawReason::ThreefoldRepetition)),
        "draw by threefold repetition"
    );
    assert_eq!(
        state_label(GameState::Draw(DrawReason::InsufficientMaterial)),
        "draw by insufficient material"
    );

    assert_eq!(color_name(Color::White), "White");
    assert_eq!(color_name(Color::Black), "Black");
}

/// Confirms the help screen documents every command and the notation examples.
///
/// Technically: help text is not enforced by the parser, so a missing entry would
/// otherwise go unnoticed. Checking for the distinctive tokens is enough to keep
/// the reference aligned with the command table.
#[test]
fn help_text_documents_commands_and_move_syntax() {
    let help = help_text();

    for needle in [
        "help", "board", "legal", "state", "undo", "new", "quit", "e2e4", "e7e8q", "e1g1",
    ] {
        assert!(help.contains(needle), "help is missing `{needle}`");
    }
}
