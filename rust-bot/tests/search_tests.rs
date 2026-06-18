use mushroom_bot::search::{search_best_move, search_best_move_report};
use mushroom_bot::types::PASS_MOVE;

mod common;

#[test]
fn search_report_respects_ten_second_budget_and_tracks_depth() {
    let board = common::dense_board();
    let report = search_best_move_report(&board, 10_000, true);

    assert!(report.elapsed_ms <= 10_000, "search exceeded budget: {}ms", report.elapsed_ms);
    assert!(report.completed_depth >= 1, "completed_depth={}", report.completed_depth);
    assert!(report.nodes > 0, "nodes={}", report.nodes);
    assert!(report.best_move.is_pass() || board.is_legal_move(&report.best_move));
}

#[test]
fn search_best_move_matches_report_best_move() {
    let board = common::sparse_board();
    let best = search_best_move(&board, 10_000, true);
    let report = search_best_move_report(&board, 10_000, true);

    assert_ne!(best, PASS_MOVE);
    assert!(best.coords_eq(&report.best_move));
}
