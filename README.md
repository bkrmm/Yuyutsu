# Yuyutsu

A chess engine written in Rust, working toward Monte Carlo Tree Search.

Yuyutsu was the one Kaurava who chose to fight for the side he judged right.

## Status

The rules layer is solid and verified, and `cargo run` starts a terminal REPL.
There is no search yet, so the engine does not choose moves: the interface is a
hot-seat board and rule checker, with both sides entered by hand.

**Working**

- 64-bit bitboard board: twelve piece bitboards plus cached color and total
  occupancy
- Pseudo-legal move generation for all six piece types, driven by precomputed
  attack tables and ray masks
- Full geometric validation on every move, returning a typed `InvalidMove` reason
- `make_move` / `unmake_move` as a matched pair driven by an `Undo` record
- Promotion to queen, rook, bishop, or knight, on pushes *and* captures
- Castling for both sides, with rights revoked by king moves, rook moves, and
  rook captures
- En passant, including the discovered-check case where the capture is
  geometrically valid but exposes the king
- Legality filtering — moves leaving the mover's own king attacked are removed
- Attack detection by bitboard intersection (FIDE 3.1.3, so pinned pieces still
  count as attacking)
- Checkmate and stalemate are distinguishable (empty legal list, plus check)
- Halfmove clock maintained by `make_move` and restored exactly by `unmake_move`
- Fifty-move rule at 100 halfmoves, threefold repetition, and the standard
  insufficient-material set
- `Game` / `GameState` reporting ongoing, checkmate, stalemate, or a typed
  `DrawReason`, with checkmate and stalemate taking precedence
- Terminal REPL with coordinate moves, promotion suffixes, legal-move listing,
  state display, undo, and new game

**Not implemented**

- [ ] The full FIDE claim procedure: claimable draws are reported as terminal, and
  automatic fivefold repetition / seventy-five-move draws are not distinguished
- [ ] Complete dead-position analysis; blocked-pawn positions are not detected
- [ ] Search, evaluation, MCTS
- [ ] Sliding-piece attacks by magic bitboard; the current ray tables are exact
  but do more work per query
- [ ] A UCI protocol for standard chess GUIs, and a graphical interface

## Getting started

```sh
cargo run                # start the terminal REPL
cargo test               # 61 tests: rules, perft to depth 4, draws, representation, interface
cargo test --release     # 63 tests: adds start-position perft 5-6 and Kiwipete 4
```

No dependencies. Rust 2024 edition.

## Layout

| Path | Contents |
| --- | --- |
| `src/board.rs` | Bitboard board, pieces, castling rights, en passant target, halfmove clock, `Undo`, `make_move`/`unmake_move`, validation, attack tables, legality filter, repetition key, material draw check |
| `src/game.rs` | `Game`, `GameState`, `DrawReason`, repetition history and terminal-state evaluation |
| `src/moves.rs` | `Move` and the per-piece pseudo-legal generators, including castling and en passant |
| `src/ui.rs` | Command parsing, `Session` with its undo stack, status labels, and help text |
| `src/lib.rs` | Library root, so `tests/` can link the engine |
| `src/main.rs` | Terminal read-eval-print loop |
| `tests/common/mod.rs` | Shared FEN parser, position builder, and perft |
| `tests/agreement.rs` | Generator/validator agreement and make/unmake round trips |
| `tests/perft.rs` | Perft oracle for the start position and Kiwipete, plus targeted position tests |
| `tests/special_moves.rs` | Castling and en passant edge cases in isolation |
| `tests/bitboard.rs` | Square mapping, occupancy caching, attack-table geometry, ray blocking, and repetition-key structure |
| `tests/draws.rs` | Halfmove clock, fifty-move, repetition identity, and insufficient material |
| `tests/ui.rs` | Command parsing, session behavior, and terminal-state gating |

### Coordinates

Squares are `(row, column)`, both `0..8`, with **row 0 as rank 8** and **column 0
as the a-file**. So White's king starts at `(7, 4)` (e1) and Black's at `(0, 4)`
(e8). White advances by *decreasing* the row. This is the one convention worth
internalising before reading the generators.

