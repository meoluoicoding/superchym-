use crate::board::Board;
use crate::types::*;

// ====== Zobrist Hashing ======
// MT19937-64 for better hash quality

struct MT19937 {
    mt: [u64; 312],
    mti: usize,
}

impl MT19937 {
    fn new(seed: u64) -> Self {
        let mut mt = [0u64; 312];
        mt[0] = seed;
        for i in 1..312 {
            mt[i] = 6364136223846793005u64.wrapping_mul(mt[i - 1] ^ (mt[i - 1] >> 62)) + i as u64;
        }
        MT19937 { mt, mti: 312 }
    }

    fn next_u64(&mut self) -> u64 {
        if self.mti >= 312 {
            self.generate();
        }
        let x = self.mt[self.mti];
        self.mti += 1;
        // Tempering
        let x = x ^ (x >> 29) & 0x5555555555555555;
        let x = x ^ (x << 17) & 0x71D67FFFEDA60000;
        let x = x ^ (x << 37) & 0xFFF7EEE000000000;
        x ^ (x >> 43)
    }

    fn generate(&mut self) {
        for i in 0..156 {
            let y = (self.mt[i] & 0xFFFFFFFF80000000) | (self.mt[i + 1] & 0x7FFFFFFF);
            self.mt[i] = self.mt[i + 156] ^ (y >> 1) ^ ((y & 1) * 0xB5026F5AA96619E9);
        }
        for i in 156..311 {
            let y = (self.mt[i] & 0xFFFFFFFF80000000) | (self.mt[i + 1] & 0x7FFFFFFF);
            self.mt[i] = self.mt[i - 156] ^ (y >> 1) ^ ((y & 1) * 0xB5026F5AA96619E9);
        }
        let y = (self.mt[311] & 0xFFFFFFFF80000000) | (self.mt[0] & 0x7FFFFFFF);
        self.mt[311] = self.mt[155] ^ (y >> 1) ^ ((y & 1) * 0xB5026F5AA96619E9);
        self.mti = 0;
    }
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
        let mut rng = MT19937::new(1234567890123456789u64);
        let mut value = [[[0u64; 10]; COLS]; ROWS];
        for r in 0..ROWS {
            for c in 0..COLS {
                for v in 0..10 {
                    value[r][c][v] = rng.next_u64();
                }
            }
        }
        let mut owner = [[[0u64; 3]; COLS]; ROWS];
        for r in 0..ROWS {
            for c in 0..COLS {
                for o in 0..3 {
                    owner[r][c][o] = rng.next_u64();
                }
            }
        }
        let player = rng.next_u64();
        let mut passes = [0u64; 3];
        for i in 0..3 {
            passes[i] = rng.next_u64();
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
