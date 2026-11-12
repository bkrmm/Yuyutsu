# Yuyutsu

A chess engine written in Rust, working toward Monte Carlo Tree Search.

Yuyutsu was the one Kaurava who chose to fight for the side he judged right.

## Status

The rules layer is solid and verified. There is no search yet, so the engine does
not play chess — it can only enumerate and apply legal moves.

**Working**

- 8x8 board with piece placement and `side_to_move`
- Pseudo-legal move generation for all six piece types
- Full geometric validation on every move, returning a typed `InvalidMove` reason
- `make_move` / `unmake_move` as a matched pair driven by an `Undo` record
- Promotion to queen, rook, bishop, or knight, on pushes *and* captures
- Legality filtering — moves leaving the mover's own king attacked are removed
- Check detection
- Checkmate and stalemate are distinguishable (empty legal list, plus check)

**Not implemented**

- Castling
- En passant
- Draw rules: fifty-move, threefold repetition, insufficient material
- Search, evaluation, MCTS
- Bitboards
- A user interface; `cargo run` is a smoke test that prints one move

## Getting started

```sh
cargo run      # print the legal move count, play one move, show the board
cargo test     # 14 tests
```

No dependencies. Rust 2024 edition.

## Layout

| Path | Contents |
| --- | --- |
| `src/board.rs` | Board, pieces, `Undo`, `make_move`/`unmake_move`, validation, legality filter |
| `src/moves.rs` | `Move` and the per-piece pseudo-legal generators |
| `src/lib.rs` | Library root, so `tests/` can link the engine |
| `src/main.rs` | Smoke-test binary |
| `tests/agreement.rs` | Generator/validator agreement and make/unmake round trips |
| `tests/perft.rs` | Perft oracle and targeted position tests |

### Coordinates

Squares are `(row, column)`, both `0..8`, with **row 0 as rank 8** and **column 0
as the a-file**. So White's king starts at `(7, 4)` (e1) and Black's at `(0, 4)`
(e8). White advances by *decreasing* the row. This is the one convention worth
internalising before reading the generators.

## Design

Three decisions shape the codebase:

**Move generation is pure.** Generators borrow the board and return moves; they
never mutate. `make_move` is the only mutator, and it validates fully *before*
writing anything, so a rejected move provably leaves the board untouched.

**`Undo` carries what happened, not what was asked.** `Move` says "pawn to e8,
promote to queen"; `Undo` records the piece that actually moved, what was
captured, and the previous side to move. That makes `unmake_move` a mechanical
inverse containing no chess knowledge. Any new move type — castling, en passant —
needs a new `Undo` field in the same change, or it cannot be reversed.

**Legality is a separate pass.** `generate_all_moves` is pseudo-legal: it ignores
whether a move exposes the king. `generate_legal_moves` filters it by playing each
candidate, asking `in_check`, and reverting. This is why make/unmake had to exist
before the filter could be written.

### Known imprecision

`in_check` asks whether any enemy *pseudo-legal* move lands on the king's square,
so a pinned enemy piece still counts as giving check. The error is fail-closed: it
can reject a legal move but cannot permit an illegal one. Replacing it with a
square-attack scan is the next planned change.

## Verification

`tests/perft.rs` checks the published perft counts for the starting position:

| depth | nodes |
| --- | --- |
| 1 | 20 |
| 2 | 400 |
| 3 | 8,902 |
| 4 | 197,281 |

Depth 4 is the current ceiling. En passant needs at least five plies to become
possible and castling roughly eight, so neither can appear in the counted tree —
which is what lets these numbers act as a clean oracle before those rules exist.
Perft is much faster in release: `cargo test --release`.

`tests/agreement.rs` separately guards the seam between the generator and the
validator. They are two independent implementations of the same rules, so they can
drift; the suite asserts that every generated move is accepted, that accepted
moves round-trip, and that rejected moves mutate nothing.

## Roadmap

Ordered by dependency rather than by visibility — each item is what the next one
needs.

1. Replace `in_check` with an `is_attacked` square scan (faster, and drops the pin
   imprecision)
2. Add a `GameState` enum: ongoing, checkmate, stalemate, draw
3. Implement castling and en passant, with their `Undo` fields
4. Extend perft to depths 5–6 against published counts
5. Switch the board to a bitboard representation
6. Add a UCI or terminal input layer
7. Build the MCTS search

