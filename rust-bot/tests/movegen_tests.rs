use mushroom_bot::movegen::generate_legal_moves;
use mushroom_bot::types::Move;

mod common;

#[test]
fn movegen_finds_expected_ten_sum_rectangle() {
    let board = common::sparse_board();
    let moves = generate_legal_moves(&board.values);
    let expected = Move { r1: 0, c1: 0, r2: 0, c2: 9, priority: 0 };
    assert!(moves.iter().any(|mv| mv.coords_eq(&expected)));
}

#[test]
fn movegen_returns_no_moves_for_empty_board() {
    let values = [[0i8; 17]; 10];
    let moves = generate_legal_moves(&values);
    assert!(moves.is_empty());
}
