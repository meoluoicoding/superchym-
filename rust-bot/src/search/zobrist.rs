use crate::board::Board;
use crate::types::*;

// ====== Zobrist Hashing ======
// Single sequential seed matching C++ exactly

fn xorshift64(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

struct ZobristTables {
    value: [[[u64; 10]; COLS]; ROWS],
    owner: [[[u64; 3]; COLS]; ROWS],
    player: u64,
    passes: [u64; 3],
}

static ZOBRIST: std::sync::OnceLock<ZobristTables> = std::sync::OnceLock::new();

fn get_zobrist() -> &'static ZobristTables {
    ZOBRIST.get_or_init(|| {
        let mut seed = 1234567890123456789u64;
        let mut value = [[[0u64; 10]; COLS]; ROWS];
        for r in 0..ROWS {
            for c in 0..COLS {
                for v in 0..10 {
                    value[r][c][v] = xorshift64(&mut seed);
                }
            }
        }
        let mut owner = [[[0u64; 3]; COLS]; ROWS];
        for r in 0..ROWS {
            for c in 0..COLS {
                for o in 0..3 {
                    owner[r][c][o] = xorshift64(&mut seed);
                }
            }
        }
        let player = xorshift64(&mut seed);
        let mut passes = [0u64; 3];
        for i in 0..3 {
            passes[i] = xorshift64(&mut seed);
        }
        ZobristTables { value, owner, player, passes }
    })
}

pub(crate) fn cell_hash(r: usize, c: usize, value: i8, owner: i8) -> u64 {
    let z = get_zobrist();
    let mut h = 0u64;
    let v = value as usize;
    if v > 0 && v <= 9 {
        h ^= z.value[r][c][v];
    }
    let oi = if owner == FIRST_PLAYER { 1usize }
             else if owner == SECOND_PLAYER { 2usize }
             else { 0usize };
    h ^ z.owner[r][c][oi]
}

pub(crate) fn player_hash(player: i8) -> u64 {
    if player == FIRST_PLAYER {
        get_zobrist().player
    } else {
        0
    }
}

pub(crate) fn passes_hash(consecutive_passes: i32) -> u64 {
    if (0..=2).contains(&consecutive_passes) {
        get_zobrist().passes[consecutive_passes as usize]
    } else {
        0
    }
}

pub fn hash_board(board: &Board) -> u64 {
    let z = get_zobrist();
    let mut h = 0u64;
    for r in 0..ROWS {
        for c in 0..COLS {
            let v = board.values[r][c] as usize;
            if v > 0 && v <= 9 {
                h ^= z.value[r][c][v];
            }
            let o = board.owners[r][c];
            let oi = if o == FIRST_PLAYER { 1usize }
                     else if o == SECOND_PLAYER { 2usize }
                     else { 0usize };
            h ^= z.owner[r][c][oi];
        }
    }
    if board.player == FIRST_PLAYER {
        h ^= z.player;
    }
    let cp = board.consecutive_passes as usize;
    if cp <= 2 {
        h ^= z.passes[cp];
    }
    h
}
