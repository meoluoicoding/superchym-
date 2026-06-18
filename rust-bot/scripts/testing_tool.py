#!/usr/bin/env python3
"""Testing tool for Mushroom Game - matches two bots via I/O protocol."""

from __future__ import annotations

import argparse
import csv
import configparser
import random
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from shlex import split as shell_split

ROWS = 10
COLS = 17
FIRST = 1
SECOND = -1
PASS = (-1, -1, -1, -1)
RECT_COUNT = ROWS * (ROWS + 1) // 2 * COLS * (COLS + 1) // 2
SCRIPT_DIR = Path(__file__).resolve().parent


config = configparser.ConfigParser()
config.read(SCRIPT_DIR / "setting.ini")

INPUT = config.get("DEFAULT", "INPUT", fallback="input.txt")
LOG = config.get("DEFAULT", "LOG", fallback="log.txt")
EXEC1 = config.get("DEFAULT", "EXEC1", fallback="")
EXEC2 = config.get("DEFAULT", "EXEC2", fallback="python3 -u opponent.py")
GAMES = config.getint("DEFAULT", "GAMES", fallback=1)
SEED = config.getint("DEFAULT", "SEED", fallback=42)
TIME_BUDGET_MS = config.getfloat("DEFAULT", "TIME_BUDGET_MS", fallback=10000.0)


def resolve_path(raw: str) -> Path:
    path = Path(raw)
    if path.is_absolute():
        return path
    return (SCRIPT_DIR / path).resolve()


def resolve_command(command: str) -> list[str]:
    tokens = shell_split(command, posix=False)
    resolved: list[str] = []
    for token in tokens:
        token = token.strip('"')
        path = Path(token)
        if path.is_absolute():
            resolved.append(str(path))
            continue
        if token.startswith("-"):
            resolved.append(token)
            continue
        if any(sep in token for sep in ("/", "\\")) or token.startswith("."):
            resolved.append(str((SCRIPT_DIR / path).resolve()))
            continue
        resolved.append(token)
    return resolved


@dataclass
class BoardState:
    values: list[list[int]]
    owners: list[list[int]]
    player: int = FIRST
    passes: int = 0

    @classmethod
    def from_rows(cls, rows: list[str]) -> "BoardState":
        values = [[int(ch) for ch in row.strip()] for row in rows[:ROWS]]
        owners = [[0 for _ in range(COLS)] for _ in range(ROWS)]
        return cls(values=values, owners=owners, player=FIRST, passes=0)

    def is_terminal(self) -> bool:
        return self.passes >= 2

    def score(self, player_id: int) -> int:
        return sum(1 for row in self.owners for owner in row if owner == player_id)

    def is_legal_action(self, move: tuple[int, int, int, int]) -> bool:
        if move == PASS:
            return True
        r1, c1, r2, c2 = move
        if r1 < 0 or c1 < 0 or r2 >= ROWS or c2 >= COLS or r1 > r2 or c1 > c2:
            return False

        total = 0
        top = bottom = left = right = False
        for r in range(r1, r2 + 1):
            for c in range(c1, c2 + 1):
                v = self.values[r][c]
                total += v
                if v > 0:
                    if r == r1:
                        top = True
                    if r == r2:
                        bottom = True
                    if c == c1:
                        left = True
                    if c == c2:
                        right = True
        return total == 10 and top and bottom and left and right

    def apply_action(self, move: tuple[int, int, int, int]) -> None:
        if move == PASS:
            self.player = -self.player
            self.passes += 1
            return

        r1, c1, r2, c2 = move
        for r in range(r1, r2 + 1):
            for c in range(c1, c2 + 1):
                self.values[r][c] = 0
                self.owners[r][c] = self.player
        self.player = -self.player
        self.passes = 0


