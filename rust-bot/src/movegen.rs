// movegen.rs — from movegen.hpp + movegen.cpp

use crate::types::*;

type PrefixSum = [[i32; COLS + 1]; ROWS + 1];

fn build_prefix_sum(values: &ValueGrid) -> PrefixSum {
    let mut pref = [[0i32; COLS + 1]; ROWS + 1];
    for r in 0..ROWS {
        for c in 0..COLS {
            let val = if values[r][c] > 0 { values[r][c] as i32 } else { 0 };
            pref[r + 1][c + 1] = val + pref[r][c + 1] + pref[r + 1][c] - pref[r][c];
        }
    }
    pref
}

#[inline]
fn rect_sum(pref: &PrefixSum, r1: usize, c1: usize, r2: usize, c2: usize) -> i32 {
    pref[r2 + 1][c2 + 1] - pref[r1][c2 + 1] - pref[r2 + 1][c1] + pref[r1][c1]
}

fn check_inscribed(values: &ValueGrid, r1: usize, c1: usize, r2: usize, c2: usize) -> bool {
    let mut top = false;
    let mut bottom = false;
    let mut left = false;
    let mut right = false;

    for c in c1..=c2 {
        if values[r1][c] > 0 {
            top = true;
        }
        if values[r2][c] > 0 {
            bottom = true;
        }
    }
    for r in r1..=r2 {
        if values[r][c1] > 0 {
            left = true;
        }
        if values[r][c2] > 0 {
            right = true;
        }
    }
    top && bottom && left && right
}

pub fn generate_legal_moves(values: &ValueGrid) -> Vec<Move> {
    let mut moves = Vec::new();
    let pref = build_prefix_sum(values);

    for r1 in 0..ROWS {
        for r2 in r1..ROWS {
            for c1 in 0..COLS {
                for c2 in c1..COLS {
                    let sum = rect_sum(&pref, r1, c1, r2, c2);
                    if sum > 10 {
                        // Row-band early break: sum only increases as c2 expands
                        break;
                    }
                    if sum == 10 && check_inscribed(values, r1, c1, r2, c2) {
                        moves.push(Move {
                            r1: r1 as i32,
                            c1: c1 as i32,
                            r2: r2 as i32,
                            c2: c2 as i32,
                            priority: 0,
                        });
                    }
                }
            }
        }
    }
    moves
}
