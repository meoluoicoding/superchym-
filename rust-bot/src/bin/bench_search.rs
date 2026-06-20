use mushroom_bot::board::Board;
use mushroom_bot::search::{search_best_move_report, SearchReport};
use mushroom_bot::types::*;
use std::fs;
use std::time::Instant;

fn read_boards(path: &str) -> Vec<Vec<String>> {
    let content = fs::read_to_string(path).unwrap_or_default();
    let mut boards = Vec::new();
    let mut board = Vec::new();
    for line in content.lines() {
        let line = line.trim().to_string();
        if line.is_empty() {
            if !board.is_empty() {
                boards.push(board);
                board = Vec::new();
            }
        } else {
            board.push(line);
        }
    }
    if !board.is_empty() {
        boards.push(board);
    }
    boards
}

fn random_board_string(rng: &mut u64) -> Vec<String> {
    let mut rows = Vec::new();
    for _ in 0..ROWS {
        let mut row = String::new();
        for _ in 0..COLS {
            // Simple LCG
            *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            let digit = ((*rng >> 33) % 9 + 1) as u8;
            row.push((b'0' + digit) as char);
        }
        rows.push(row);
    }
    rows
}

/// Generate a sparse board: start with random, then remove random cells
fn random_sparse_board(rng: &mut u64, target_live: usize) -> Vec<String> {
    let mut grid = [[0i8; COLS]; ROWS];
    // Fill with random values
    for r in 0..ROWS {
        for c in 0..COLS {
            *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
            grid[r][c] = ((*rng >> 33) % 9 + 1) as i8;
        }
    }
    // Remove random cells until target_live
    let mut live = ROWS * COLS;
    while live > target_live {
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let r = ((*rng >> 33) % ROWS as u64) as usize;
        *rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        let c = ((*rng >> 33) % COLS as u64) as usize;
        if grid[r][c] > 0 {
            grid[r][c] = 0;
            live -= 1;
        }
    }
    let mut rows = Vec::new();
    for r in 0..ROWS {
        let mut row = String::new();
        for c in 0..COLS {
            row.push((b'0' + grid[r][c] as u8) as char);
        }
        rows.push(row);
    }
    rows
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let time_budget: i32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
    
    let mut rng: u64 = 0xDEAD_BEEF_CAFE_1234;
    
    println!("=== RANDOM BOARD SEARCH SPEED TEST ===");
    println!("Time budget: {}ms per move", time_budget);
    println!();
    
    // Test on different sparsity levels
    let configs: Vec<(&str, Option<usize>)> = vec![
        ("Full board (170 live)", None),
        ("Mid game (80 live)", Some(80)),
        ("Late game (40 live)", Some(40)),
        ("Endgame (15 live)", Some(15)),
    ];
    
    for (label, target_live) in &configs {
        println!("--- {} ---", label);
        let mut total_nodes = 0u64;
        let mut total_time = 0i64;
        let mut total_depth = 0i32;
        let mut count = 0i32;
        
        for i in 0..5 {
            let board_rows = match target_live {
                Some(tl) => random_sparse_board(&mut rng, *tl),
                None => random_board_string(&mut rng),
            };
            
            let mut board = Board::new();
            let init_str = board_rows.join(" ");
            board.init_from_string(&init_str);
            
            let report = search_best_move_report(&board, time_budget, true);
            eprintln!("  Board {}: depth={}, nodes={}, time={}ms",
                i + 1, report.completed_depth, report.nodes, report.elapsed_ms);
            
            total_nodes += report.nodes;
            total_time += report.elapsed_ms;
            total_depth += report.completed_depth;
            count += 1;
        }
        
        let avg_depth = total_depth / count;
        let avg_nodes = total_nodes / count as u64;
        let avg_time = total_time / count as i64;
        let nodes_per_sec = if avg_time > 0 { total_nodes * 1000 / total_time as u64 } else { 0 };
        
        println!("  Average: depth={}, nodes={}, time={}ms, nodes/sec={}",
            avg_depth, avg_nodes, avg_time, nodes_per_sec);
        println!();
    }
}
