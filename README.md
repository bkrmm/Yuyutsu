# Yuyutsu

A chess engine written in Rust, working toward Monte Carlo Tree Search.

Yuyutsu was the one Kaurava who chose to fight for the side he judged right.

## Status

The rules layer is solid and verified. There is no search yet, so the engine does
not play chess — it can only enumerate and apply legal moves, then report the
resulting game state.

**Working**

- 8x8 board with piece placement and `side_to_move`
- Pseudo-legal move generation for all six piece types
- Full geometric validation on every move, returning a typed `InvalidMove` reason
- `make_move` / `unmake_move` as a matched pair driven by an `Undo` record
- Promotion to queen, rook, bishop, or knight, on pushes *and* captures
- Castling for both sides, with rights revoked by king moves, rook moves, and
  rook captures
- En passant, including the discovered-check case where the capture is
  geometrically valid but exposes the king
- Legality filtering — moves leaving the mover's own king attacked are removed
- Attack detection by scanning outward from a square (FIDE 3.1.3, so pinned
  pieces still count as attacking)
- Checkmate and stalemate are distinguishable (empty legal list, plus check)
- Halfmove clock maintained by `make_move` and restored exactly by `unmake_move`
- Fifty-move rule at 100 halfmoves, threefold repetition, and the standard
  insufficient-material set
- `Game` / `GameState` reporting ongoing, checkmate, stalemate, or a typed
  `DrawReason`, with checkmate and stalemate taking precedence

**Not implemented**

- The full FIDE claim procedure: claimable draws are reported as terminal, and
  automatic fivefold repetition / seventy-five-move draws are not distinguished
- Complete dead-position analysis; blocked-pawn positions are not detected
- Search, evaluation, MCTS
- Bitboards
- A user interface; `cargo run` is a smoke test that prints one move

## Getting started

```sh
cargo run                # print the legal move count, play one move, show the board
cargo test               # 41 tests: rules, agreement, perft to depth 4, special moves, draws
cargo test --release     # 43 tests: adds start-position perft 5-6 and Kiwipete 4
```

No dependencies. Rust 2024 edition.

## Layout

| Path | Contents |
| --- | --- |
| `src/board.rs` | Board, pieces, castling rights, en passant target, halfmove clock, `Undo`, `make_move`/`unmake_move`, validation, attack detection, legality filter, repetition key, material draw check |
| `src/game.rs` | `Game`, `GameState`, `DrawReason`, repetition history and terminal-state evaluation |
| `src/moves.rs` | `Move` and the per-piece pseudo-legal generators, including castling and en passant |
| `src/lib.rs` | Library root, so `tests/` can link the engine |
| `src/main.rs` | Smoke-test binary |
| `tests/common/mod.rs` | Shared FEN parser, position builder, and perft |
| `tests/agreement.rs` | Generator/validator agreement and make/unmake round trips |
| `tests/perft.rs` | Perft oracle for the start position and Kiwipete, plus targeted position tests |
| `tests/special_moves.rs` | Castling and en passant edge cases in isolation |
| `tests/draws.rs` | Halfmove clock, fifty-move, repetition identity, and insufficient material |

### Coordinates

Squares are `(row, column)`, both `0..8`, with **row 0 as rank 8** and **column 0
as the a-file**. So White's king starts at `(7, 4)` (e1) and Black's at `(0, 4)`
(e8). White advances by *decreasing* the row. This is the one convention worth
internalising before reading the generators.

## Design

Five decisions shape the codebase:

**Move generation is pure.** Generators borrow the board and return moves; they
never mutate. `make_move` is the only mutator, and it validates fully *before*
writing anything, so a rejected move provably leaves the board untouched.

**Special moves are derived, not stored.** `Move` is still only `{ from, to,
promotion }`. A king moving two files is a castle; a pawn moving diagonally onto
an empty square is an en passant capture. Both tests are exact, so no extra field
or parallel generator logic is needed. `Undo` is where the consequences are
recorded: the captured square (different from `to` for en passant), the rook's
source and destination (castling moves two pieces), the previous castling rights
and en passant target, and the previous halfmove clock.

