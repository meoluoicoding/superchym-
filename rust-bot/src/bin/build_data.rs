use std::fs;
use std::path::Path;

use mushroom_bot::opponent_db::OpponentDB;
use mushroom_bot::types::{FeatureVector8, MovePriorConfig};

const DB_MAGIC: u32 = 0x44415441;
const DB_FOOTER_MAGIC: u32 = 0x544144;
const DB_VERSION: u32 = 3;
const DB_SECTION_FINGERPRINTS: u32 = 1;
const DB_SECTION_PRIOR_CONFIGS: u32 = 3;
const DB_SECTION_METADATA: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
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
struct KnownFingerprintRaw {
    id: u32,
    style: u32,
    side_mask: u8,
    min_moves: u8,
    confidence_threshold: u8,
    margin_to_second: u8,
    mean: FeatureVector8,
    var: FeatureVector8,
    prior_config_id: u32,
}

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
    pad: u8,
}

#[derive(Clone)]
struct PriorConfigEntry {
    id: u32,
    config: MovePriorConfig,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut output = "data.bin".to_string();
    let mut build_id = 1u64;
    let mut baseline_id = 0u64;
    let mut prior_file = String::new();
    let mut fingerprint_file = String::new();
    let mut metadata_file = String::new();
    let mut include_default_prior = true;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--output" => {
                i += 1;
                output = args.get(i).cloned().unwrap_or_else(|| "data.bin".to_string());
            }
            "--build-id" => {
                i += 1;
                build_id = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(build_id);
            }
            "--baseline-id" => {
                i += 1;
                baseline_id = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(baseline_id);
            }
            "--prior-file" => {
                i += 1;
                prior_file = args.get(i).cloned().unwrap_or_default();
            }
            "--fingerprint-file" => {
                i += 1;
                fingerprint_file = args.get(i).cloned().unwrap_or_default();
            }
            "--metadata-file" => {
                i += 1;
                metadata_file = args.get(i).cloned().unwrap_or_default();
            }
            "--no-default-prior" => {
                include_default_prior = false;
            }
            _ => {}
        }
        i += 1;
    }

    let mut priors = if prior_file.is_empty() {
        Vec::new()
    } else {
        load_prior_configs(&prior_file)
    };
    if include_default_prior && priors.is_empty() {
        priors.extend(default_prior_configs());
    }

    let fingerprints = if fingerprint_file.is_empty() {
        Vec::new()
    } else {
        load_fingerprints(&fingerprint_file)
    };
    let metadata = if metadata_file.is_empty() {
        Vec::new()
    } else {
        fs::read(&metadata_file).unwrap_or_else(|e| {
            eprintln!("Warning: cannot read metadata file {}: {}", metadata_file, e);
            Vec::new()
        })
    };

    let bytes = build_database_bytes(build_id, baseline_id, &priors, &fingerprints, &metadata);

    if let Some(parent) = Path::new(&output).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).expect("create output directory");
        }
    }
    fs::write(&output, &bytes).expect("write data.bin");

    let mut db = OpponentDB::new();
    if !db.load(&bytes) {
        panic!("generated data.bin did not validate through OpponentDB loader");
    }

    eprintln!(
        "Written {}: {} bytes | priors={} fingerprints={} metadata={} bytes",
        output,
        bytes.len(),
        priors.len(),
        fingerprints.len(),
        metadata.len()
    );
}

fn build_database_bytes(
    build_id: u64,
    baseline_id: u64,
    priors: &[PriorConfigEntry],
    fingerprints: &[KnownFingerprintRaw],
    metadata: &[u8],
) -> Vec<u8> {
    let mut sections: Vec<(DBSection, Vec<u8>)> = Vec::new();

    if !fingerprints.is_empty() {
        let mut raw = Vec::with_capacity(fingerprints.len() * std::mem::size_of::<KnownFingerprintRaw>());
        for fp in fingerprints {
            push_known_fingerprint(&mut raw, fp);
        }
        sections.push((
            DBSection {
                stype: DB_SECTION_FINGERPRINTS,
                count: fingerprints.len() as u32,
                data_size: raw.len() as u32,
                reserved: 0,
            },
            raw,
        ));
    }

    if !priors.is_empty() {
        let mut raw = Vec::with_capacity(priors.len() * (4 + std::mem::size_of::<MovePriorConfigRaw>()));
        for prior in priors {
            raw.extend_from_slice(&prior.id.to_le_bytes());
            push_prior_config(&mut raw, &prior.config);
        }
        sections.push((
            DBSection {
                stype: DB_SECTION_PRIOR_CONFIGS,
                count: priors.len() as u32,
                data_size: raw.len() as u32,
                reserved: 0,
            },
            raw,
        ));
    }

    if !metadata.is_empty() {
        sections.push((
            DBSection {
                stype: DB_SECTION_METADATA,
                count: 1,
                data_size: metadata.len() as u32,
                reserved: 0,
            },
            metadata.to_vec(),
        ));
    }

    let header = DBHeader {
        magic: DB_MAGIC,
        version: DB_VERSION,
        section_count: sections.len() as u32,
        crc32: 0,
        build_id,
        baseline_id,
        reserved: [0; 2],
    };

    let mut bytes = Vec::new();
    push_header(&mut bytes, &header);
    for (section, payload) in &sections {
        push_section(&mut bytes, section);
        bytes.extend_from_slice(payload);
    }

    let crc = crc32_simple(&bytes[std::mem::size_of::<DBHeader>()..]);
    let crc_offset = 12;
    bytes[crc_offset..crc_offset + 4].copy_from_slice(&crc.to_le_bytes());

    let footer = DBFooter {
        magic: DB_FOOTER_MAGIC,
        total_size: (bytes.len() + std::mem::size_of::<DBFooter>()) as u32,
    };
    push_footer(&mut bytes, &footer);
    bytes
}

