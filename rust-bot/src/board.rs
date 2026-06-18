// board.rs — from board.hpp + board.cpp

use crate::movegen::generate_legal_moves;
use crate::opponent_db::G_ACTIVE_PRIOR_CONFIG;
use crate::search::zobrist::{cell_hash, hash_board, passes_hash, player_hash};
use crate::types::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvalCache {
    pub owned_first: i32,
    pub owned_second: i32,
    pub connectivity_first: i32,
    pub connectivity_second: i32,
    pub corners_first: i32,
    pub corners_second: i32,
    pub edges_first: i32,
    pub edges_second: i32,
    pub live_adjacent_first: i32,
    pub live_adjacent_second: i32,
}

// MoveRecord for undo — stores all changes made by a move.
#[derive(Clone, Copy, Debug, Default)]
pub struct CellChange {
    pub r: i8,
    pub c: i8,
    pub old_value: i8,
    pub old_owner: i8,
}

const MAX_CELL_CHANGES: usize = ROWS * COLS;

pub struct MoveRecord {
    pub changes: [CellChange; MAX_CELL_CHANGES],
    pub change_count: usize,
    pub old_player: i8,
    pub old_consecutive_passes: i32,
    pub old_hash: u64,
    pub old_eval_cache: EvalCache,
    pub was_pass: bool,
}

impl MoveRecord {
    pub fn new() -> Self {
        MoveRecord {
            changes: [CellChange::default(); MAX_CELL_CHANGES],
            change_count: 0,
            old_player: 0,
            old_consecutive_passes: 0,
            old_hash: 0,
            old_eval_cache: EvalCache::default(),
            was_pass: false,
        }
    }
}

#[derive(Clone)]
pub struct Board {
    pub values: ValueGrid,
    pub owners: OwnerGrid,
    pub player: i8,
    pub consecutive_passes: i32,
    pub hash: u64,
    pub eval_cache: EvalCache,
}

impl Board {
    pub fn new() -> Self {
        let mut board = Board {
            values: [[0i8; COLS]; ROWS],
            owners: [[NO_OWNER; COLS]; ROWS],
            player: FIRST_PLAYER,
            consecutive_passes: 0,
            hash: 0,
            eval_cache: EvalCache::default(),
        };
        board.recompute_hash();
        board.recompute_eval_cache();
        board
    }

    fn recompute_hash(&mut self) {
        self.hash = hash_board(self);
    }

