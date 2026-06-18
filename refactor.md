# Mushroom Bot Architecture Refactor

## Goal

Reduce structural complexity in `mushroom-bot` without breaking search strength.

The current repo works, but the boundaries between engine, strategy, protocol,
and tooling are blurry. The result is that:

- `main.rs` is too large and orchestration-heavy
- `search/` contains both core search logic and strategy heuristics
- `opening/`, `side/`, `policy/`, and `timeman/` overlap in responsibility
- `mid_game/` exists but is effectively dead
- `end_game/` is really a search mode, not a separate subsystem
- `src/bin/*` mixes essential tools and experimental workflows in one flat area

This document defines a target architecture and a safe refactor order.

## Current Map

### Core board engine

- `src/types.rs`
- `src/bitboard.rs`
- `src/board.rs`
- `src/movegen.rs`
- `src/tt.rs`

### Evaluation and learned data

- `src/eval.rs`
- `src/dataloader.rs`
- `src/mquality.rs`

### Search stack

- `src/search/mod.rs`
- `src/search/root.rs`
- `src/search/negamax.rs`
- `src/search/ordering.rs`
- `src/search/pruning.rs`
- `src/search/tactics.rs`
- `src/search/phase_bonus.rs`
- `src/search/movelist.rs`
- `src/search/result.rs`

### Strategy and runtime policy

- `src/opening/first.rs`
- `src/opening/second.rs`
- `src/opening/side.rs`
- `src/opening/stealth.rs`
- `src/opening/traps.rs`
- `src/side/mod.rs`
- `src/policy.rs`
- `src/timeman.rs`

### Phase-specific leftovers

- `src/end_game/search.rs`
- `src/end_game/mod.rs`
- `src/mid_game/mod.rs`

### Runtime entrypoint

- `src/main.rs`

### Tooling

- `src/bin/build_data.rs`
- `src/bin/gen_geometry.rs`
- `src/bin/gen_mquality.rs`
- `src/bin/opening_sampler.rs`
- `src/bin/tournament.rs`
- `src/bin/tuner.rs`
- `src/bin/update_weights.rs`

## Main Problems

### 1. `main.rs` is a god file

`main.rs` currently owns:

- env-based config loading
- weight/data loading
- policy config loading
- protocol handling
- game state orchestration
- timing integration
- search invocation
- logging
- pass-artifact handling

This makes changes risky and hides the real runtime control flow.

### 2. Search and strategy are mixed

The search stack is not just alpha-beta infrastructure. It also contains:

- phase bonuses
- side-specific bonuses
- root reply heuristics
- tactical policy-like ranking

That makes it harder to reason about what is "search quality" versus what is
"game strategy".

### 3. Phase folders do not reflect real boundaries

- `mid_game/` is mostly dead
- `end_game/` is actually an exact-search mode
- `opening/` contains both opening logic and side-specific long-lived tuning

The folder names suggest a cleaner phase split than the code actually has.

### 4. Tooling is too visible at the same layer as engine code

`src/bin/*` is legitimate, but the number of tools makes the repo feel heavier
than it is. There is no clear distinction between:

- production support tools
- training tools
- analysis tools
- one-off experiments

## Target Architecture

### `src/engine/`

Keep low-level board and state machinery here.

- `engine/types.rs`
- `engine/bitboard.rs`
- `engine/board.rs`
- `engine/movegen.rs`
- `engine/tt.rs`

### `src/eval/`

Keep evaluation and data-backed scoring here.

- `eval/mod.rs`
- `eval/core.rs`
- `eval/data.rs`
- `eval/mquality.rs`

If a lighter first step is preferred, keep filenames as-is and only move
folders later.

### `src/search/`

Keep only true search infrastructure here.

- `search/mod.rs`
- `search/root.rs`
- `search/negamax.rs`
- `search/ordering.rs`
- `search/pruning.rs`
- `search/result.rs`
- `search/exact_endgame.rs`

Candidates to merge:

