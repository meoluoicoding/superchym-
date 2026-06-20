// opponent_db.rs — from opponent_db.hpp + opponent_db.cpp

use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicI32};
use crate::types::*;

// ====== Binary Format Constants ======

const DB_MAGIC: u32 = 0x44415441;      // "DATA" LE
const DB_FOOTER_MAGIC: u32 = 0x544144; // "DAT" reversed
const DB_VERSION: u32 = 3;
const DB_SECTION_FINGERPRINTS: u32 = 1;
const DB_SECTION_CENTROIDS: u32 = 2;
const DB_SECTION_PRIOR_CONFIGS: u32 = 3;
const DB_SECTION_METADATA: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct DBHeader {
    magic: u32,
    version: u32,
    section_count: u32,
    crc32: u32,
    build_id: u64,
    baseline_id: u64,
    reserved: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DBSection {
    stype: u32,
    count: u32,
    data_size: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DBFooter {
    magic: u32,
    total_size: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct KnownFingerprint {
    pub id: u32,
    pub style: u32,    // KnownStyle as u32
    pub side_mask: u8, // bit0=FIRST, bit1=SECOND
    pub min_moves: u8,
    pub confidence_threshold: u8,
    pub margin_to_second: u8,
    pub mean: FeatureVector8,
    pub var: FeatureVector8,
    pub prior_config_id: u32,
}

#[derive(Clone, Default)]
pub struct LoadedPriorConfig {
    pub id: u32,
    pub config: MovePriorConfig,
}

const MAX_FINGERPRINTS: usize = 16;
const MAX_PRIOR_CONFIGS: usize = 16;

pub struct OpponentDB {
    loaded: bool,
    data: Vec<u8>,
    header: DBHeader,
    fingerprints: [KnownFingerprint; MAX_FINGERPRINTS],
    fingerprint_count: usize,
    prior_configs: [Option<LoadedPriorConfig>; MAX_PRIOR_CONFIGS],
    prior_config_count: usize,
}

impl OpponentDB {
    pub fn new() -> Self {
        OpponentDB {
            loaded: false,
            data: Vec::new(),
            header: DBHeader::default(),
            fingerprints: [KnownFingerprint::default(); MAX_FINGERPRINTS],
            fingerprint_count: 0,
            prior_configs: Default::default(),
            prior_config_count: 0,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        let header_size = std::mem::size_of::<DBHeader>();
        let footer_size = std::mem::size_of::<DBFooter>();

        if data.len() < header_size + footer_size {
            eprintln!("OpponentDB: data too small ({} bytes)", data.len());
            return false;
        }

        self.data = data.to_vec();

        // Parse header
        self.header = unsafe {
            std::ptr::read_unaligned(self.data.as_ptr() as *const DBHeader)
        };

        if self.header.magic != DB_MAGIC || self.header.version != DB_VERSION {
            eprintln!(
                "OpponentDB: bad magic/version (magic=0x{:X}, ver={})",
                self.header.magic, self.header.version
            );
            self.loaded = false;
            return false;
        }

        if !self.validate_crc32() {
            eprintln!("OpponentDB: CRC mismatch");
            self.loaded = false;
            return false;
        }

        if !self.parse_sections() {
            self.loaded = false;
            return false;
        }

        self.loaded = true;
        true
    }

    pub fn load_from_file(&mut self, path: &str) -> bool {
        match std::fs::read(path) {
            Ok(buf) => self.load(&buf),
            Err(_) => {
                eprintln!("OpponentDB: cannot open {}", path);
                false
            }
        }
    }

    fn validate_crc32(&self) -> bool {
        let header_size = std::mem::size_of::<DBHeader>();
        let footer_size = std::mem::size_of::<DBFooter>();
        if header_size >= self.data.len() - footer_size {
            return false;
        }
        let crc_len = self.data.len() - header_size - footer_size;
        let expected = self.header.crc32;
        let actual = crc32_simple(&self.data[header_size..header_size + crc_len]);
        expected == actual
    }

    fn parse_sections(&mut self) -> bool {
        self.fingerprint_count = 0;
        self.prior_config_count = 0;

        let header_size = std::mem::size_of::<DBHeader>();
        let footer_size = std::mem::size_of::<DBFooter>();
        let section_size = std::mem::size_of::<DBSection>();
        let mut offset = header_size;

        for _ in 0..self.header.section_count {
            if offset + section_size > self.data.len() - footer_size {
                return false;
            }
            let sec: DBSection = unsafe {
                std::ptr::read_unaligned(self.data[offset..].as_ptr() as *const DBSection)
            };
            offset += section_size;

            if offset + sec.data_size as usize > self.data.len() - footer_size {
                return false;
            }

            let sec_data = &self.data[offset..offset + sec.data_size as usize];

            match sec.stype {
                DB_SECTION_FINGERPRINTS => {
                    let fp_size = std::mem::size_of::<KnownFingerprint>();
                    let count = (sec.data_size as usize / fp_size)
                        .min(MAX_FINGERPRINTS);
                    for i in 0..count {
                        self.fingerprints[self.fingerprint_count] = unsafe {
                            std::ptr::read_unaligned(
                                sec_data[i * fp_size..].as_ptr() as *const KnownFingerprint
                            )
                        };
                        self.fingerprint_count += 1;
                    }
                }
                DB_SECTION_PRIOR_CONFIGS => {
                    let entry_size = 4 + std::mem::size_of::<MovePriorConfigRaw>();
                    let count = (sec.data_size as usize / entry_size)
                        .min(MAX_PRIOR_CONFIGS);
                    for i in 0..count {
                        let base = i * entry_size;
                        let id: u32 = unsafe {
                            std::ptr::read_unaligned(sec_data[base..].as_ptr() as *const u32)
                        };
                        let raw: MovePriorConfigRaw = unsafe {
                            std::ptr::read_unaligned(
                                sec_data[base + 4..].as_ptr() as *const MovePriorConfigRaw
                            )
                        };
                        let cfg = MovePriorConfig {
                            shape_boost: raw.shape_boost,
                            medium_rect_boost: raw.medium_rect_boost,
                            barrier_boost: raw.barrier_boost,
                            connection_boost: raw.connection_boost,
                            dead_cell_risk_penalty: raw.dead_cell_risk_penalty,
                            side_boost_first: raw.side_boost_first,
                            side_boost_second: raw.side_boost_second,
                            max_total_adjustment: raw.max_total_adjustment,
                            confidence_min: raw.confidence_min,
                        };
                        let idx = self.prior_config_count;
                        self.prior_configs[idx] = Some(LoadedPriorConfig { id, config: cfg });
                        self.prior_config_count += 1;
                    }
                }
                DB_SECTION_CENTROIDS | DB_SECTION_METADATA => {
                    // Reserved
                }
                _ => {}
            }
            offset += sec.data_size as usize;
        }
        true
    }

    pub fn fingerprints(&self) -> &[KnownFingerprint] {
        &self.fingerprints[..self.fingerprint_count]
    }

    pub fn get_prior_config(&self, id: u32) -> Option<&MovePriorConfig> {
        for i in 0..self.prior_config_count {
            if let Some(ref lpc) = self.prior_configs[i] {
                if lpc.id == id {
                    return Some(&lpc.config);
                }
            }
        }
        None
    }

    pub fn default_prior_config(&self) -> Option<&MovePriorConfig> {
        if self.prior_config_count > 0 {
            if let Some(ref lpc) = self.prior_configs[0] {
                return Some(&lpc.config);
            }
        }
        None
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }
}

// Raw C-layout struct matching the binary file format for MovePriorConfig
#[repr(C)]
#[derive(Clone, Copy)]
struct MovePriorConfigRaw {
    shape_boost: [i16; 8],
    medium_rect_boost: i16,
    barrier_boost: i16,
    connection_boost: i16,
    dead_cell_risk_penalty: i16,
    side_boost_first: i16,
    side_boost_second: i16,
    max_total_adjustment: u16,
    confidence_min: u8,
    _pad: u8,
}

// Global instances
use std::sync::{Mutex, OnceLock};

static G_OPPONENT_DB_INNER: OnceLock<Mutex<OpponentDB>> = OnceLock::new();

pub fn g_opponent_db() -> &'static Mutex<OpponentDB> {
    G_OPPONENT_DB_INNER.get_or_init(|| Mutex::new(OpponentDB::new()))
}

// Active prior config pointer (nullptr = no overhead when no data.bin)
pub static G_ACTIVE_PRIOR_CONFIG: AtomicPtr<MovePriorConfig> =
    AtomicPtr::new(std::ptr::null_mut());

// Current matched opponent style (for search engine to read)
pub static G_MATCHED_STYLE: AtomicU32 = AtomicU32::new(0); // KnownStyle as u32
pub static G_MATCH_CONFIDENCE: AtomicI32 = AtomicI32::new(0); // confidence * 100

// CRC32 (nibble-by-nibble)
fn crc32_simple(data: &[u8]) -> u32 {
    const TABLE: [u32; 16] = [
        0x00000000, 0x1DB71064, 0x3B6E20C8, 0x26D930AC,
        0x76DC4190, 0x6B6B51F4, 0x4DB26158, 0x5005713C,
        0xEDB88320, 0xF00F9344, 0xD6D6A3E8, 0xCB61B38C,
        0x9B64C2B0, 0x86D392D4, 0xA00AE278, 0xBDBDF21C,
    ];
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in data {
        crc ^= byte as u32;
        crc = TABLE[(crc & 0xF) as usize] ^ (crc >> 4);
        crc = TABLE[(crc & 0xF) as usize] ^ (crc >> 4);
    }
    !crc
}

// ====== OpponentFingerprint ======

#[derive(Clone, Default)]
pub struct OpponentFingerprint {
    pub move_count: i32,
    pub shape_counts: [i32; 8],
    pub total_area: i32,
    pub medium_count: i32,
    pub large_count: i32,
    pub tall_count: i32,
    pub wide_count: i32,
    pub region_counts: [i32; 4],
    pub steal_seen: i32,
    pub pass_seen: i32,
    pub first_pass_ply: i32,
    pub barrier_freq: i32,
    pub side_ply: i32,
    pub we_are_first: bool,
}

impl OpponentFingerprint {
    pub fn to_feature_vector(&self) -> FeatureVector8 {
        let mut fv = FeatureVector8::default();
        let n = self.move_count;
        if n == 0 {
            return fv;
        }
        let total = self.total_area;
        let medium = self.medium_count;
        let large = self.large_count;
        let tall = self.tall_count;
        let wide = self.wide_count;
        let steal = self.steal_seen;
        let pass = self.pass_seen;
        let barrier = self.barrier_freq;

        fv.dim[0] = ((total * 128) / n) as i16;
        fv.dim[1] = ((medium * 128) / n) as i16;
        fv.dim[2] = ((large * 128) / n) as i16;
        fv.dim[3] = ((tall * 128) / n) as i16;
        fv.dim[4] = ((wide * 128) / n) as i16;
        fv.dim[5] = ((steal * 128) / n) as i16;
        fv.dim[6] = ((pass * 128) / n) as i16;
        fv.dim[7] = ((barrier * 128) / n) as i16;
        fv
    }

    pub fn match_fingerprint(
        &self,
        fps: &[KnownFingerprint],
        out_confidence: &mut f32,
        out_margin: &mut f32,
    ) -> KnownStyle {
        *out_confidence = 0.0;
        *out_margin = 0.0;
        if fps.is_empty() || self.move_count == 0 {
            return KnownStyle::Unknown;
        }

        let fv = self.to_feature_vector();
        let mut best_idx: i32 = -1;
        let mut best_dist: f32 = 1e30;
        let mut second_dist: f32 = 1e30;

        // Per-dimension importance weights (steal/barrier/pass more discriminative)
        const DIM_WEIGHTS: [f32; 8] = [0.8, 1.0, 1.2, 1.0, 1.0, 1.5, 1.3, 1.4];

        for (i, fp) in fps.iter().enumerate() {
            let first_ok = (fp.side_mask & 1) != 0 && self.we_are_first;
            let second_ok = (fp.side_mask & 2) != 0 && !self.we_are_first;
            if !first_ok && !second_ok {
                continue;
            }
            if self.move_count < fp.min_moves as i32 {
                continue;
            }

            let mut dist = 0.0f32;
            for d in 0..8 {
                let diff = (fv.dim[d] - fp.mean.dim[d]) as f32;
                let inv_var = fp.var.dim[d] as f32;
                let scaled = if inv_var > 0.0 {
                    diff / (inv_var / 128.0)
                } else {
                    diff
                };
                dist += DIM_WEIGHTS[d] * scaled * scaled;
            }

            if dist < best_dist {
                second_dist = best_dist;
                best_dist = dist;
                best_idx = i as i32;
            } else if dist < second_dist {
                second_dist = dist;
            }
        }

        if best_idx < 0 {
            return KnownStyle::Unknown;
        }

        // Confidence: ratio-based
        if second_dist < 1e29 && second_dist > 0.01 {
            *out_confidence = 1.0 - (best_dist / second_dist);
        } else {
            // Single match: high confidence (no ambiguity with alternatives)
            *out_confidence = 0.95;
        }
        *out_margin = second_dist - best_dist;

        // Adaptive threshold: lower when we have more observations
        let best_fp = &fps[best_idx as usize];
        let base_threshold = best_fp.confidence_threshold as f32;
        let adaptive_threshold = if self.move_count >= 15 {
            base_threshold * 0.75
        } else if self.move_count >= 10 {
            base_threshold * 0.85
        } else {
            base_threshold
        };

        if (*out_confidence * 100.0) < adaptive_threshold {
            return KnownStyle::Unknown;
        }

        // Map u32 id to KnownStyle
        match best_fp.style {
            1 => KnownStyle::CordycepsAttack,
            2 => KnownStyle::CordycepsDefense,
            3 => KnownStyle::CordycepsBalanced,
            4 => KnownStyle::RustOld,
            5 => KnownStyle::RustUpdate,
            6 => KnownStyle::Main4Cordyceps,
            7 => KnownStyle::Main5Cordyceps,
            _ => KnownStyle::Unknown,
        }
    }
}