**`Undo` carries what happened, not what was asked.** `Move` says "pawn to e8,
promote to queen"; `Undo` records the piece that actually moved, what was
captured, and every piece of bookkeeping the move disturbed. That makes
`unmake_move` a mechanical inverse containing no chess knowledge. A rule that
changes more than one piece or field must write all of it into `Undo` at make
time, or it cannot be reversed.

**Legality is a separate pass.** `generate_all_moves` is pseudo-legal: it ignores
whether a move exposes the king. `generate_legal_moves` filters it by playing each
candidate, asking `in_check`, and reverting. This is why make/unmake had to exist
before the filter could be written. Castling needs two extra gates before that
loop, because the king has already left the transit square by the time the board
can be inspected: `castling_is_forbidden` rejects castling out of check or across
an attacked square. Castling *into* check is caught by the ordinary
make-and-ask loop.

**Draw state is layered.** `Board` stays a pure position plus the halfmove clock,
so perft and search can make and unmake without copying a history. Repetition is
inherently historical, so `Game` owns a stack of `PositionKey`s alongside the
board. The key is exact, not a Zobrist hash: grid, side to move, castling rights,
and the en passant target *only when a legal capture exists*. Hashing would be
faster, but a collision here would silently turn a win into a draw. `GameState`
treats claimable draws as terminal and reports the reason; full FIDE claim
handling belongs to a UI, not to search.

### Attack detection

`is_attacked` scans outward from the target square instead of generating every
enemy move, so it allocates nothing and stops at the first blocker on each ray.
It implements the FIDE definition of "attacked" (3.1.3): a piece attacks a
square even when it is pinned and could not legally move there. That is the
correct definition for both check detection and the castling transit rule.

## Verification

`tests/perft.rs` checks exact published counts for two positions. Any deviation
points at a concrete defect in generation, legality filtering, or the
make/unmake pair.

Starting position:

| depth | nodes | runs in |
| --- | --- | --- |
| 1 | 20 | debug and release |
| 2 | 400 | debug and release |
| 3 | 8,902 | debug and release |
| 4 | 197,281 | debug and release |
| 5 | 4,865,609 | release only |
| 6 | 119,060,324 | release only |

Kiwipete — the standard castling and en passant oracle:

| depth | nodes | runs in |
| --- | --- | --- |
| 1 | 48 | debug and release |
| 2 | 2,039 | debug and release |
| 3 | 97,862 | debug and release |
| 4 | 4,085,603 | release only |

The deep cases are gated with `#[cfg(not(debug_assertions))]` rather than
returning early in debug, so a debug run cannot be mistaken for coverage. A full
release run takes roughly 17 seconds on the development machine.

`tests/special_moves.rs` isolates what perft can only count: both castles in
Kiwipete, occupied paths (including b1 for queenside), castling out of, through,
and into check, rook relocation and round-tripping, all three ways a right is
revoked, en passant target lifetime, capture removal and round-tripping, missing
targets and pawns, and the discovered-check capture.

`tests/agreement.rs` separately guards the seam between the generator and the
validator. They are two independent implementations of the same rules, so they
can drift; the suite asserts that every generated move is accepted, that accepted
moves round-trip, and that rejected moves mutate nothing.

`tests/draws.rs` pins the draw layer at both levels. On `Board`, it checks that
the halfmove clock resets only on pawn moves and captures, advances otherwise, and
is restored exactly by unmake; it also checks every insufficient-material
boundary. On `Game`, it checks the fifty-move threshold at 99 versus 100, that
checkmate and stalemate outrank a draw claim, and that threefold repetition needs
a third occurrence and disappears again on unplay. The repetition-key tests
include the FIDE detail that an en passant target distinguishes positions only
when the capture is actually legal, including the pinned-pawn case.

## Roadmap

Ordered by dependency rather than by visibility — each item is what the next one
needs.

1. Switch the board to a bitboard representation
2. Add a UCI or terminal input layer
3. Build the MCTS search

The remaining draw-rule refinements — FIDE claim semantics, automatic fivefold
and seventy-five-move draws, and full dead-position detection — are deliberately
deferred. They change how a result is reported to a human, not how search should
treat a position it already knows is drawn.

