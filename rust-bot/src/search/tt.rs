use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::cell::UnsafeCell;
use std::sync::OnceLock;

use crate::types::*;

// ====== Transposition Table ======

pub const TT_SIZE: usize = 1 << 24;

#[derive(Clone, Copy, Default)]
pub struct CompactTTEntry {
    pub key_sig: u32,
    pub depth: i16,
    pub flag: u8,
    pub age: u8,
    pub value: i32,
    pub packed_move: u32,
}

#[derive(Clone, Copy, Default)]
pub struct TTBucket {
    pub slot0: CompactTTEntry,
    pub slot1: CompactTTEntry,
}

pub mod tt_flag {
    pub const EMPTY: u8 = 0;
    pub const EXACT: u8 = 1;
    pub const LOWER_BOUND: u8 = 2;
    pub const UPPER_BOUND: u8 = 3;
}

pub static NODES_SEARCHED: AtomicU64 = AtomicU64::new(0);

struct SingleThreadTT(UnsafeCell<Vec<TTBucket>>);

unsafe impl Sync for SingleThreadTT {}

static TT: OnceLock<SingleThreadTT> = OnceLock::new();

static TT_AGE: AtomicU8 = AtomicU8::new(1);

pub fn ensure_tt_ready() {
    let tt = TT.get_or_init(|| SingleThreadTT(UnsafeCell::new(Vec::new())));
    let tt = unsafe { &mut *tt.0.get() };
    if tt.len() != TT_SIZE {
        tt.resize(TT_SIZE, TTBucket::default());
    }
}

pub fn pack_move(m: &Move) -> u32 {
    if m.is_pass() {
        return 0xFFFFFFFF;
    }
    ((m.r1 as u32 & 0xF) << 15)
        | ((m.c1 as u32 & 0x1F) << 10)
        | ((m.r2 as u32 & 0xF) << 6)
        | ((m.c2 as u32 & 0x1F) << 1)
}

pub fn unpack_move(packed: u32) -> Move {
    if packed == 0xFFFFFFFF {
        return PASS_MOVE;
    }
    Move {
        r1: ((packed >> 15) & 0xF) as i32,
        c1: ((packed >> 10) & 0x1F) as i32,
        r2: ((packed >> 6) & 0xF) as i32,
        c2: ((packed >> 1) & 0x1F) as i32,
        priority: 0,
    }
}

pub fn tt_store(key: u64, depth: i32, value: i32, flag: u8, best_move: &Move) {
    let age = TT_AGE.load(Ordering::Relaxed);
    let ksig = (key >> 32) as u32;
    let idx = (key as usize) & (TT_SIZE - 1);
    let pm = pack_move(best_move);
    let d16 = depth as i16;

    let tt = TT.get_or_init(|| SingleThreadTT(UnsafeCell::new(Vec::new())));
    let tt = unsafe { &mut *tt.0.get() };
    let b = &mut tt[idx];

    b.slot0 = CompactTTEntry { key_sig: ksig, depth: d16, flag, age, value, packed_move: pm };

    let same_key = b.slot1.key_sig == ksig;
    let stale = b.slot1.age != age;
    let deeper = depth >= b.slot1.depth as i32;
    let replace = if same_key { deeper } else { stale || deeper };
    if replace {
        b.slot1 = CompactTTEntry { key_sig: ksig, depth: d16, flag, age, value, packed_move: pm };
    }
}

pub fn tt_probe(
    key: u64,
    depth: i32,
    alpha: i32,
    beta: i32,
    value_out: &mut i32,
    best_move_out: &mut Move,
) -> bool {
    let ksig = (key >> 32) as u32;
    let idx = (key as usize) & (TT_SIZE - 1);
    let tt = TT.get_or_init(|| SingleThreadTT(UnsafeCell::new(Vec::new())));
    let tt = unsafe { &*tt.0.get() };
    let b = &tt[idx];

    let mut best: Option<CompactTTEntry> = None;
    if b.slot0.key_sig == ksig && b.slot0.depth >= depth as i16 {
        best = Some(b.slot0);
    }
    if b.slot1.key_sig == ksig && b.slot1.depth >= depth as i16 {
        if best.is_none() || b.slot1.depth > best.unwrap().depth {
            best = Some(b.slot1);
        }
    }

    let entry = match best {
        None => return false,
        Some(e) => e,
    };

    *best_move_out = unpack_move(entry.packed_move);
    let stored = entry.value;

    if entry.flag == tt_flag::EXACT { *value_out = stored; return true; }
    if entry.flag == tt_flag::LOWER_BOUND && stored >= beta { *value_out = stored; return true; }
    if entry.flag == tt_flag::UPPER_BOUND && stored <= alpha { *value_out = stored; return true; }
    false
}

pub fn advance_tt_age() {
    TT_AGE.fetch_add(1, Ordering::Relaxed);
}
