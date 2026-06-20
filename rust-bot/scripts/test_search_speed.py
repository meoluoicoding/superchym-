#!/usr/bin/env python3
"""Test search speed and depth."""

import subprocess
import time
import sys

BOT_EXE = "../target/release/mushroom_bot.exe"
INPUT_FILE = "../input.txt"

def read_boards(path):
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

def run_bot_search(board_rows, time_ms=1000):
    """Run bot and capture timing info."""
    proc = subprocess.Popen(
        BOT_EXE,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=0,
    )
    
    try:
        # READY FIRST
        proc.stdin.write("READY FIRST\n")
        proc.stdin.flush()
        resp = proc.stdout.readline().strip()
        if resp != "OK":
            return None
        
        # INIT
        init_line = "INIT " + " ".join(board_rows)
        proc.stdin.write(init_line + "\n")
        proc.stdin.flush()
        
        # TIME - 1 second budget
        start = time.time()
        proc.stdin.write(f"TIME {time_ms} {time_ms}\n")
        proc.stdin.flush()
        resp = proc.stdout.readline().strip()
        elapsed = (time.time() - start) * 1000
        
        # Parse move
        parts = resp.split()
        if len(parts) == 4:
            move = tuple(int(p) for p in parts)
            return move, elapsed
        return None, elapsed
    finally:
        try:
            proc.stdin.write("FINISH\n")
            proc.stdin.flush()
        except:
            pass
        proc.kill()

def measure_search_speed(board_rows, time_ms=1000, games=5):
    """Measure average search speed."""
    total_time = 0
    total_moves = 0
    
    for _ in range(games):
        result = run_bot_search(board_rows, time_ms)
        if result:
            move, elapsed = result
            total_time += elapsed
            total_moves += 1
    
    if total_moves == 0:
        return 0, 0
    
    avg_time = total_time / total_moves
    return avg_time, total_moves

def main():
    boards = read_boards(INPUT_FILE)
    if not boards:
        print("No boards found")
        return
    
    print("=" * 60)
    print("SEARCH SPEED TEST")
    print("=" * 60)
    
    # Test each board
    for i, board in enumerate(boards):
        print(f"\nBoard {i+1}:")
        
        # Test with 1s budget
        avg_time, moves = measure_search_speed(board, time_ms=1000, games=3)
        print(f"  1s budget: avg={avg_time:.0f}ms per move ({moves} moves)")
        
        # Test with 2s budget
        avg_time, moves = measure_search_speed(board, time_ms=2000, games=3)
        print(f"  2s budget: avg={avg_time:.0f}ms per move ({moves} moves)")
    
    print("\n" + "=" * 60)

if __name__ == "__main__":
    main()
