// eval.rs — from eval.hpp + eval.cpp

use std::sync::atomic::{AtomicU64, Ordering};
use crate::board::Board;
use crate::types::*;

pub static EVAL_CALLS: AtomicU64 = AtomicU64::new(0);

// eval weights — loaded once from EVAL_WEIGHTS_FILE or defaults
static WEIGHTS: std::sync::OnceLock<EvalWeightSet> = std::sync::OnceLock::new();

#[derive(Clone, Copy)]
struct EvalWeights {
    territory: i32,
    mobility: i32,
    connectivity: i32,
    corners: i32,
    edges: i32,
    recapture: i32,
    vulnerability: i32,
}

struct EvalWeightSet {
    first: EvalWeights,
    second: EvalWeights,
}

fn get_weights() -> &'static EvalWeightSet {
    WEIGHTS.get_or_init(|| {
        // Try to load from EVAL_WEIGHTS_FILE env
        if let Ok(fname) = std::env::var("EVAL_WEIGHTS_FILE") {
            if let Ok(content) = std::fs::read_to_string(&fname) {
                let nums: Vec<i32> = content
                    .split_whitespace()
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if nums.len() >= 14 {
                    return EvalWeightSet {
                        first: EvalWeights {
                            territory: nums[0],
                            mobility: nums[1],
                            connectivity: nums[2],
                            corners: nums[3],
                            edges: nums[4],
                            recapture: nums[5],
                            vulnerability: nums[6],
                        },
                        second: EvalWeights {
                            territory: nums[7],
                            mobility: nums[8],
                            connectivity: nums[9],
                            corners: nums[10],
                            edges: nums[11],
                            recapture: nums[12],
                            vulnerability: nums[13],
                        },
                    };
                }
                if nums.len() >= 7 {
                    let shared = EvalWeights {
                        territory: nums[0],
                        mobility: nums[1],
                        connectivity: nums[2],
                        corners: nums[3],
                        edges: nums[4],
                        recapture: nums[5],
                        vulnerability: nums[6],
                    };
                    return EvalWeightSet {
                        first: shared,
                        second: shared,
                    };
                }
            }
        }
        EvalWeightSet {
            first: EvalWeights {
                territory: 148,
                mobility: 20,
                connectivity: 19,
                corners: 18,
                edges: 3,
                recapture: 39,
                vulnerability: 9,
            },
            second: EvalWeights {
                territory: 140,
                mobility: 16,
                connectivity: 28,
                corners: 20,
                edges: 6,
                recapture: 28,
                vulnerability: 18,
            },
        }
    })
}

pub fn evaluate(board: &Board, is_first: bool) -> i32 {
    EVAL_CALLS.fetch_add(1, Ordering::Relaxed);

    let player = board.player;
    let opp = opponent(player);
    let cache = &board.eval_cache;

    let territory = if player == FIRST_PLAYER {
        cache.owned_first - cache.owned_second
    } else {
        cache.owned_second - cache.owned_first
    };

    let mobility_score = 0i32;
    let connectivity = if player == FIRST_PLAYER {
        cache.connectivity_first - cache.connectivity_second
    } else {
        cache.connectivity_second - cache.connectivity_first
    };
    let corners = if player == FIRST_PLAYER {
        cache.corners_first - cache.corners_second
    } else {
        cache.corners_second - cache.corners_first
    };
    let edges = if player == FIRST_PLAYER {
        cache.edges_first - cache.edges_second
    } else {
        cache.edges_second - cache.edges_first
    };
    let recapture_swing = if opp == FIRST_PLAYER {
        cache.live_adjacent_first
    } else {
        cache.live_adjacent_second
    };
    let vulnerability = if player == FIRST_PLAYER {
        cache.live_adjacent_first
    } else {
        cache.live_adjacent_second
    };

    let weights = get_weights();
    let w = if is_first {
        weights.first
    } else {
        weights.second
    };
    territory * w.territory
        + mobility_score * w.mobility
        + connectivity * w.connectivity
        + corners * w.corners
        + edges * w.edges
        + recapture_swing * w.recapture
        - vulnerability * w.vulnerability
}

pub fn score_move(board: &Board, mv: &Move, is_first: bool) -> i32 {
    if mv.is_pass() {
        let mut copy = board.clone();
        copy.apply_pass();
        return evaluate(&copy, is_first);
    }
    let mut copy = board.clone();
    copy.apply_move(mv);
    evaluate(&copy, is_first)
}