- merge `phase_bonus.rs` into `ordering.rs`
- move or merge small tactical helpers from `tactics.rs` into `ordering.rs` or
  `pruning.rs`

### `src/strategy/`

Everything that expresses how the bot wants to play, outside the pure search
kernel.

- `strategy/policy.rs`
- `strategy/timeman.rs`
- `strategy/first.rs`
- `strategy/second.rs`
- `strategy/patterns/stealth.rs`
- `strategy/patterns/traps.rs`

This replaces the current `opening/`, `side/`, and much of the phase-oriented
naming.

### `src/agent/`

Protocol and runtime orchestration should move here.

- `agent/mod.rs`
- `agent/config.rs`
- `agent/protocol.rs`
- `agent/logging.rs`

`src/main.rs` should become a very thin bootstrap.

### `src/bin/`

Keep it, but treat it as grouped tooling:

- data generation
- tuning
- benchmarking
- analysis

If needed later, mirror that grouping in comments or sub-readmes.

## Concrete File Moves

### First wave

- remove `src/mid_game/mod.rs`
- remove `pub mod mid_game;` from `src/lib.rs`

### Second wave

- move `src/end_game/search.rs` to `src/search/exact_endgame.rs`
- remove `src/end_game/mod.rs`
- remove `pub mod end_game;` from `src/lib.rs`

### Third wave

- move `src/opening/first.rs` to `src/strategy/first.rs`
- move `src/opening/second.rs` to `src/strategy/second.rs`
- move `src/opening/stealth.rs` to `src/strategy/patterns/stealth.rs`
- move `src/opening/traps.rs` to `src/strategy/patterns/traps.rs`
- absorb `src/opening/side.rs` into a new `strategy/mod.rs` or side-specific
  trait file
- simplify `src/side/mod.rs` so it becomes a compatibility shim or remove it

### Fourth wave

- split `src/main.rs` into:
  - `src/agent/config.rs`
  - `src/agent/protocol.rs`
  - `src/agent/logging.rs`
  - `src/main.rs` bootstrap only

### Fifth wave

- merge `src/search/phase_bonus.rs` into `src/search/ordering.rs`
- review whether `src/search/tactics.rs` should stay separate

## Recommended Refactor Order

### Stage 1: dead/simple cleanup

Safe and low-risk:

1. delete `mid_game`
2. rename or move `end_game/search.rs` into `search/`

### Stage 2: untangle runtime

Important before larger engine cleanup:

1. extract `Agent`
2. extract env/config loading
3. extract logging

### Stage 3: normalize strategy folders

1. replace `opening/` with `strategy/`
2. reduce `side/` to a thin compatibility layer or remove it

### Stage 4: shrink search surface

1. merge tiny heuristic modules
2. keep only core search concepts in `search/`

### Stage 5: tooling organization

1. group binaries conceptually
2. decide which ones are permanent and which are experimental

## What Not To Do First

- do not rename every folder at once
- do not move `search/*` and `main.rs` in the same commit
- do not mix architecture refactor with search-strength tuning in one patch
- do not remove `src/bin/*` before confirming each tool is still needed

## Suggested First Commit Plan

### Commit 1

- remove `mid_game`
- move `end_game/search.rs` into `search/exact_endgame.rs`
- update imports

### Commit 2

- extract `Agent` out of `main.rs`
- keep behavior identical

### Commit 3

- introduce `strategy/`
- move `opening/*` and `policy.rs` gradually

### Commit 4

- collapse `phase_bonus.rs`
- decide whether `tactics.rs` stays

## Success Criteria

The refactor is successful if:

- a new reader can trace move selection quickly
- `main.rs` becomes small
- `search/` reads like a search engine, not a mixed strategy folder
- `strategy/` becomes the home of policy and side-dependent playstyle
- dead folders are removed
- tools remain available without cluttering core engine understanding

## Short Version

Start with:

1. delete `mid_game`
2. move `end_game/search.rs` into `search/`
3. split `main.rs`

That gets the biggest clarity win with the lowest risk.
