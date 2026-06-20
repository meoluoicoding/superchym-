// protocol.rs — from protocol.hpp + protocol.cpp

use std::io::{BufRead, Write};
use crate::board::Board;
use crate::opponent_db::{g_opponent_db, G_ACTIVE_PRIOR_CONFIG, G_MATCHED_STYLE, G_MATCH_CONFIDENCE, OpponentFingerprint};
use crate::search::{search_best_move, ensure_tt_ready};
use crate::types::*;

pub struct Protocol {
    board: Board,
    i_am_first: bool,
    running: bool,
    opp_consecutive_passes: i32,
    opp_passes_since_our_move: i32,
    // QR data.bin: passive opponent fingerprint
    opp_fp: OpponentFingerprint,
    move_counter: i32,
    opp_move_counter: i32,
    ply_counter: i32,
    matched_style: KnownStyle,
    match_confidence: f32,
    fingerprint_checked: bool,
    fingerprint_check_interval: i32,
    last_check_move: i32,
}

impl Protocol {
    fn activate_side_prior(&self) {
        let db = g_opponent_db().lock().unwrap();
        let preferred_id = if self.i_am_first { 1 } else { 2 };
        let selected = db
            .get_prior_config(preferred_id)
            .or_else(|| db.default_prior_config())
            .cloned();
        drop(db);

        if let Some(cfg) = selected {
            let raw = Box::into_raw(Box::new(cfg));
            G_ACTIVE_PRIOR_CONFIG.store(raw, std::sync::atomic::Ordering::Relaxed);
        } else {
            G_ACTIVE_PRIOR_CONFIG.store(std::ptr::null_mut(), std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn new() -> Self {
        Protocol {
            board: Board::new(),
            i_am_first: false,
            running: true,
            opp_consecutive_passes: 0,
            opp_passes_since_our_move: 0,
            opp_fp: OpponentFingerprint::default(),
            move_counter: 0,
            opp_move_counter: 0,
            ply_counter: 0,
            matched_style: KnownStyle::Unknown,
            match_confidence: 0.0,
            fingerprint_checked: false,
            fingerprint_check_interval: 3,
            last_check_move: 0,
        }
    }

    pub fn run(&mut self) -> i32 {
        let stdin = std::io::stdin();
        let stdout = std::io::stdout();
        let mut out = stdout.lock();

        for line_res in stdin.lock().lines() {
            let line = match line_res {
                Ok(l) => l.trim_end_matches('\r').to_string(),
                Err(_) => break,
            };
            if line.is_empty() {
                continue;
            }

            if line.starts_with("READY") {
                self.handle_ready(&line, &mut out);
            } else if line.starts_with("INIT") {
                self.handle_init(&line, &mut out);
            } else if line.starts_with("TIME") {
                self.handle_time(&line, &mut out);
            } else if line.starts_with("OPP") {
                self.handle_opp(&line);
            } else if line.starts_with("FINISH") {
                self.log_shadow_metrics();
                self.running = false;
                break;
            }

            if !self.running {
                break;
            }
        }
        0
    }

    fn write_line(out: &mut dyn Write, s: &str) {
        writeln!(out, "{}", s).ok();
        out.flush().ok();
    }

    fn handle_ready(&mut self, line: &str, out: &mut dyn Write) {
        if line.contains("FIRST") {
            self.i_am_first = true;
            self.board.player = FIRST_PLAYER;
        } else {
            self.i_am_first = false;
            self.board.player = SECOND_PLAYER;
        }
        self.activate_side_prior();
        Self::write_line(out, "OK");
    }

    fn handle_init(&mut self, line: &str, out: &mut dyn Write) {
        // Format: "INIT row1 row2 ... row10"
        let board_str = &line[5..]; // skip "INIT "
        self.board.init_from_string(board_str);
        self.opp_consecutive_passes = 0;
        ensure_tt_ready();

        // Reset fingerprint state per game
        self.opp_fp = OpponentFingerprint::default();
        self.move_counter = 0;
        self.opp_move_counter = 0;
        self.ply_counter = 0;
        self.matched_style = KnownStyle::Unknown;
        self.match_confidence = 0.0;
        self.fingerprint_checked = false;
        self.fingerprint_check_interval = 3;
        self.last_check_move = 0;
        self.activate_side_prior();

        // INIT doesn't output anything in the original protocol
        let _ = out;
    }

    fn handle_time(&mut self, line: &str, out: &mut dyn Write) {
        // Format: "TIME our_remaining opp_remaining"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            Self::write_line(out, "-1 -1 -1 -1");
            self.board.apply_pass();
            return;
        }
        let our_time: i32 = parts[1].parse().unwrap_or(0);
        let _opp_time: i32 = parts[2].parse().unwrap_or(0);

        self.opp_passes_since_our_move = 0;

        // Dynamic time budget
        let mut live = 0i32;
        for r in 0..ROWS {
            for c in 0..COLS {
                if self.board.values[r][c] > 0 {
                    live += 1;
                }
            }
        }
        let est_moves_left = std::cmp::max(4, live / 4);
        let mut time_budget = our_time / est_moves_left;
        if time_budget < 20 { time_budget = 20; }
        if time_budget > 2500 { time_budget = 2500; }

        // Edge: TIME after terminal
        if self.board.consecutive_passes >= 2 && self.opp_consecutive_passes < 2 {
            Self::write_line(out, "-1 -1 -1 -1");
            return;
        }

        // Opponent passed twice — lock win if ahead
        if self.opp_consecutive_passes >= 2 {
            let opp = opponent(self.board.player);
            let margin = self.board.owned_cells(self.board.player)
                - self.board.owned_cells(opp);
            if margin > 0 {
                Self::write_line(out, "-1 -1 -1 -1");
                self.board.apply_pass();
                return;
            }
        }

        self.move_counter += 1;
        self.ply_counter += 1;

        let mut best = search_best_move(&self.board, time_budget, self.i_am_first);
        if !best.is_pass() && !self.board.is_legal_move(&best) {
            let fallback = self.board.legal_moves();
            best = if fallback.is_empty() { PASS_MOVE } else { fallback[0] };
        }

        if best.is_pass() {
            Self::write_line(out, "-1 -1 -1 -1");
        } else {
            Self::write_line(
                out,
                &format!("{} {} {} {}", best.r1, best.c1, best.r2, best.c2),
            );
        }
        self.board.apply_move(&best);
    }

    fn handle_opp(&mut self, line: &str) {
        // Format: "OPP r1 c1 r2 c2 time_ms"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            return;
        }
        let r1: i32 = parts[1].parse().unwrap_or(-1);
        let c1: i32 = parts[2].parse().unwrap_or(-1);
        let r2: i32 = parts[3].parse().unwrap_or(-1);
        let c2: i32 = parts[4].parse().unwrap_or(-1);
        let opp_move = Move { r1, c1, r2, c2, priority: 0 };

        if opp_move.is_pass() {
            self.opp_consecutive_passes += 1;
            self.opp_passes_since_our_move += 1;
            // Only apply first opp pass since our move; second+ = Always Pass artifact
            if self.opp_passes_since_our_move <= 1 {
                self.board.apply_move(&opp_move);
            }
        } else {
            // Measure steal BEFORE apply
            let mut steal_before = 0i32;
            for r in opp_move.r1..=opp_move.r2 {
                for c in opp_move.c1..=opp_move.c2 {
                    if self.board.owners[r as usize][c as usize] == self.board.player {
                        steal_before += 1;
                    }
                }
            }
            self.opp_consecutive_passes = 0;
            self.opp_passes_since_our_move = 0;
            self.board.apply_move(&opp_move);
            if steal_before > 0 {
                self.opp_fp.steal_seen += 1;
            }
        }

        self.ply_counter += 1;

        if !opp_move.is_pass() {
            self.opp_move_counter += 1;
            self.opp_fp.move_count = self.opp_move_counter;
            self.opp_fp.side_ply = self.ply_counter;
            self.opp_fp.we_are_first = self.i_am_first;

            let area = move_area(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
            self.opp_fp.total_area += area;
            let sc = classify_shape(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2) as usize;
            self.opp_fp.shape_counts[sc] += 1;
            if area >= 5 && area <= 10 { self.opp_fp.medium_count += 1; }
            if area >= 11 { self.opp_fp.large_count += 1; }

            let orient = classify_orientation(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
            if orient == Orientation::Portrait { self.opp_fp.tall_count += 1; }
            if orient == Orientation::Landscape { self.opp_fp.wide_count += 1; }

            let rt = classify_region(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
            if rt != RegionTag::None {
                self.opp_fp.region_counts[rt as usize] += 1;
            }

            if self.board.barrier_potential(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2) > 0 {
                self.opp_fp.barrier_freq += 1;
            }
        } else {
            self.opp_fp.pass_seen += 1;
            if self.opp_fp.first_pass_ply == 0 {
                self.opp_fp.first_pass_ply = self.ply_counter;
            }
        }

        // Try to match known fingerprint after sufficient observations
        // Re-check periodically to improve accuracy as more data accumulates
        let should_check = if !self.fingerprint_checked && self.opp_move_counter >= 5 {
            true
        } else if self.fingerprint_checked
            && self.opp_move_counter >= self.last_check_move + self.fingerprint_check_interval
            && self.match_confidence < 0.80
        {
            true
        } else {
            false
        };

        if should_check {
                let db = g_opponent_db().lock().unwrap();
            let fps = db.fingerprints();
            let mut conf = 0.0f32;
            let mut margin = 0.0f32;
            let style = self.opp_fp.match_fingerprint(fps, &mut conf, &mut margin);

            if style != KnownStyle::Unknown && conf >= 0.40 {
                let old_style = self.matched_style;
                self.matched_style = style;
                self.match_confidence = conf;
                self.fingerprint_checked = true;
                self.last_check_move = self.opp_move_counter;

                // Update globals for search engine
                G_MATCHED_STYLE.store(style as u32, std::sync::atomic::Ordering::Relaxed);
                G_MATCH_CONFIDENCE.store((conf * 100.0) as i32, std::sync::atomic::Ordering::Relaxed);

                // Only update prior config if style changed or first match
                if old_style != style || !self.fingerprint_checked {
                    // Select style-specific counter prior config
                    for fp in fps {
                        let fp_style = match fp.style {
                            1 => KnownStyle::CordycepsAttack,
                            2 => KnownStyle::CordycepsDefense,
                            3 => KnownStyle::CordycepsBalanced,
                            4 => KnownStyle::RustOld,
                            5 => KnownStyle::RustUpdate,
                            _ => KnownStyle::Unknown,
                        };
                        if fp_style == style {
                            if let Some(cfg) = db.get_prior_config(fp.prior_config_id) {
                                // Leak a Box to get a 'static pointer — small, lives for program lifetime
                                let boxed = Box::new(cfg.clone());
                                let raw = Box::into_raw(boxed);
                                G_ACTIVE_PRIOR_CONFIG.store(
                                    raw,
                                    std::sync::atomic::Ordering::Relaxed,
                                );
                            }
                            break;
                        }
                    }
                }
            } else if self.fingerprint_checked {
                // Update last_check_move even if no new match
                self.last_check_move = self.opp_move_counter;
            }
        }
    }

    fn log_shadow_metrics(&self) {
        #[cfg(not(feature = "online_judge"))]
        {
            static GAME_COUNT: std::sync::atomic::AtomicI32 =
                std::sync::atomic::AtomicI32::new(0);
            let game = GAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

            let fv = self.opp_fp.to_feature_vector();
            let side = if self.i_am_first { "FIRST" } else { "SECOND" };
            let style = match self.matched_style {
                KnownStyle::CordycepsAttack => "CORDYCEPS_ATTACK",
                KnownStyle::CordycepsDefense => "CORDYCEPS_DEFENSE",
                KnownStyle::CordycepsBalanced => "CORDYCEPS_BALANCED",
                KnownStyle::RustOld => "RUST_OLD",
                KnownStyle::RustUpdate => "RUST_UPDATE",
                KnownStyle::Unknown => "UNKNOWN",
            };
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("fingerprint_shadow.log")
            {
                writeln!(
                    f,
                    "G{} {} moves={} opp_moves={} matched={} conf={:.2} \
                     fv=[{},{},{},{},{},{},{},{}] ply={}",
                    game, side, self.move_counter, self.opp_move_counter,
                    style, self.match_confidence,
                    fv.dim[0], fv.dim[1], fv.dim[2], fv.dim[3],
                    fv.dim[4], fv.dim[5], fv.dim[6], fv.dim[7],
                    self.ply_counter
                ).ok();
            }
        }
    }
}