fn default_prior_configs() -> Vec<PriorConfigEntry> {
    vec![
        PriorConfigEntry {
            id: 1,
            config: MovePriorConfig {
                shape_boost: [160, 170, 110, 130, 220, 50, 90, -30],
                medium_rect_boost: 120,
                barrier_boost: 160,
                connection_boost: 90,
                dead_cell_risk_penalty: 120,
                side_boost_first: 60,
                side_boost_second: 0,
                max_total_adjustment: 1500,
                confidence_min: 55,
            },
        },
        PriorConfigEntry {
            id: 2,
            config: MovePriorConfig {
                shape_boost: [220, 230, 120, 180, 200, 30, 100, -120],
                medium_rect_boost: 40,
                barrier_boost: 320,
                connection_boost: 180,
                dead_cell_risk_penalty: 320,
                side_boost_first: 0,
                side_boost_second: 220,
                max_total_adjustment: 2200,
                confidence_min: 50,
            },
        },
    ]
}

fn load_prior_configs(path: &str) -> Vec<PriorConfigEntry> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("cannot read prior config file {}: {}", path, e);
    });

    let mut entries = Vec::new();
    let mut current_id: Option<u32> = None;
    let mut current = MovePriorConfig::new_default();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "---" {
            if let Some(id) = current_id.take() {
                entries.push(PriorConfigEntry { id, config: current.clone() });
                current = MovePriorConfig::new_default();
            }
            continue;
        }

        let (key, value) = split_key_value(line);
        match key {
            "id" => current_id = Some(value.parse().expect("prior id must be u32")),
            "shape_boost" => current.shape_boost = parse_i16_array::<8>(value),
            "medium_rect_boost" => current.medium_rect_boost = value.parse().expect("medium_rect_boost must be i16"),
            "barrier_boost" => current.barrier_boost = value.parse().expect("barrier_boost must be i16"),
            "connection_boost" => current.connection_boost = value.parse().expect("connection_boost must be i16"),
            "dead_cell_risk_penalty" => current.dead_cell_risk_penalty = value.parse().expect("dead_cell_risk_penalty must be i16"),
            "side_boost_first" => current.side_boost_first = value.parse().expect("side_boost_first must be i16"),
            "side_boost_second" => current.side_boost_second = value.parse().expect("side_boost_second must be i16"),
            "max_total_adjustment" => current.max_total_adjustment = value.parse().expect("max_total_adjustment must be u16"),
            "confidence_min" => current.confidence_min = value.parse().expect("confidence_min must be u8"),
            _ => panic!("unknown prior config key: {}", key),
        }
    }

    if let Some(id) = current_id {
        entries.push(PriorConfigEntry { id, config: current });
    }
    entries
}

fn load_fingerprints(path: &str) -> Vec<KnownFingerprintRaw> {
    let text = fs::read_to_string(path).unwrap_or_else(|e| {
        panic!("cannot read fingerprint file {}: {}", path, e);
    });

    let mut out = Vec::new();
    let mut current = KnownFingerprintRaw::default();
    let mut has_entry = false;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "---" {
            if has_entry {
                out.push(current);
                current = KnownFingerprintRaw::default();
                has_entry = false;
            }
            continue;
        }

        has_entry = true;
        let (key, value) = split_key_value(line);
        match key {
            "id" => current.id = value.parse().expect("fingerprint id must be u32"),
            "style" => current.style = value.parse().expect("style must be u32"),
            "side_mask" => current.side_mask = value.parse().expect("side_mask must be u8"),
            "min_moves" => current.min_moves = value.parse().expect("min_moves must be u8"),
            "confidence_threshold" => current.confidence_threshold = value.parse().expect("confidence_threshold must be u8"),
            "margin_to_second" => current.margin_to_second = value.parse().expect("margin_to_second must be u8"),
            "mean" => current.mean = FeatureVector8 { dim: parse_i16_array::<8>(value) },
            "var" => current.var = FeatureVector8 { dim: parse_i16_array::<8>(value) },
            "prior_config_id" => current.prior_config_id = value.parse().expect("prior_config_id must be u32"),
            _ => panic!("unknown fingerprint key: {}", key),
        }
    }

    if has_entry {
        out.push(current);
    }
    out
}