    fn recompute_eval_cache(&mut self) {
        self.eval_cache = EvalCache::default();
        for r in 0..ROWS {
            for c in 0..COLS {
                let owner = self.owners[r][c];
                Self::adjust_owner_count(&mut self.eval_cache, owner, 1);
                if Self::cell_is_corner(r, c) {
                    Self::adjust_corner_count(&mut self.eval_cache, owner, 1);
                }
                if Self::cell_is_edge(r, c) {
                    Self::adjust_edge_count(&mut self.eval_cache, owner, 1);
                }
                if owner != NO_OWNER && self.cell_has_live_neighbor(r, c) {
                    Self::adjust_live_adjacent_count(&mut self.eval_cache, owner, 1);
                }
                if c + 1 < COLS && owner != NO_OWNER && self.owners[r][c + 1] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, 1);
                }
                if r + 1 < ROWS && owner != NO_OWNER && self.owners[r + 1][c] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, 1);
                }
            }
        }
    }

    fn update_hash_before_turn_change(&mut self) {
        self.hash ^= player_hash(self.player);
        self.hash ^= passes_hash(self.consecutive_passes);
    }

    fn update_hash_after_turn_change(&mut self) {
        self.hash ^= player_hash(self.player);
        self.hash ^= passes_hash(self.consecutive_passes);
    }

    fn replace_cell(&mut self, r: usize, c: usize, new_value: i8, new_owner: i8) {
        self.hash ^= cell_hash(r, c, self.values[r][c], self.owners[r][c]);
        self.values[r][c] = new_value;
        self.owners[r][c] = new_owner;
        self.hash ^= cell_hash(r, c, self.values[r][c], self.owners[r][c]);
    }

    fn cell_has_live_neighbor(&self, r: usize, c: usize) -> bool {
        (r > 0 && self.values[r - 1][c] > 0)
            || (r + 1 < ROWS && self.values[r + 1][c] > 0)
            || (c > 0 && self.values[r][c - 1] > 0)
            || (c + 1 < COLS && self.values[r][c + 1] > 0)
    }

    fn cell_is_corner(r: usize, c: usize) -> bool {
        (r == 0 || r == ROWS - 1) && (c == 0 || c == COLS - 1)
    }

    fn cell_is_edge(r: usize, c: usize) -> bool {
        r == 0 || r == ROWS - 1 || c == 0 || c == COLS - 1
    }

    fn adjust_owner_count(cache: &mut EvalCache, owner: i8, delta: i32) {
        if owner == FIRST_PLAYER {
            cache.owned_first += delta;
        } else if owner == SECOND_PLAYER {
            cache.owned_second += delta;
        }
    }

    fn adjust_corner_count(cache: &mut EvalCache, owner: i8, delta: i32) {
        if owner == FIRST_PLAYER {
            cache.corners_first += delta;
        } else if owner == SECOND_PLAYER {
            cache.corners_second += delta;
        }
    }

    fn adjust_edge_count(cache: &mut EvalCache, owner: i8, delta: i32) {
        if owner == FIRST_PLAYER {
            cache.edges_first += delta;
        } else if owner == SECOND_PLAYER {
            cache.edges_second += delta;
        }
    }

    fn adjust_connectivity_count(cache: &mut EvalCache, owner: i8, delta: i32) {
        if owner == FIRST_PLAYER {
            cache.connectivity_first += delta;
        } else if owner == SECOND_PLAYER {
            cache.connectivity_second += delta;
        }
    }

    fn adjust_live_adjacent_count(cache: &mut EvalCache, owner: i8, delta: i32) {
        if owner == FIRST_PLAYER {
            cache.live_adjacent_first += delta;
        } else if owner == SECOND_PLAYER {
            cache.live_adjacent_second += delta;
        }
    }

    fn add_flag(flagged: &mut [[bool; COLS]; ROWS], r: i32, c: i32) {
        if r >= 0 && r < ROWS as i32 && c >= 0 && c < COLS as i32 {
            flagged[r as usize][c as usize] = true;
        }
    }

    fn update_eval_cache_for_move(&mut self, mv: &Move) {
        if mv.is_pass() {
            return;
        }

        let mut impacted = [[false; COLS]; ROWS];
        let mut changed = [[false; COLS]; ROWS];
        for r in mv.r1..=mv.r2 {
            for c in mv.c1..=mv.c2 {
                let ru = r as usize;
                let cu = c as usize;
                changed[ru][cu] = true;
                Self::add_flag(&mut impacted, r, c);
                Self::add_flag(&mut impacted, r - 1, c);
                Self::add_flag(&mut impacted, r + 1, c);
                Self::add_flag(&mut impacted, r, c - 1);
                Self::add_flag(&mut impacted, r, c + 1);
            }
        }

        for r in 0..ROWS {
            for c in 0..COLS {
                if !impacted[r][c] {
                    continue;
                }
                let owner = self.owners[r][c];
                if owner != NO_OWNER && self.cell_has_live_neighbor(r, c) {
                    Self::adjust_live_adjacent_count(&mut self.eval_cache, owner, -1);
                }
            }
        }

        for r in 0..ROWS {
            for c in 0..COLS {
                let owner = self.owners[r][c];
                if owner == NO_OWNER {
                    continue;
                }
                if c + 1 < COLS && (changed[r][c] || changed[r][c + 1]) && self.owners[r][c + 1] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, -1);
                }
                if r + 1 < ROWS && (changed[r][c] || changed[r + 1][c]) && self.owners[r + 1][c] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, -1);
                }
            }
        }

        for r in mv.r1..=mv.r2 {
            for c in mv.c1..=mv.c2 {
                let ru = r as usize;
                let cu = c as usize;
                let old_owner = self.owners[ru][cu];
                Self::adjust_owner_count(&mut self.eval_cache, old_owner, -1);
                if Self::cell_is_corner(ru, cu) {
                    Self::adjust_corner_count(&mut self.eval_cache, old_owner, -1);
                }
                if Self::cell_is_edge(ru, cu) {
                    Self::adjust_edge_count(&mut self.eval_cache, old_owner, -1);
                }

                self.replace_cell(ru, cu, 0, self.player);

                let new_owner = self.owners[ru][cu];
                Self::adjust_owner_count(&mut self.eval_cache, new_owner, 1);
                if Self::cell_is_corner(ru, cu) {
                    Self::adjust_corner_count(&mut self.eval_cache, new_owner, 1);
                }
                if Self::cell_is_edge(ru, cu) {
                    Self::adjust_edge_count(&mut self.eval_cache, new_owner, 1);
                }
            }
        }

        for r in 0..ROWS {
            for c in 0..COLS {
                let owner = self.owners[r][c];
                if owner == NO_OWNER {
                    continue;
                }
                if c + 1 < COLS && (changed[r][c] || changed[r][c + 1]) && self.owners[r][c + 1] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, 1);
                }
                if r + 1 < ROWS && (changed[r][c] || changed[r + 1][c]) && self.owners[r + 1][c] == owner {
                    Self::adjust_connectivity_count(&mut self.eval_cache, owner, 1);
                }
            }
        }

        for r in 0..ROWS {
            for c in 0..COLS {
                if !impacted[r][c] {
                    continue;
                }
                let owner = self.owners[r][c];
                if owner != NO_OWNER && self.cell_has_live_neighbor(r, c) {
                    Self::adjust_live_adjacent_count(&mut self.eval_cache, owner, 1);
                }
            }
        }
    }

    pub fn init_from_string(&mut self, board_str: &str) {
        let mut tokens = board_str.split_whitespace();
        for r in 0..ROWS {
            let row_str = tokens.next().expect("INIT: insufficient rows (expected 10)");
            assert_eq!(
                row_str.len(),
                COLS,
                "INIT: row {} has {} columns, expected {}",
                r,
                row_str.len(),
                COLS
            );
            for (c, ch) in row_str.chars().enumerate() {
                assert!(ch.is_ascii_digit(), "INIT: invalid digit '{}' at ({},{})", ch, r, c);
                self.values[r][c] = (ch as u8 - b'0') as i8;
            }
        }
        for row in self.owners.iter_mut() {
            row.fill(NO_OWNER);
        }
        self.consecutive_passes = 0;
        self.recompute_hash();
        self.recompute_eval_cache();
    }

    pub fn apply_pass(&mut self) {
        self.update_hash_before_turn_change();
        self.consecutive_passes += 1;
        self.player = opponent(self.player);
        self.update_hash_after_turn_change();
    }

    pub fn apply_move(&mut self, mv: &Move) {
        if mv.is_pass() {
            self.apply_pass();
            return;
        }
        self.update_hash_before_turn_change();
        self.update_eval_cache_for_move(mv);
        self.consecutive_passes = 0;
        self.player = opponent(self.player);
        self.update_hash_after_turn_change();
    }

    pub fn make_move(&mut self, mv: &Move, record: &mut MoveRecord) {
        record.old_player = self.player;
        record.old_consecutive_passes = self.consecutive_passes;
        record.old_hash = self.hash;
        record.old_eval_cache = self.eval_cache;
        record.change_count = 0;

        if mv.is_pass() {
            record.was_pass = true;
            self.update_hash_before_turn_change();
            self.consecutive_passes += 1;
            self.player = opponent(self.player);
            self.update_hash_after_turn_change();
            return;
        }

        record.was_pass = false;
        self.update_hash_before_turn_change();
        for r in mv.r1..=mv.r2 {
            for c in mv.c1..=mv.c2 {
                record.changes[record.change_count] = CellChange {
                    r: r as i8,
                    c: c as i8,
                    old_value: self.values[r as usize][c as usize],
                    old_owner: self.owners[r as usize][c as usize],
                };
                record.change_count += 1;
            }
        }
        self.update_eval_cache_for_move(mv);
        self.consecutive_passes = 0;
        self.player = opponent(self.player);
        self.update_hash_after_turn_change();
    }

    pub fn unmake_move(&mut self, record: &MoveRecord) {
        self.player = record.old_player;
        self.consecutive_passes = record.old_consecutive_passes;
        self.hash = record.old_hash;
        self.eval_cache = record.old_eval_cache;
        if record.was_pass {
            return;
        }
        for ch in &record.changes[..record.change_count] {
            self.values[ch.r as usize][ch.c as usize] = ch.old_value;
            self.owners[ch.r as usize][ch.c as usize] = ch.old_owner;
        }
    }

    pub fn is_legal_move(&self, mv: &Move) -> bool {
        if mv.is_pass() {
            return true;
        }
        if mv.r1 < 0 || mv.r2 >= ROWS as i32 || mv.c1 < 0 || mv.c2 >= COLS as i32 {
            return false;
        }
        if mv.r1 > mv.r2 || mv.c1 > mv.c2 {
            return false;
        }
        let mut sum = 0i32;
        for r in mv.r1..=mv.r2 {
            for c in mv.c1..=mv.c2 {
                let v = self.values[r as usize][c as usize];
                if v > 0 {
                    sum += v as i32;
                    if sum > 10 {
                        return false;
                    }
                }
            }
        }
        if sum != 10 {
            return false;
        }
        let mut top = false;
        let mut bottom = false;
        let mut left = false;
        let mut right = false;
        for c in mv.c1..=mv.c2 {
            if self.values[mv.r1 as usize][c as usize] > 0 {
                top = true;
            }
            if self.values[mv.r2 as usize][c as usize] > 0 {
                bottom = true;
            }
        }
        for r in mv.r1..=mv.r2 {
            if self.values[r as usize][mv.c1 as usize] > 0 {
                left = true;
            }
            if self.values[r as usize][mv.c2 as usize] > 0 {
                right = true;
            }
        }
        top && bottom && left && right
    }

    pub fn legal_moves(&self) -> Vec<Move> {
        let mut moves = generate_legal_moves(&self.values);
        let cfg_ptr = G_ACTIVE_PRIOR_CONFIG.load(std::sync::atomic::Ordering::Relaxed);
        for m in moves.iter_mut() {
            if m.is_pass() {
                m.priority = 0;
                continue;
            }
            let mut steal = 0i32;
            let area = (m.r2 - m.r1 + 1) * (m.c2 - m.c1 + 1);
            for r in m.r1..=m.r2 {
                for c in m.c1..=m.c2 {
                    if self.owners[r as usize][c as usize] == opponent(self.player) {
                        steal += 1;
                    }
                }
            }
            let height = m.r2 - m.r1 + 1;
            let width = m.c2 - m.c1 + 1;
            let portrait_bonus = if height > width { 500 } else { 0 };
            let small_bonus = if area <= 4 { 300 } else { 0 };
            m.priority = steal * 1000 + area + portrait_bonus + small_bonus;

                if !cfg_ptr.is_null() {
                    let cfg = unsafe { &*cfg_ptr };
                    let mut adj = 0i32;
                    let sc = classify_shape(m.r1, m.c1, m.r2, m.c2) as usize;
                    adj += cfg.shape_boost[sc] as i32;
                    if self.player == FIRST_PLAYER {
                        adj += cfg.side_boost_first as i32;
                    } else if self.player == SECOND_PLAYER {
                        adj += cfg.side_boost_second as i32;
                    }
                    if area >= 5 && area <= 10 {
                        adj += cfg.medium_rect_boost as i32;
                    }
                if self.barrier_potential(m.r1, m.c1, m.r2, m.c2) > 0 {
                    adj += cfg.barrier_boost as i32;
                }
                adj += self.connectivity_boost(m.r1, m.c1, m.r2, m.c2)
                    * cfg.connection_boost as i32
                    / 4;
                adj -= self.dead_cell_risk_proxy(m.r1, m.c1, m.r2, m.c2)
                    * cfg.dead_cell_risk_penalty as i32
                    / 4;
                let cap = cfg.max_total_adjustment as i32;
                adj = adj.clamp(-cap, cap);
                m.priority += adj;
            }
        }
        moves
    }

    pub fn owned_cells(&self, player: i8) -> i32 {
        if player == FIRST_PLAYER {
            self.eval_cache.owned_first
        } else if player == SECOND_PLAYER {
            self.eval_cache.owned_second
        } else {
            0
        }
    }

    pub fn barrier_potential(&self, r1: i32, c1: i32, r2: i32, c2: i32) -> i32 {
        let h = r2 - r1 + 1;
        let w = c2 - c1 + 1;
        if h >= 4 && w <= 2 {
            let cc = (c1 + c2) / 2;
            if cc >= 4 && cc <= 12 {
                return 2;
            }
            return 1;
        }
        if w >= 6 && h <= 2 {
            let cr = (r1 + r2) / 2;
            if cr >= 3 && cr <= 6 {
                return 2;
            }
            return 1;
        }
        0
    }

    pub fn dead_cell_risk_proxy(&self, r1: i32, c1: i32, r2: i32, c2: i32) -> i32 {
        let mut risk = 0i32;
        for r in r1..=r2 {
            for c in c1..=c2 {
                let ru = r as usize;
                let cu = c as usize;
                if self.owners[ru][cu] == self.player {
                    continue;
                }
                let mut protection = 0i32;
                if r > 0 && self.owners[ru - 1][cu] == self.player {
                    protection += 1;
                }
                if r < ROWS as i32 - 1 && self.owners[ru + 1][cu] == self.player {
                    protection += 1;
                }
                if c > 0 && self.owners[ru][cu - 1] == self.player {
                    protection += 1;
                }
                if c < COLS as i32 - 1 && self.owners[ru][cu + 1] == self.player {
                    protection += 1;
                }
                let adjacent_live = self.cell_has_live_neighbor(ru, cu);
                if protection == 0 && adjacent_live {
                    risk += 2;
                } else if protection <= 1 && adjacent_live {
                    risk += 1;
                }
            }
        }
        risk
    }

    pub fn connectivity_boost(&self, r1: i32, c1: i32, r2: i32, c2: i32) -> i32 {
        let mut boost = 0i32;
        for r in r1..=r2 {
            for c in c1..=c2 {
                let ru = r as usize;
                let cu = c as usize;
                if self.owners[ru][cu] == self.player {
                    continue;
                }
                if r > 0 && self.owners[ru - 1][cu] == self.player {
                    boost += 1;
                }
                if r < ROWS as i32 - 1 && self.owners[ru + 1][cu] == self.player {
                    boost += 1;
                }
                if c > 0 && self.owners[ru][cu - 1] == self.player {
                    boost += 1;
                }
                if c < COLS as i32 - 1 && self.owners[ru][cu + 1] == self.player {
                    boost += 1;
                }
            }
        }
        boost
    }
}
