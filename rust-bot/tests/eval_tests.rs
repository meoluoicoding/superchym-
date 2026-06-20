use mushroom_bot::eval::{evaluate, score_move};
use mushroom_bot::types::{opponent, Move, COLS, FIRST_PLAYER, ROWS};

mod common;

fn slow_evaluate(board: &mushroom_bot::board::Board) -> i32 {
    let player = board.player;
    let opp = opponent(player);
    let owners = &board.owners;
    let values = &board.values;

    let mut own_cells = 0i32;
    let mut opp_cells = 0i32;
    for r in 0..ROWS {
        for c in 0..COLS {
            if owners[r][c] == player {
                own_cells += 1;
            } else if owners[r][c] == opp {
                opp_cells += 1;
            }
        }
    }
    let territory = own_cells - opp_cells;

    let mobility_score = 0i32;

    let mut own_connectivity = 0i32;
    let mut opp_connectivity = 0i32;
    for r in 0..ROWS {
        for c in 0..COLS {
            let owner = owners[r][c];
            if c + 1 < COLS && owners[r][c + 1] == owner {
                if owner == player {
                    own_connectivity += 1;
                } else if owner == opp {
                    opp_connectivity += 1;
                }
            }
            if r + 1 < ROWS && owners[r + 1][c] == owner {
                if owner == player {
                    own_connectivity += 1;
                } else if owner == opp {
                    opp_connectivity += 1;
                }
            }
        }
    }
    let connectivity = own_connectivity - opp_connectivity;

    let corners = [
        (0, 0),
        (0, COLS - 1),
        (ROWS - 1, 0),
        (ROWS - 1, COLS - 1),
    ]
    .into_iter()
    .fold(0i32, |acc, (r, c)| {
        if owners[r][c] == player {
            acc + 1
        } else if owners[r][c] == opp {
            acc - 1
        } else {
            acc
        }
    });

    let mut edges = 0i32;
    for c in 0..COLS {
        if owners[0][c] == player {
            edges += 1;
        } else if owners[0][c] == opp {
            edges -= 1;
        }
        if owners[ROWS - 1][c] == player {
            edges += 1;
        } else if owners[ROWS - 1][c] == opp {
            edges -= 1;
        }
    }
    for r in 1..ROWS - 1 {
        if owners[r][0] == player {
            edges += 1;
        } else if owners[r][0] == opp {
            edges -= 1;
        }
        if owners[r][COLS - 1] == player {
            edges += 1;
        } else if owners[r][COLS - 1] == opp {
            edges -= 1;
        }
    }

    let mut recapture_swing = 0i32;
    let mut vulnerability = 0i32;
    for r in 0..ROWS {
        for c in 0..COLS {
            let adjacent_to_live = (r > 0 && values[r - 1][c] > 0)
                || (r + 1 < ROWS && values[r + 1][c] > 0)
                || (c > 0 && values[r][c - 1] > 0)
                || (c + 1 < COLS && values[r][c + 1] > 0);
            if !adjacent_to_live {
                continue;
            }
            if owners[r][c] == opp {
                recapture_swing += 1;
            } else if owners[r][c] == player {
                vulnerability += 1;
            }
        }
    }

    territory * 74
        + mobility_score * (-42)
        + connectivity * 93
        + corners * 80
        + edges * 40
        + recapture_swing * 101
        - vulnerability * 83
}

#[test]
fn evaluate_matches_slow_reference_after_incremental_updates() {
    let mut board = common::medium_board();
    board.player = FIRST_PLAYER;

    let moves = [
        Move { r1: 0, c1: 0, r2: 0, c2: 9, priority: 0 },
        Move { r1: 2, c1: 2, r2: 3, c2: 4, priority: 0 },
    ];

    assert_eq!(evaluate(&board, true), slow_evaluate(&board));
    for mv in moves {
        board.apply_move(&mv);
        assert_eq!(evaluate(&board, true), slow_evaluate(&board));
    }
}

#[test]
fn score_move_matches_manual_apply_and_eval() {
    let board = common::sparse_board();
    let mv = Move { r1: 0, c1: 0, r2: 0, c2: 9, priority: 0 };

    let mut expected_board = board.clone();
    expected_board.apply_move(&mv);

    assert_eq!(score_move(&board, &mv, true), evaluate(&expected_board, true));
}
