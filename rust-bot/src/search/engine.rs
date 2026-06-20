use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::board::{Board, MoveRecord};
use crate::eval::evaluate;
use crate::types::*;

use super::ordering::order_moves;
use super::tt::{advance_tt_age, ensure_tt_ready, pack_move, tt_flag, tt_probe, tt_store, NODES_SEARCHED};

// ====== Search Context ======

struct SearchCtx {
    start: Instant,
    budget_ms: i64,
    is_first: bool,
    killer_history_on: bool,
    nullmove_on: bool,
    pass_mode: i32,
    pass_fix: i32,
    killers: [[u32; 2]; 64],
    history: [[i32; COLS]; ROWS],
    nullmove_ok: bool,
}

impl SearchCtx {
    fn timed_out(&self) -> bool {
        self.start.elapsed().as_millis() as i64 >= self.budget_ms
    }

    fn elapsed_ms(&self) -> i64 {
        self.start.elapsed().as_millis() as i64
    }
}

fn should_try_pass(board: &Board, ctx: &SearchCtx) -> bool {
    if ctx.pass_mode == 1 {
        return true;
    }
    if ctx.pass_mode == 2 {
        return false;
    }
    if ctx.pass_fix != 0 {
        let p = board.player;
        let opp = opponent(p);
        return board.owned_cells(p) - board.owned_cells(opp) > 0;
    }
    evaluate(board, ctx.is_first) > 0
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SearchReport {
    pub best_move: Move,
    pub completed_depth: i32,
    pub nodes: u64,
    pub elapsed_ms: i64,
}

fn apply_second_root_biases(board: &mut Board, moves: &mut [Move], is_first: bool, second_steal: i32) {
    apply_second_defensive_biases(board, moves, is_first, second_steal, true);
}

fn immediate_recapture_pressure(board: &Board) -> i32 {
    let opp_moves = board.legal_moves();
    let mut best_steal = 0i32;
    for mv in &opp_moves {
        if mv.is_pass() {
            continue;
        }
        let mut steal = 0i32;
        for r in mv.r1..=mv.r2 {
            for c in mv.c1..=mv.c2 {
                if board.owners[r as usize][c as usize] == opponent(board.player) {
                    steal += 1;
                }
            }
        }
        if steal > best_steal {
            best_steal = steal;
        }
    }
    best_steal
}

fn apply_second_defensive_biases(
    board: &mut Board,
    moves: &mut [Move],
    is_first: bool,
    second_steal: i32,
    include_static_eval: bool,
) {
    if is_first {
        return;
    }

    let opp_p = opponent(board.player);
    let claimed_cells = board.owned_cells(FIRST_PLAYER) + board.owned_cells(SECOND_PLAYER);
    let opening_phase = claimed_cells < 24;
    let midgame_phase = claimed_cells < 70;
    for m in moves.iter_mut() {
        if m.is_pass() {
            m.priority -= 2000;
            continue;
        }

        let mut steal = 0i32;
        let area = (m.r2 - m.r1 + 1) * (m.c2 - m.c1 + 1);
        for r in m.r1..=m.r2 {
            for c in m.c1..=m.c2 {
                let ru = r as usize;
                let cu = c as usize;
                if board.owners[ru][cu] != opp_p {
                    continue;
                }
                let mut adjacent_live = false;
                if r > 0 && board.values[ru - 1][cu] > 0 {
                    adjacent_live = true;
                } else if r < ROWS as i32 - 1 && board.values[ru + 1][cu] > 0 {
                    adjacent_live = true;
                } else if c > 0 && board.values[ru][cu - 1] > 0 {
                    adjacent_live = true;
                } else if c < COLS as i32 - 1 && board.values[ru][cu + 1] > 0 {
                    adjacent_live = true;
                }
                if adjacent_live {
                    steal += 1;
                }
            }
        }

        let connection = board.connectivity_boost(m.r1, m.c1, m.r2, m.c2);
        let risk = board.dead_cell_risk_proxy(m.r1, m.c1, m.r2, m.c2);
        let barrier = board.barrier_potential(m.r1, m.c1, m.r2, m.c2);
        let compact_bonus = if area <= 4 {
            140
        } else if area >= 8 {
            -100
        } else {
            40
        };
        let counter_bonus = if steal > 0 {
            if opening_phase { steal * 85 } else { steal * 60 }
        } else {
            0
        };
        let connection_bonus = if opening_phase { connection * 32 } else { connection * 22 };
        let risk_penalty = if opening_phase { risk * 85 } else { risk * 52 };
        let barrier_bonus = if opening_phase { barrier * 180 } else { barrier * 110 };
        let extension_penalty = if steal == 0 && area >= 7 { 140 } else { 0 };
        let loose_shape_penalty = if connection == 0 && risk > 0 { 120 } else { 0 };
        let quiet_opening_penalty = if opening_phase && steal == 0 && barrier == 0 {
            area * 8
        } else {
            0
        };
        let midgame_guard_bonus = if midgame_phase && connection > 0 && risk == 0 { 90 } else { 0 };
        let mut recapture_tiebreak = 0;

        m.priority += counter_bonus;
        if second_steal != 0 {
            m.priority += steal * 30;
        }
        m.priority += connection_bonus;
        m.priority -= risk_penalty;
        m.priority += barrier_bonus;
        m.priority += compact_bonus;
        m.priority += midgame_guard_bonus;
        m.priority -= extension_penalty;
        m.priority -= loose_shape_penalty;
        m.priority -= quiet_opening_penalty;

        if include_static_eval {
            let mut rec = MoveRecord::new();
            board.make_move(m, &mut rec);
            if opening_phase {
                let recapture = immediate_recapture_pressure(board);
                recapture_tiebreak = recapture * 8;
            }
            let static_score = -evaluate(board, is_first);
            board.unmake_move(&rec);
            m.priority += static_score / 12;
        }
        m.priority -= recapture_tiebreak;
    }
}

// ====== Alpha-Beta ======

fn alpha_beta(
    board: &mut Board,
    depth: i32,
    mut alpha: i32,
    beta: i32,
    ctx: &mut SearchCtx,
    best_move_out: &mut Move,
) -> i32 {
    if board.consecutive_passes >= 2 {
        let p = board.player;
        let opp = opponent(p);
        let margin = board.owned_cells(p) - board.owned_cells(opp);
        return if margin > 0 { 100000 + margin }
               else if margin < 0 { -100000 + margin }
               else { 0 };
    }

    let nc = NODES_SEARCHED.load(Ordering::Relaxed);
    if (nc & 4095) == 0 && ctx.timed_out() {
        return evaluate(board, ctx.is_first);
    }
    NODES_SEARCHED.fetch_add(1, Ordering::Relaxed);

    if depth == 0 {
        return evaluate(board, ctx.is_first);
    }

    let key = board.hash;
    let mut tt_value = 0i32;
    let mut tt_move = PASS_MOVE;
    if tt_probe(key, depth, alpha, beta, &mut tt_value, &mut tt_move) {
        *best_move_out = tt_move;
        return tt_value;
    }

    if ctx.nullmove_on && depth >= 3 && !ctx.nullmove_ok {
        ctx.nullmove_ok = true;
    }
    if ctx.nullmove_on && depth >= 3 && ctx.nullmove_ok && alpha == beta - 1 {
        let static_eval = evaluate(board, ctx.is_first);
        if static_eval >= beta {
            let mut rec = MoveRecord::new();
            board.make_move(&PASS_MOVE, &mut rec);
            ctx.nullmove_ok = false;
            let mut dummy = PASS_MOVE;
            let score = -alpha_beta(board, depth - 4, -beta, -beta + 1, ctx, &mut dummy);
            ctx.nullmove_ok = true;
            board.unmake_move(&rec);
            if score >= beta {
                *best_move_out = PASS_MOVE;
                return score;
            }
        }
    }

    let mut moves = board.legal_moves();

    if moves.is_empty() {
        let mut rec = MoveRecord::new();
        board.make_move(&PASS_MOVE, &mut rec);
        let mut dummy = PASS_MOVE;
        let score = -alpha_beta(board, depth - 1, -beta, -alpha, ctx, &mut dummy);
        board.unmake_move(&rec);
        return score;
    }

    if !ctx.is_first && depth <= 3 {
        apply_second_defensive_biases(board, &mut moves, false, 0, false);
    }

    let killers = ctx.killers;
    let history = ctx.history;
    order_moves(&mut moves, &tt_move, depth, &killers, &history, ctx.killer_history_on);

    let mut best_value = -999999i32;
    let mut local_best = moves[0];
    let mut flag = tt_flag::UPPER_BOUND;

    for mv in &moves {
        let mut rec = MoveRecord::new();
        board.make_move(mv, &mut rec);
        let mut dummy = PASS_MOVE;
        let score = -alpha_beta(board, depth - 1, -beta, -alpha, ctx, &mut dummy);
        board.unmake_move(&rec);

        if score > best_value {
            best_value = score;
            local_best = *mv;
        }
        if score > alpha {
            alpha = score;
            flag = tt_flag::EXACT;
        }
        if alpha >= beta {
            flag = tt_flag::LOWER_BOUND;
            if ctx.killer_history_on && depth < 64 && !mv.is_pass() {
                let d = depth as usize;
                let pm = pack_move(mv);
                if pm != ctx.killers[d][0] {
                    ctx.killers[d][1] = ctx.killers[d][0];
                    ctx.killers[d][0] = pm;
                }
                ctx.history[mv.r2 as usize][mv.c2 as usize] += 1;
            }
            break;
        }
    }

    if ctx.pass_mode != 2 && depth >= 3 && moves.len() <= 5 {
        let try_pass = should_try_pass(board, ctx);

        if try_pass {
            let mut rec = MoveRecord::new();
            board.make_move(&PASS_MOVE, &mut rec);
            let mut dummy = PASS_MOVE;
            let pass_score = -alpha_beta(board, depth - 1, -beta, -alpha, ctx, &mut dummy);
            board.unmake_move(&rec);
            if pass_score > best_value {
                best_value = pass_score;
                local_best = PASS_MOVE;
                flag = tt_flag::EXACT;
            }
        }
    }

    *best_move_out = local_best;
    tt_store(key, depth, best_value, flag, &local_best);
    best_value
}

// ====== Iterative Deepening ======

pub fn search_best_move(board: &Board, time_budget_ms: i32, is_first: bool) -> Move {
    search_best_move_report(board, time_budget_ms, is_first).best_move
}

pub fn search_best_move_report(board: &Board, time_budget_ms: i32, is_first: bool) -> SearchReport {
    NODES_SEARCHED.store(0, Ordering::Relaxed);
    ensure_tt_ready();

    let killer_history_on = std::env::var("KILLER_HISTORY")
        .ok().and_then(|v| v.parse::<i32>().ok()).map(|v| v > 0).unwrap_or(true);
    let nullmove_on = std::env::var("NULLMOVE")
        .ok().and_then(|v| v.parse::<i32>().ok()).map(|v| v > 0).unwrap_or(false);
    let asp_window: i32 = match std::env::var("ASP_WINDOW").as_deref() {
        Ok("0") | Ok("full") => 0,
        Ok("adaptive") => -1,
        Ok(s) => s.parse().unwrap_or(50),
        Err(_) => 50,
    };
    let pass_mode: i32 = match std::env::var("PASS_MODE").as_deref() {
        Ok("no-gate") => 1,
        Ok("none") => 2,
        _ => 0,
    };
    let pass_fix: i32 = std::env::var("PASS_FIX")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let second_steal: i32 = std::env::var("SECOND_STEAL")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(0);

    advance_tt_age();

    let budget = std::cmp::max(1i64, time_budget_ms as i64 * 80 / 100);

    let mut work_board = board.clone();
    let mut moves = work_board.legal_moves();
    if moves.is_empty() {
        return SearchReport {
            best_move: PASS_MOVE,
            completed_depth: 0,
            nodes: NODES_SEARCHED.load(Ordering::Relaxed),
            elapsed_ms: 0,
        };
    }

    let empty_killers = [[0u32; 2]; 64];
    let empty_history = [[0i32; COLS]; ROWS];
    order_moves(&mut moves, &PASS_MOVE, 0, &empty_killers, &empty_history, killer_history_on);
    apply_second_root_biases(&mut work_board, &mut moves, is_first, second_steal);
    order_moves(&mut moves, &PASS_MOVE, 0, &empty_killers, &empty_history, killer_history_on);

    let mut best_move = moves[0];
    let max_depth = 12i32;
    let search_start = Instant::now();
    let mut completed_depth = 0i32;

    if moves.len() <= 5 {
        let mut ctx = SearchCtx {
            start: search_start,
            budget_ms: budget,
            is_first,
            killer_history_on,
            nullmove_on,
            pass_mode,
            pass_fix,
            killers: [[0u32; 2]; 64],
            history: [[0i32; COLS]; ROWS],
            nullmove_ok: true,
        };
        let mut endgame_best = best_move;
        alpha_beta(&mut work_board, 8, -999999, 999999, &mut ctx, &mut endgame_best);
        if !ctx.timed_out() {
            return SearchReport {
                best_move: endgame_best,
                completed_depth: 8,
                nodes: NODES_SEARCHED.load(Ordering::Relaxed),
                elapsed_ms: search_start.elapsed().as_millis() as i64,
            };
        }
        best_move = endgame_best;
    }

    let mut prev_score = 0i32;
    let mut first_iteration = true;

    let mut ctx = SearchCtx {
        start: search_start,
        budget_ms: budget,
        is_first,
        killer_history_on,
        nullmove_on,
        pass_mode,
        pass_fix,
        killers: [[0u32; 2]; 64],
        history: [[0i32; COLS]; ROWS],
        nullmove_ok: true,
    };

    for depth in 1..=max_depth {
        let depth_start_ms = ctx.elapsed_ms();
        let mut depth_best = best_move;
        let score;

        if first_iteration || asp_window == 0 {
            score = alpha_beta(&mut work_board, depth, -999999, 999999, &mut ctx, &mut depth_best);
            first_iteration = false;
        } else {
            let w = if asp_window == -1 {
                std::cmp::max(200, prev_score.abs() / 4)
            } else {
                asp_window
            };
            let a = prev_score - w;
            let b_val = prev_score + w;
            let s = alpha_beta(&mut work_board, depth, a, b_val, &mut ctx, &mut depth_best);
            if s <= a || s >= b_val {
                score = alpha_beta(&mut work_board, depth, -999999, 999999, &mut ctx, &mut depth_best);
            } else {
                score = s;
            }
        }

        if !ctx.timed_out() {
            best_move = depth_best;
            prev_score = score;
            completed_depth = depth;
        }

        let depth_elapsed = ctx.elapsed_ms() - depth_start_ms;
        if ctx.timed_out() || depth_elapsed > budget / 2 {
            break;
        }
    }

    SearchReport {
        best_move,
        completed_depth,
        nodes: NODES_SEARCHED.load(Ordering::Relaxed),
        elapsed_ms: search_start.elapsed().as_millis() as i64,
    }
}
