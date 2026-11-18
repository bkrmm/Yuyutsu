use std::io::{self, BufRead, Write};

use yuyutsu::board::in_check;
use yuyutsu::game::GameState;
use yuyutsu::moves::Move;
use yuyutsu::ui::{Action, Session, color_name, format_move, help_text, state_label};

/// Entry point: runs the terminal read-eval-print loop.
///
/// Technically: `main` owns the parts that touch the operating system -- reading
/// lines, printing prompts, and flushing stdout. All command interpretation and
/// game mutation live in `Session`, which is why the REPL rules can be tested
/// without a terminal. EOF (Ctrl-D) and an empty line both exit or continue
/// cleanly instead of being treated as moves.
fn main() {
    let mut session = Session::new();
    print_welcome();
    print_position(&session);

    let stdin = io::stdin();
    let mut lines = stdin.lock().lines();

    loop {
        print!("{} > ", color_name(session.game().board().side_to_move));
        if io::stdout().flush().is_err() {
            break;
        }

        let line = match lines.next() {
            Some(Ok(line)) => line,
            Some(Err(error)) => {
                eprintln!("could not read input: {error}");
                break;
            }
            None => {
                println!();
                break;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        match session.execute(&line) {
            Action::Played(mv) => {
                println!("played {}.", format_move(&mv));
                print_position(&session);
            }
            Action::Undone => {
                println!("move undone.");
                print_position(&session);
            }
            Action::NewGame => {
                println!("new game.");
                print_position(&session);
            }
            Action::Board => print_position(&session),
            Action::Legal(moves) => print_legal_moves(&moves),
            Action::State(state) => println!("State: {}.", state_label(state)),
            Action::Help => println!("{}", help_text()),
            Action::Quit => {
                println!("goodbye.");
                break;
            }
            Action::Error(error) => println!("error: {error}"),
        }
    }
}

/// Prints the opening banner and the one behavioural limitation that matters.
///
/// Technically: there is no search yet, so the terminal is a hot-seat board and
/// rule checker, not a game against an engine. Saying so up front prevents the
/// user from waiting for the program to move.
fn print_welcome() {
    println!("Yuyutsu — terminal chess");
    println!("No search yet: both sides are entered by hand.");
    println!("Type `help` for commands, `quit` to exit.");
}

/// Prints the board, the side to move, check status, and the game state.
///
/// Technically: `Board` already implements `Display`, so this function only adds
/// the information that lives outside the pure position. `Game::state` is called
/// again after every move so the heading is always derived from the same rules
/// the engine uses, not from terminal bookkeeping.
fn print_position(session: &Session) {
    let board = session.game().board();
    let state = session.game().state();

    println!();
    println!("{board}");
    println!();
    println!("{} to move.", color_name(board.side_to_move));
    if matches!(state, GameState::Ongoing) && in_check(board, board.side_to_move) {
        println!("Check.");
    }
    println!("State: {}.", state_label(state));
    println!();
}

/// Prints the legal move list in sorted coordinate notation.
///
/// Technically: the generator's order is an implementation detail, so sorting the
/// formatted strings makes repeated `legal` commands stable and makes a
/// particular move easy to find. An empty list is a terminal position, not an
/// error; the state display already explains which terminal state it is.
fn print_legal_moves(moves: &[Move]) {
    if moves.is_empty() {
        println!("no legal moves.");
        return;
    }

    let mut labels: Vec<String> = moves.iter().map(format_move).collect();
    labels.sort();
    println!("{} legal moves: {}", labels.len(), labels.join(" "));
}