def _score_bucket(score: int) -> int:
    return max(0, min(7, (score // 25) + 4))


def _corner_edge_bonus(move: tuple[int, int, int, int]) -> int:
    if move == PASS:
        return 0
    r1, c1, r2, c2 = move
    bonus = 0
    touches_corner = (
        (r1 == 0 and c1 == 0)
        or (r1 == 0 and c2 == COLS - 1)
        or (r2 == ROWS - 1 and c1 == 0)
        or (r2 == ROWS - 1 and c2 == COLS - 1)
    )
    if touches_corner:
        bonus += 15
    touches_edge = r1 == 0 or r2 == ROWS - 1 or c1 == 0 or c2 == COLS - 1
    if touches_edge:
        bonus += 5
    return bonus


def _action_score(state: BoardState, move: tuple[int, int, int, int]) -> tuple[int, int, int, int, int, int]:
    if move == PASS:
        return (-10_000, 0, 0, 0, 0, 0)
    r1, c1, r2, c2 = move
    opp = -state.player
    area = (r2 - r1 + 1) * (c2 - c1 + 1)
    fresh = recaptured = own = live = 0
    for r in range(r1, r2 + 1):
        for c in range(c1, c2 + 1):
            owner = state.owners[r][c]
            if owner == 0:
                fresh += 1
            elif owner == opp:
                recaptured += 1
            else:
                own += 1
            if state.values[r][c] > 0:
                live += 1
    return fresh + 2 * recaptured, recaptured, fresh, live, area - own, area


def _move_metadata(state: BoardState, move: tuple[int, int, int, int]) -> tuple[int, int, int, float] | None:
    if move == PASS:
        live_count = sum(1 for row in state.values for v in row if v > 0)
        phase = 2 if live_count <= 12 else 0 if live_count >= 25 else 1
        return RECT_COUNT, phase, 0, -10_000.0

    live_count = sum(1 for row in state.values for v in row if v > 0)
    phase = 2 if live_count <= 12 else 0 if live_count >= 25 else 1
    rect_id = 0

    for r1 in range(ROWS):
        for r2 in range(r1, ROWS):
            col_sums = [sum(state.values[r][c] for r in range(r1, r2 + 1)) for c in range(COLS)]
            c1 = 0
            while c1 < COLS:
                total = 0
                c2 = c1
                while c2 < COLS:
                    total += col_sums[c2]
                    if total > 10:
                        break
                    if total == 10:
                        top = any(state.values[r1][c] > 0 for c in range(c1, c2 + 1))
                        bottom = any(state.values[r2][c] > 0 for c in range(c1, c2 + 1))
                        left = any(state.values[r][c1] > 0 for r in range(r1, r2 + 1))
                        right = any(state.values[r][c2] > 0 for r in range(r1, r2 + 1))
                        if top and bottom and left and right:
                            candidate = (r1, c1, r2, c2)
                            if candidate == move:
                                sd, rec, fresh, _live, _area2, area = _action_score(state, move)
                                corner_edge = _corner_edge_bonus(move)
                                score = sd + rec * 25 + fresh * 5 + area * 3 + corner_edge
                                return rect_id, phase, _score_bucket(score), float(score)
                            rect_id += 1
                    c2 += 1
                c1 += 1

    return None


def read_boards(path: str) -> list[list[str]]:
    boards = []
    with open(path) as f:
        board = []
        for line in f:
            line = line.strip()
            if not line:
                if board:
                    boards.append(board)
                    board = []
            else:
                board.append(line)
        if board:
            boards.append(board)
    return boards


def parse_move(resp: str) -> tuple[int, int, int, int] | None:
    parts = resp.split()
    if len(parts) != 4:
        return None
    try:
        return tuple(int(p) for p in parts)  # type: ignore[return-value]
    except ValueError:
        return None


def make_random_board(rng: random.Random) -> list[str]:
    return ["".join(str(rng.randint(1, 9)) for _ in range(COLS)) for _ in range(ROWS)]


def format_progress(current: int, total: int, wins_a: int, wins_b: int, draws: int, start_time: float) -> str:
    width = 28
    ratio = 1.0 if total <= 0 else current / total
    filled = min(width, max(0, int(round(width * ratio))))
    bar = "#" * filled + "-" * (width - filled)
    elapsed = time.time() - start_time
    per_game = elapsed / current if current > 0 else 0.0
    remaining = max(total - current, 0) * per_game
    return (
        f"[{bar}] {current}/{total} "
        f"A={wins_a} B={wins_b} D={draws} "
        f"elapsed={elapsed:6.1f}s eta={remaining:6.1f}s"
    )


def print_progress(current: int, total: int, wins_a: int, wins_b: int, draws: int, start_time: float) -> None:
    line = format_progress(current, total, wins_a, wins_b, draws, start_time)
    end = "\r" if current < total else "\n"
    print(line, end=end, flush=True)


def run_match(
    board_rows: list[str],
    log_fh,
    csv_writer,
    game_id: int,
    first_exec: str,
    second_exec: str,
    time_budget_ms: float,
) -> str:
    state = BoardState.from_rows(board_rows)
    procs = []
    records: list[tuple[int, int, int, int, int, int, float]] = []
    result = "DRAW"
    try:
        if first_exec:
            procs.append(
                subprocess.Popen(
                    resolve_command(first_exec),
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    text=True,
                    bufsize=0,
                )
            )
        if second_exec:
            procs.append(
                subprocess.Popen(
                    resolve_command(second_exec) if isinstance(second_exec, str) else second_exec,
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    text=True,
                    bufsize=0,
                )
            )

        for i, p in enumerate(procs):
            p.stdin.write(f"READY {'FIRST' if i == 0 else 'SECOND'}\n")
            p.stdin.flush()
            resp = p.stdout.readline().strip()
            assert resp == "OK", f"Bot {i} READY failed: {resp}"
            init_line = f"INIT {' '.join(board_rows)}"
            p.stdin.write(init_line + "\n")
            p.stdin.flush()

        log_fh.write(f"INIT {' '.join(board_rows)}\n")

        t1 = t2 = float(time_budget_ms)
        turn = 0
        ply = 0
        while not state.is_terminal() and turn <= 200:
            idx = 0 if state.player == FIRST else 1
            if idx >= len(procs):
                log_fh.write("ABORT 0 missing_player\n")
                break

            my_time = t1 if idx == 0 else t2
            opp_time = t2 if idx == 0 else t1
            start = time.time()

            p = procs[idx]
            p.stdin.write(f"TIME {int(my_time)} {int(opp_time)}\n")
            p.stdin.flush()
            resp = p.stdout.readline().strip()
            elapsed_ms = (time.time() - start) * 1000

            move = parse_move(resp)
            if move is None:
                log_fh.write(f"INVALID_RESPONSE: {resp}\n")
                break

            if idx == 0:
                t1 -= elapsed_ms
            else:
                t2 -= elapsed_ms

            if t1 < 0 or t2 < 0:
                log_fh.write(f"ABORT {idx} timeout\n")
                break

            if not state.is_legal_action(move):
                log_fh.write(f"ABORT {idx} illegal_move {' '.join(map(str, move))}\n")
                break

            mover = "FIRST" if idx == 0 else "SECOND"
            log_fh.write(f"{mover} {' '.join(map(str, move))} {elapsed_ms:.0f}\n")

            meta = _move_metadata(state, move)
            if meta is not None:
                rect_id, phase, bucket, move_value = meta
                records.append((game_id, ply, state.player, rect_id, phase, bucket, move_value))

            state.apply_action(move)

            opp_idx = 1 - idx
            if opp_idx < len(procs) and not state.is_terminal():
                procs[opp_idx].stdin.write(f"OPP {' '.join(map(str, move))} {elapsed_ms:.0f}\n")
                procs[opp_idx].stdin.flush()

            turn += 1
            ply += 1

        first_score = state.score(FIRST)
        second_score = state.score(SECOND)
        log_fh.write(f"SCOREFIRST {first_score}\n")
        log_fh.write(f"SCORESECOND {second_score}\n")
        if first_score > second_score:
            log_fh.write("RESULT FIRST\n")
            result = "FIRST"
        elif second_score > first_score:
            log_fh.write("RESULT SECOND\n")
            result = "SECOND"
        else:
            log_fh.write("RESULT DRAW\n")
        log_fh.write("\n")

        if csv_writer is not None:
            margin = first_score - second_score
            for game_id, ply, mover, rect_id, phase, bucket, move_value in records:
                if margin == 0:
                    outcome = 0.5
                else:
                    mover_wins = (mover == FIRST and margin > 0) or (mover == SECOND and margin < 0)
                    outcome = 1.0 if mover_wins else 0.0
                csv_writer.writerow([game_id, ply, mover, rect_id, phase, bucket, f"{move_value:.4f}", f"{outcome:.2f}", margin])
        return result

    finally:
        for p in procs:
            try:
                p.stdin.write("FINISH\n")
                p.stdin.flush()
            except Exception:
                pass
            try:
                p.kill()
            except Exception:
                pass


def main() -> None:
    parser = argparse.ArgumentParser(description="Run Mushroom protocol matches between two bots.")
    parser.add_argument("--input", default=INPUT, help="Path to board input file. Defaults to setting.ini INPUT.")
    parser.add_argument("--log", default=LOG, help="Path to the output log file. Defaults to setting.ini LOG.")
    parser.add_argument("--csv-log", default="", help="Optional CSV log compatible with update_weights/gen_mquality.")
    parser.add_argument("--exec1", default=EXEC1, help="Command for bot 1. Defaults to setting.ini EXEC1.")
    parser.add_argument("--exec2", default=EXEC2, help="Command for bot 2. Defaults to setting.ini EXEC2.")
    parser.add_argument("--games", type=int, default=GAMES, help="Number of games to play.")
    parser.add_argument("--seed", type=int, default=SEED, help="Random seed for board and side shuffling.")
    parser.add_argument(
        "--time-budget",
        type=float,
        default=TIME_BUDGET_MS,
        help="Initial time budget in milliseconds passed to each bot.",
    )
    parser.add_argument(
        "--shuffle-sides",
        action="store_true",
        help="Randomly swap which bot goes first in each game.",
    )
    parser.add_argument(
        "--progress",
        action="store_true",
        help="Show a progress bar on the terminal.",
    )
    parser.add_argument(
        "--no-progress",
        action="store_false",
        dest="progress",
        help="Disable the terminal progress bar.",
    )
    parser.set_defaults(progress=sys.stdout.isatty())
    args = parser.parse_args()

    rng = random.Random(args.seed)
    input_path = resolve_path(args.input)
    log_path = resolve_path(args.log)
    boards = read_boards(str(input_path)) if input_path.exists() else []

    bot1_wins = 0
    bot2_wins = 0
    total_draw = 0
    bot1_first = 0
    bot1_second = 0

    start_time = time.time()
    with open(log_path, "w") as log_fh:
        csv_fh = None
        csv_writer = None
        if args.csv_log:
            csv_path = resolve_path(args.csv_log)
            csv_path.parent.mkdir(parents=True, exist_ok=True)
            csv_fh = open(csv_path, "w", newline="", encoding="utf-8")
            csv_writer = csv.writer(csv_fh)
            csv_writer.writerow(["game_id", "ply", "mover", "rect_id", "phase", "bucket", "move_value", "outcome", "margin"])

        try:
            for game_idx in range(args.games):
                board = boards[game_idx % len(boards)] if boards else make_random_board(rng)
                bot1_goes_first = not args.shuffle_sides or bool(rng.getrandbits(1))
                first_exec = args.exec1 if bot1_goes_first else args.exec2
                second_exec = args.exec2 if bot1_goes_first else args.exec1
                if bot1_goes_first:
                    bot1_first += 1
                else:
                    bot1_second += 1

                log_fh.write(f"GAME {game_idx + 1}/{args.games}\n")
                log_fh.write(f"SIDE BOT1={'FIRST' if bot1_goes_first else 'SECOND'}\n")
                summary = run_match(
                    board,
                    log_fh,
                    csv_writer,
                    game_idx,
                    first_exec,
                    second_exec,
                    args.time_budget,
                )
                if summary == "DRAW":
                    total_draw += 1
                elif summary == "FIRST":
                    if bot1_goes_first:
                        bot1_wins += 1
                    else:
                        bot2_wins += 1
                elif summary == "SECOND":
                    if bot1_goes_first:
                        bot2_wins += 1
                    else:
                        bot1_wins += 1

                if args.progress:
                    print_progress(game_idx + 1, args.games, bot1_wins, bot2_wins, total_draw, start_time)

            log_fh.write(f"SUMMARY BOT1={bot1_wins} BOT2={bot2_wins} DRAW={total_draw}\n")
            log_fh.write(f"SIDECOUNT BOT1_FIRST={bot1_first} BOT1_SECOND={bot1_second}\n")
        finally:
            if csv_fh is not None:
                csv_fh.close()

    if args.progress and args.games == 0:
        print_progress(0, 0, bot1_wins, bot2_wins, total_draw, start_time)
    print(
        f"Games: {args.games} | BOT1={bot1_wins} BOT2={bot2_wins} DRAW={total_draw} | "
        f"BOT1_FIRST={bot1_first} BOT1_SECOND={bot1_second}"
    )


if __name__ == "__main__":
    main()
