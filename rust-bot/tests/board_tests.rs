use mushroom_bot::search::hash_board;
use mushroom_bot::board::MoveRecord;
use mushroom_bot::types::{Move, FIRST_PLAYER, NO_OWNER, SECOND_PLAYER};

mod common;

#[test]
fn init_from_string_populates_values_and_clears_owners() {
    let board = common::sparse_board();
    assert_eq!(board.values[0][0], 1);
    assert_eq!(board.values[0][9], 1);
    assert_eq!(board.values[0][10], 0);
    assert_eq!(board.owners[0][0], NO_OWNER);
    assert_eq!(board.hash, hash_board(&board));
}

#[test]
fn make_and_unmake_restore_board_state() {
    let mut board = common::sparse_board();
    board.player = FIRST_PLAYER;
    let original = board.clone();
    let mv = Move { r1: 0, c1: 0, r2: 0, c2: 9, priority: 0 };
    let mut rec = MoveRecord::new();

    board.make_move(&mv, &mut rec);
    assert_eq!(board.player, SECOND_PLAYER);
    assert_eq!(board.consecutive_passes, 0);
    assert_eq!(board.values[0][0], 0);
    assert_eq!(board.owners[0][0], FIRST_PLAYER);
    assert_eq!(board.hash, hash_board(&board));

    board.unmake_move(&rec);
    assert_eq!(board.values, original.values);
    assert_eq!(board.owners, original.owners);
    assert_eq!(board.player, original.player);
    assert_eq!(board.consecutive_passes, original.consecutive_passes);
    assert_eq!(board.hash, original.hash);
    assert_eq!(board.hash, hash_board(&board));
}

#[test]
fn board_legal_moves_matches_known_rectangle() {
    let board = common::sparse_board();
    let mv = Move { r1: 0, c1: 0, r2: 0, c2: 9, priority: 0 };
    assert!(board.is_legal_move(&mv));
    assert!(board.legal_moves().iter().any(|cand| cand.coords_eq(&mv)));
}
