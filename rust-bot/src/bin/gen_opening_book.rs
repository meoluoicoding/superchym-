use mushroom_bot::board::Board;
use mushroom_bot::search::{search_best_move_report, SearchReport};
use mushroom_bot::types::*;
use std::collections::HashMap;
use std::fs;

const OPENING_PLY: i32 = 8;

fn board_key(board: &Board) -> String {
    let mut s = String::with_capacity(ROWS * COLS + ROWS);
    for r in 0..ROWS {
        for c in 0..COLS {
            s.push((b'0' + board.values[r][c] as u8) as char);
        }
        s.push(' ');
    }
    s.pop(); // remove trailing space
    s
}

fn apply_move_to_rows(rows: &mut Vec<String>, mv: &Move) {
    if mv.is_pass() {
        return;
    }
    for r in mv.r1..=mv.r2 {
        for c in mv.c1..=mv.c2 {
            let ru = r as usize;
            let cu = c as usize;
            let mut chars: Vec<char> = rows[ru].chars().collect();
            chars[cu] = '0';
            rows[ru] = chars.into_iter().collect();
        }
    }
}

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = args.get(1).map(|s| s.as_str()).unwrap_or("input.txt");
    let output_path = args.get(2).map(|s| s.as_str()).unwrap_or("opening_book.txt");
    
    let boards = read_boards(input_path);
    if boards.is_empty() {
        eprintln!("No boards found in {}", input_path);
        return;
    }
    
    let mut book: HashMap<String, (i32, i32, i32, i32)> = HashMap::new();
    
    for (board_idx, board_rows) in boards.iter().enumerate() {
        eprintln!("Processing board {}/{}...", board_idx + 1, boards.len());
        
        let mut board = Board::new();
        let init_str = board_rows.join(" ");
        board.init_from_string(&init_str);
        
        let mut current_rows = board_rows.clone();
        
        for ply in 0..OPENING_PLY {
            let key = board_key(&board);
            
            if let Some(&mv) = book.get(&key) {
                eprintln!("  Ply {}: cached ({},{})-({},{})", ply, mv.0, mv.1, mv.2, mv.3);
                apply_move_to_rows(&mut current_rows, &Move { r1: mv.0, c1: mv.1, r2: mv.2, c2: mv.3, priority: 0 });
                // Apply to board too
                let mv_obj = Move { r1: mv.0, c1: mv.1, r2: mv.2, c2: mv.3, priority: 0 };
                board.apply_move(&mv_obj);
                continue;
            }
            
            // Deep search for opening
            let report: SearchReport = search_best_move_report(&board, 1000, true);
            let mv = report.best_move;
            
            if mv.is_pass() {
                eprintln!("  Ply {}: pass (depth={}, nodes={})", ply, report.completed_depth, report.nodes);
                break;
            }
            
            eprintln!("  Ply {}: ({},{})-({},{}) depth={} nodes={} time={}ms",
                ply, mv.r1, mv.c1, mv.r2, mv.c2, report.completed_depth, report.nodes, report.elapsed_ms);
            
            book.insert(key, (mv.r1, mv.c1, mv.r2, mv.c2));
            
            apply_move_to_rows(&mut current_rows, &mv);
            board.apply_move(&mv);
        }
    }
    
    // Write book
    let mut output = String::new();
    output.push_str(&format!("# Opening book: {} entries, {} boards\n", book.len(), boards.len()));
    output.push_str("# key -> r1 c1 r2 c2\n\n");
    
    for (key, mv) in book.iter() {
        output.push_str(&format!("{}\n", key));
        output.push_str(&format!("{} {} {} {}\n\n", mv.0, mv.1, mv.2, mv.3));
    }
    
    fs::write(output_path, &output).expect("write opening book");
    eprintln!("\nGenerated {} entries, saved to {}", book.len(), output_path);
}
