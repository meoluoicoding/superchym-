#!/usr/bin/env python3
"""Parse game log and compute fingerprint for an opponent."""

import sys
import math

ROWS = 10
COLS = 17

def classify_shape(r1, c1, r2, c2):
    h = r2 - r1 + 1
    w = c2 - c1 + 1
    if (h, w) == (1, 2): return 0  # Rect1x2
    if (h, w) == (2, 1): return 1  # Rect2x1
    if (h, w) == (1, 3): return 2  # Rect1x3
    if (h, w) == (3, 1): return 3  # Rect3x1
    if (h, w) == (2, 2): return 4  # Rect2x2
    if (h, w) == (1, 4): return 5  # Rect1x4
    if (h, w) == (4, 1): return 6  # Rect4x1
    return 7  # RectOther

def classify_orientation(r1, c1, r2, c2):
    h = r2 - r1 + 1
    w = c2 - c1 + 1
    if h == w: return 0  # Square
    if h > w: return 1   # Portrait
    return 2              # Landscape

def barrier_potential(r1, c1, r2, c2):
    h = r2 - r1 + 1
    w = c2 - c1 + 1
    if h >= 4 and w <= 2:
        cc = (c1 + c2) // 2
        return 2 if 4 <= cc <= 12 else 1
    if w >= 6 and h <= 2:
        cr = (r1 + r2) // 2
        return 2 if 3 <= cr <= 6 else 1
    return 0

def parse_game_log(path, target_side):
    """Parse game log and extract moves for target_side (FIRST or SECOND)."""
    moves = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line.startswith(target_side + " "):
                parts = line.split()
                if len(parts) >= 5:
                    r1, c1, r2, c2 = int(parts[1]), int(parts[2]), int(parts[3]), int(parts[4])
                    if r1 == -1:  # pass
                        moves.append(None)
                    else:
                        moves.append((r1, c1, r2, c2))
    return moves

def compute_fingerprint(moves):
    """Compute fingerprint feature vector from a list of moves."""
    n = len(moves)
    if n == 0:
        return None
    
    total_area = 0
    medium_count = 0
    large_count = 0
    tall_count = 0
    wide_count = 0
    steal_count = 0  # can't compute without board state
    pass_count = 0
    barrier_count = 0
    shape_counts = [0] * 8
    
    for mv in moves:
        if mv is None:
            pass_count += 1
            continue
        
        r1, c1, r2, c2 = mv
        area = (r2 - r1 + 1) * (c2 - c1 + 1)
        total_area += area
        
        if 5 <= area <= 10:
            medium_count += 1
        if area >= 11:
            large_count += 1
        
        sc = classify_shape(r1, c1, r2, c2)
        shape_counts[sc] += 1
        
        orient = classify_orientation(r1, c1, r2, c2)
        if orient == 1:  # Portrait
            tall_count += 1
        elif orient == 2:  # Landscape
            wide_count += 1
        
        if barrier_potential(r1, c1, r2, c2) > 0:
            barrier_count += 1
    
    non_pass = n - pass_count
    if non_pass == 0:
        return None
    
    # Feature vector (Q8.7 fixed point, scaled by 128)
    fv = [
        (total_area * 128) // non_pass,      # avg_area
        (medium_count * 128) // non_pass,     # medium_ratio
        (large_count * 128) // non_pass,      # large_ratio
        (tall_count * 128) // non_pass,       # portrait_ratio
        (wide_count * 128) // non_pass,       # landscape_ratio
        (steal_count * 128) // non_pass,      # steal_ratio (unknown without board)
        (pass_count * 128) // n,              # pass_ratio
        (barrier_count * 128) // non_pass,    # barrier_ratio
    ]
    
    return fv, n, non_pass

def main():
    if len(sys.argv) < 3:
        print("Usage: parse_fingerprint.py <game_log> <target_side>")
        print("  target_side: FIRST or SECOND")
        sys.exit(1)
    
    path = sys.argv[1]
    target_side = sys.argv[2]
    
    moves = parse_game_log(path, target_side)
    print(f"Found {len(moves)} moves for {target_side}")
    
    result = compute_fingerprint(moves)
    if result is None:
        print("No moves found")
        sys.exit(1)
    
    fv, n, non_pass = result
    print(f"\nFingerprint (n={n}, non_pass={non_pass}):")
    print(f"  avg_area={fv[0]}, medium_ratio={fv[1]}, large_ratio={fv[2]}")
    print(f"  portrait_ratio={fv[3]}, landscape_ratio={fv[4]}")
    print(f"  steal_ratio={fv[5]}, pass_ratio={fv[6]}, barrier_ratio={fv[7]}")
    print(f"\nFor data.bin (mean vector):")
    print(f"  mean = [{fv[0]}, {fv[1]}, {fv[2]}, {fv[3]}, {fv[4]}, {fv[5]}, {fv[6]}, {fv[7]}]")

if __name__ == "__main__":
    main()
