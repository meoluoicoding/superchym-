// main.rs — from main.cpp

use mushroom_bot::opponent_db::g_opponent_db;
use mushroom_bot::protocol::Protocol;

// Embedded data.bin: place your data.bin bytes here if needed.
// Build with feature "embedded_data" to activate, or let it fall back to file.
#[cfg(feature = "embedded_data")]
const EMBEDDED_DATA_BIN: &[u8] = include_bytes!("../data.bin");

#[cfg(not(feature = "embedded_data"))]
const EMBEDDED_DATA_BIN: &[u8] = &[];

fn main() {
    // Load data.bin: try embedded first, then file
    if !EMBEDDED_DATA_BIN.is_empty() {
        let mut db = g_opponent_db().lock().unwrap();
        db.load(EMBEDDED_DATA_BIN);
    } else {
        let mut db = g_opponent_db().lock().unwrap();
        db.load_from_file("data.bin");
    }

    let mut protocol = Protocol::new();
    std::process::exit(protocol.run());
}
