use crate::types::*;

use super::tt::pack_move;

// ====== Move Ordering ======

pub(super) fn order_moves(
    moves: &mut Vec<Move>,
    pv_move: &Move,
    depth: i32,
    killers: &[[u32; 2]; 64],
    history: &[[i32; COLS]; ROWS],
    killer_history_on: bool,
) {
    if killer_history_on && depth >= 0 && depth < 64 {
        let d = depth as usize;
        for m in moves.iter_mut() {
            let pm = pack_move(m);
            if pm == killers[d][0] {
                m.priority += 9000;
            } else if pm == killers[d][1] {
                m.priority += 8000;
            } else {
                let h = history[m.r2 as usize][m.c2 as usize];
                if h > 0 { m.priority += h * 10; }
            }
        }
    }
    let pv = *pv_move;
    moves.sort_unstable_by(|a, b| {
        let a_pv = a.coords_eq(&pv);
        let b_pv = b.coords_eq(&pv);
        if a_pv && !b_pv { return std::cmp::Ordering::Less; }
        if b_pv && !a_pv { return std::cmp::Ordering::Greater; }
        b.priority.cmp(&a.priority)
    });
}