## Design

Seven decisions shape the codebase:

**The board is a set of bitboards.** One `u64` per color and piece kind holds
every square that piece stands on, with bit `row * 8 + column` (a8 is bit 0, h1
is bit 63). Attacks are then set operations: a knight's targets are one table
lookup, a rook's are four precomputed rays truncated at the nearest blocker.
`occupied` and `occupied_by(color)` are cached words maintained by the same
`put_piece`/`take_piece` helpers that move making uses, so no query rebuilds them
from the twelve piece boards. The board is the only source of truth -- there is no
parallel mailbox to drift out of sync. The measured gain is about 2× on perft,
not an order of magnitude, because the legality filter still plays and reverts
every candidate; attack detection is no longer the bottleneck.

**Move generation is pure.** Generators borrow the board and return moves; they
never mutate. `make_move` is the only mutator, and it validates fully *before*
writing anything, so a rejected move provably leaves the board untouched. The
legality filter calls an unchecked make internally, because the generator has
already guaranteed every candidate's geometry; validation stays on the public
entry point where external callers need it.

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
board. The key is exact, not a Zobrist hash: twelve bitboards, side to move,
castling rights, and the en passant target *only when a legal capture exists*.
Hashing would be smaller, but a collision here would silently turn a win into a
draw. `GameState` treats claimable draws as terminal and reports the reason; full
FIDE claim handling belongs to a UI, not to search, and the current terminal
simply locks the position once a claim becomes available.

**The interface is a separate layer.** `main.rs` owns stdin, stdout, and the
prompt. `ui::Session` owns the `Game` plus the undo receipts, parses one line, and
returns an `Action` describing what happened; it prints nothing. That split makes
the REPL rules integration-testable without capturing terminal output.

Moves are entered in coordinate notation (`e2e4`, `e7e8q`, `e1g1`), which maps
directly onto `Move` and needs no ambiguous SAN disambiguation. The session
checks the generated legal list before playing, because `make_move` validates
geometry but not the generic "does this expose my own king?" rule. After a
terminal state the interface refuses further moves, while `Game` remains
permissive for search.

### Attack detection

`is_attacked` answers "does this color attack this square?" without generating
moves. It intersects the square's precomputed knight, king, and pawn attack sets
with that color's piece bitboards, then asks whether the square's rook rays hit a
rook or queen and whether its bishop rays hit a bishop or queen. A pawn's
reverse lookup uses the *opposite* color's table, because the table maps
"attacker square to attacked squares" and the query runs backwards.

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
release run of the perft suite takes roughly 8 seconds on the development
machine, down from about 17 before the bitboard rewrite. A/B runs of the same
individual case show 12.25 s → 5.75 s for start-position depths 5–6 and
0.95 s → 0.47 s for Kiwipete depth 4.

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

`tests/ui.rs` tests the interface layer without touching stdin or stdout. It
covers command aliases, coordinate parsing and formatting, promotion suffixes,
unknown commands and malformed moves, rejection of a pinned move that geometry
alone would accept, undo as a true stack, `new` clearing the old undo stack,
checkmate and fifty-move lockout, and the human-readable status labels.

`tests/bitboard.rs` covers the representation rather than the rules. Perft would
notice a wrong attack table eventually, but it would report a node count, not the
broken primitive. These tests pin the square-to-bit mapping as a bijection,
occupancy-cache maintenance through add/replace/clear, open-board attack counts,
ray truncation at a friendly blocker, the repetition key's exact fields, and
make/unmake restoring both occupancy words.

## Roadmap

Ordered by dependency rather than by visibility — each item is what the next one
needs.

1. Build the MCTS search
2. Add a UCI protocol for standard chess GUIs
3. Replace the ray tables with magic bitboards if profiling shows slider attacks
   dominating

The remaining draw-rule refinements — FIDE claim semantics, automatic fivefold
and seventy-five-move draws, and full dead-position detection — are deliberately
deferred. They change how a result is reported to a human, not how search should
treat a position it already knows is drawn.