fn split_key_value(line: &str) -> (&str, &str) {
    let Some((key, value)) = line.split_once('=') else {
        panic!("expected key=value line, got: {}", line);
    };
    (key.trim(), value.trim())
}

fn parse_i16_array<const N: usize>(value: &str) -> [i16; N] {
    let parsed: Vec<i16> = value
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i16>().expect("array element must be i16"))
        .collect();
    assert_eq!(parsed.len(), N, "expected {} values, got {}", N, parsed.len());

    let mut out = [0i16; N];
    out.copy_from_slice(&parsed);
    out
}

fn push_header(buf: &mut Vec<u8>, header: &DBHeader) {
    buf.extend_from_slice(&header.magic.to_le_bytes());
    buf.extend_from_slice(&header.version.to_le_bytes());
    buf.extend_from_slice(&header.section_count.to_le_bytes());
    buf.extend_from_slice(&header.crc32.to_le_bytes());
    buf.extend_from_slice(&header.build_id.to_le_bytes());
    buf.extend_from_slice(&header.baseline_id.to_le_bytes());
    for value in header.reserved {
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

fn push_section(buf: &mut Vec<u8>, section: &DBSection) {
    buf.extend_from_slice(&section.stype.to_le_bytes());
    buf.extend_from_slice(&section.count.to_le_bytes());
    buf.extend_from_slice(&section.data_size.to_le_bytes());
    buf.extend_from_slice(&section.reserved.to_le_bytes());
}

fn push_footer(buf: &mut Vec<u8>, footer: &DBFooter) {
    buf.extend_from_slice(&footer.magic.to_le_bytes());
    buf.extend_from_slice(&footer.total_size.to_le_bytes());
}

fn push_known_fingerprint(buf: &mut Vec<u8>, fp: &KnownFingerprintRaw) {
    buf.extend_from_slice(&fp.id.to_le_bytes());
    buf.extend_from_slice(&fp.style.to_le_bytes());
    buf.push(fp.side_mask);
    buf.push(fp.min_moves);
    buf.push(fp.confidence_threshold);
    buf.push(fp.margin_to_second);
    push_feature_vector(buf, &fp.mean);
    push_feature_vector(buf, &fp.var);
    buf.extend_from_slice(&fp.prior_config_id.to_le_bytes());
}

fn push_prior_config(buf: &mut Vec<u8>, cfg: &MovePriorConfig) {
    let raw = MovePriorConfigRaw {
        shape_boost: cfg.shape_boost,
        medium_rect_boost: cfg.medium_rect_boost,
        barrier_boost: cfg.barrier_boost,
        connection_boost: cfg.connection_boost,
        dead_cell_risk_penalty: cfg.dead_cell_risk_penalty,
        side_boost_first: cfg.side_boost_first,
        side_boost_second: cfg.side_boost_second,
        max_total_adjustment: cfg.max_total_adjustment,
        confidence_min: cfg.confidence_min,
        pad: 0,
    };
    for value in raw.shape_boost {
        buf.extend_from_slice(&value.to_le_bytes());
    }
    buf.extend_from_slice(&raw.medium_rect_boost.to_le_bytes());
    buf.extend_from_slice(&raw.barrier_boost.to_le_bytes());
    buf.extend_from_slice(&raw.connection_boost.to_le_bytes());
    buf.extend_from_slice(&raw.dead_cell_risk_penalty.to_le_bytes());
    buf.extend_from_slice(&raw.side_boost_first.to_le_bytes());
    buf.extend_from_slice(&raw.side_boost_second.to_le_bytes());
    buf.extend_from_slice(&raw.max_total_adjustment.to_le_bytes());
    buf.push(raw.confidence_min);
    buf.push(raw.pad);
}

fn push_feature_vector(buf: &mut Vec<u8>, fv: &FeatureVector8) {
    for value in fv.dim {
        buf.extend_from_slice(&value.to_le_bytes());
    }
}

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
