/*
 * agent-i-think-change — NYPC Mushroom Game Engine
 * QR data.bin build: known-fingerprint + move-order-priors
 * Baseline: p1a-killer-v2-khfix (KILLER_HISTORY ON)
 * NO eval/search/PASS changes. Move ordering only.
 */

// === System includes ===
#include <algorithm>
#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iostream>
#include <span>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>


// === Engine Code ===

// ====== From: types.hpp ======

#include <array>
#include <cstddef>
#include <cstdint>


// Board dimensions
constexpr int ROWS = 10;
constexpr int COLS = 17;

// Player constants
constexpr int FIRST_PLAYER = 1;
constexpr int SECOND_PLAYER = -1;
constexpr int NO_OWNER = 0;

// Grid types
using ValueGrid = std::array<std::array<int8_t, COLS>, ROWS>;
using OwnerGrid = std::array<std::array<int8_t, COLS>, ROWS>;

// QR data.bin: Shape classification
enum class ShapeClass : uint8_t {
  RECT_1x2,
  RECT_2x1,
  RECT_1x3,
  RECT_3x1,
  RECT_2x2,
  RECT_1x4,
  RECT_4x1,
  RECT_OTHER
};

enum class Orientation : uint8_t { SQUARE, PORTRAIT, LANDSCAPE };
enum class RegionTag : uint8_t {
  CORNER,
  EDGE,
  CENTER_OUTER,
  CENTER_INNER,
  NONE
};

// QR data.bin: Known fingerprint style IDs
enum class KnownStyle : uint32_t {
  UNKNOWN = 0,
  CORDYCEPS_ATTACK = 1,
  CORDYCEPS_DEFENSE = 2,
  CORDYCEPS_BALANCED = 3,
  RUST_OLD = 4,
  RUST_UPDATE = 5
};

// QR data.bin: Move prior config (from data.bin or compile-time default)
struct MovePriorConfig {
  int16_t shape_boost[8] = {}; // indexed by ShapeClass
  int16_t medium_rect_boost = 0;
  int16_t barrier_boost = 0;
  int16_t connection_boost = 0;
  int16_t dead_cell_risk_penalty = 0;
  int16_t side_boost_FIRST = 0;
  int16_t side_boost_SECOND = 0;
  uint16_t max_total_adjustment =
      3000; // cap to not override killer(9000)/history
  uint8_t confidence_min = 60;
};

// QR data.bin: Feature vector (8-dim, Q8.7 fixed point)
struct alignas(16) FeatureVector8 {
  int16_t dim[8] = {};
};

// QR data.bin: Opponent fingerprint observation (passive/shadow)
struct OpponentFingerprint {
  int move_count = 0;
  int shape_counts[8] = {};
  int total_area = 0;
  int medium_count = 0; // area 5-10
  int large_count = 0;  // area >= 11
  int tall_count = 0;   // portrait
  int wide_count = 0;   // landscape
  int region_counts[4] = {};
  int steal_seen = 0;
  int pass_seen = 0;
  int first_pass_ply = 0;
  int barrier_freq = 0; // moves with barrier_potential > threshold
  int side_ply = 0;     // current ply
  bool we_are_first = false;

  FeatureVector8 to_feature_vector() const;
  KnownStyle match_fingerprint(const struct KnownFingerprint *fps, int count,
                               float &confidence, float &margin) const;
};

// QR data.bin: Known fingerprint prototype (loaded from data.bin)
struct KnownFingerprint {
  uint32_t id;
  KnownStyle style;
  uint8_t side_mask; // bit0=FIRST, bit1=SECOND
  uint8_t min_moves;
  uint8_t confidence_threshold;
  uint8_t margin_to_second;
  FeatureVector8 mean;
  FeatureVector8 var; // inverse weight
  uint32_t prior_config_id;
};

// QR data.bin: MovePriorConfig loaded from data.bin
struct LoadedPriorConfig {
  uint32_t id;
  MovePriorConfig config;
};

// Move representation
struct Move {
  int r1, c1, r2, c2;
  int priority =
      0; // Phase 8c: pre-computed ordering priority (set by Board::legal_moves)

  [[nodiscard]] bool is_pass() const {
    return r1 == -1 && c1 == -1 && r2 == -1 && c2 == -1;
  }

  bool operator==(const Move &other) const {
    return r1 == other.r1 && c1 == other.c1 && r2 == other.r2 && c2 == other.c2;
  }
};

constexpr Move PASS_MOVE{-1, -1, -1, -1};

// Return the opponent of a player
constexpr int opponent(int player) { return -player; }

// === QR Shape/Feature Helpers (constexpr, zero-overhead) ===

[[nodiscard]] constexpr ShapeClass classify_shape(int r1, int c1, int r2,
                                                  int c2) {
  int h = r2 - r1 + 1;
  int w = c2 - c1 + 1;
  if (h == 1 && w == 2)
    return ShapeClass::RECT_1x2;
  if (h == 2 && w == 1)
    return ShapeClass::RECT_2x1;
  if (h == 1 && w == 3)
    return ShapeClass::RECT_1x3;
  if (h == 3 && w == 1)
    return ShapeClass::RECT_3x1;
  if (h == 2 && w == 2)
    return ShapeClass::RECT_2x2;
  if (h == 1 && w == 4)
    return ShapeClass::RECT_1x4;
  if (h == 4 && w == 1)
    return ShapeClass::RECT_4x1;
  return ShapeClass::RECT_OTHER;
}

[[nodiscard]] constexpr Orientation classify_orientation(int r1, int c1, int r2,
                                                         int c2) {
  int h = r2 - r1 + 1;
  int w = c2 - c1 + 1;
  if (h == w)
    return Orientation::SQUARE;
  if (h > w)
    return Orientation::PORTRAIT;
  return Orientation::LANDSCAPE;
}

[[nodiscard]] constexpr RegionTag classify_region(int r1, int c1, int r2,
                                                  int c2) {
  // Corner: within 2 cells of any corner
  bool near_corner = (r1 <= 1 && c1 <= 1) || (r1 <= 1 && c2 >= COLS - 2) ||
                     (r2 >= ROWS - 2 && c1 <= 1) ||
                     (r2 >= ROWS - 2 && c2 >= COLS - 2);
  if (near_corner)
    return RegionTag::CORNER;

  // Edge: within 1 cell of any edge
  bool near_edge =
      (r1 == 0) || (r2 == ROWS - 1) || (c1 == 0) || (c2 == COLS - 1);
  if (near_edge)
    return RegionTag::EDGE;

  // Center-inner: within center 4x6
  int cr = (r1 + r2) / 2, cc = (c1 + c2) / 2;
  if (cr >= 3 && cr <= 6 && cc >= 5 && cc <= 11)
    return RegionTag::CENTER_INNER;
  return RegionTag::CENTER_OUTER;
}

[[nodiscard]] constexpr int move_area(int r1, int c1, int r2, int c2) {
  return (r2 - r1 + 1) * (c2 - c1 + 1);
}

// ====== From: timer.hpp ======

#include <chrono>

class Timer {
public:
  void start() { start_ = std::chrono::steady_clock::now(); }
  [[nodiscard]] int64_t elapsed_ms() const {
    auto now = std::chrono::steady_clock::now();
    return std::chrono::duration_cast<std::chrono::milliseconds>(now - start_)
        .count();
  }
  [[nodiscard]] bool timed_out(int64_t budget_ms) const {
    return elapsed_ms() >= budget_ms;
  }

private:
  std::chrono::steady_clock::time_point start_;
};

// ====== From: board.hpp ======

#include <string>
#include <vector>

// Phase 10: Move record for undo â€” stores all changes made by a move.
// Enables make_move/unmake_move instead of full Board copy (O(170) â†’
// O(changed_cells)).
struct MoveRecord {
  static constexpr int MAX_CHANGES = 170; // worst case: whole board
  struct CellChange {
    int8_t r, c;
    int8_t old_value;
    int8_t old_owner;
  };
  CellChange changes[MAX_CHANGES];
  int num_changes = 0;
  int old_player = 0;
  int old_consecutive_passes = 0;
  bool was_pass = false;
};

class Board {
public:
  Board();

  // Parse INIT input: 10 space-separated strings of 17 digits each
  void init_from_string(const std::string &board_str);

  // Accessors
  const ValueGrid &values() const { return values_; }
  const OwnerGrid &owners() const { return owners_; }
  int player() const { return player_; }
  int consecutive_passes() const { return consecutive_passes_; }

  void set_player(int p) { player_ = p; }

  // Apply a pass move (no undo support â€” used by protocol)
  void apply_pass();

  // Apply a non-pass move (no undo support â€” used by protocol)
  void apply_move(const Move &move);

  // Phase 10: Make a move and record changes for undo.
  // After calling, call unmake_move(record) to restore state.
  void make_move(const Move &move, MoveRecord &record);

  // Phase 10: Undo a move using the record from make_move.
  void unmake_move(const MoveRecord &record);

  // Validate a rectangle move (check sum=10 and edge rule)
  [[nodiscard]] bool is_legal_move(const Move &move) const;

  // Generate all legal moves for current values
  [[nodiscard]] std::vector<Move> legal_moves() const;

  // Count owned cells for a player
  [[nodiscard]] int owned_cells(int player) const;

  // QR data.bin: Feature helpers
  [[nodiscard]] int barrier_potential(int r1, int c1, int r2, int c2) const;
  [[nodiscard]] int dead_cell_risk_proxy(int r1, int c1, int r2, int c2) const;
  [[nodiscard]] int connectivity_boost(int r1, int c1, int r2, int c2) const;

private:
  ValueGrid values_{};
  OwnerGrid owners_{};
  int player_ = FIRST_PLAYER;
  int consecutive_passes_ = 0;
};

// ====== From: movegen.hpp ======

#include <vector>

// Generate all legal moves for the current board state.
// A move is legal if:
//   1. Sum of live mushroom values in rectangle == 10
//   2. Each of the 4 edges touches >=1 live mushroom (inscribed rule)
// Uses 2D prefix sum for O(1) rectangle sum.
// Uses row-band early break: stops expanding column when sum exceeds 10.
[[nodiscard]] std::vector<Move> generate_legal_moves(const ValueGrid &values);

// ====== From: eval.hpp ======

// Evaluate a board position from the current player's perspective.
// Higher = better for current player.
// is_first: true if our engine plays FIRST (infrastructure for Phase 8b)
[[nodiscard]] int evaluate(const Board &board, bool is_first = false);

// P0.1: Instrumentation
extern uint64_t eval_calls;

// Score a single move by applying it and evaluating the resulting position.
[[nodiscard]] int score_move(const Board &board, const Move &move,
                             bool is_first = false);

// ====== From: search.hpp ======

#include <cstdint>
#include <vector>

// Zobrist hashing for transposition table.
uint64_t hash_board(const Board &board);

// P0.3: Compact TT entry â€” 16 bytes (4 entries per 64B cache line)
// key_sig = upper 32 bits of Zobrist hash (collision check)
// depth = search depth (int16_t fits max depth 12)
// age = generation counter, replaces tt_clear() â€” stale entries naturally
// expire packed_move = r1(4)|c1(5)|r2(4)|c2(5)|pass(1) = 19 bits in uint32_t
struct CompactTTEntry {
  uint32_t key_sig = 0;
  int16_t depth = -1;
  uint8_t flag = 0;
  uint8_t age = 0;
  int32_t value = 0;
  uint32_t packed_move = 0;
};

// P0.4: 2-slot TT bucket â€” 32 bytes (2 buckets per 64B cache line)
// Slot0 = always-replace (most recent entry)
// Slot1 = depth-preferred (keep deepest entry); stale entries replaceable
struct TTBucket {
  CompactTTEntry slot0;
  CompactTTEntry slot1;
};

// Flag constants
namespace TTFlag {
constexpr uint8_t EMPTY = 0;
constexpr uint8_t EXACT = 1;
constexpr uint8_t LOWER_BOUND = 2;
constexpr uint8_t UPPER_BOUND = 3;
} // namespace TTFlag

// P0b-rescue-v1: restore the high-capacity TT used by the stronger P0b
// baseline. The 16 MiB v2 table was safe but too weak in BTC diagnostics.
constexpr size_t TT_SIZE = 1 << 22;

extern std::vector<TTBucket> tt;
extern uint8_t tt_age; // incremented each search â€” replaces tt_clear()
void ensure_tt_ready();

// Pack/unpack Move into 32-bit (19 bits used)
uint32_t pack_move(const Move &m);
Move unpack_move(uint32_t packed);

void tt_store(uint64_t key, int depth, int value, uint8_t flag,
              const Move &best_move);
bool tt_probe(uint64_t key, int depth, int alpha, int beta, int &value,
              Move &best_move);

[[nodiscard]] Move search_best_move(const Board &board, int time_budget_ms,
                                    bool is_first = false);
extern uint64_t nodes_searched;

// ====== From: protocol.hpp ======

#include <cstdio>

class Protocol {
public:
  // Run the main protocol loop (reads stdin, writes stdout)
  // Returns 0 on success, non-zero on error
  int run();

private:
  // Read a line from stdin, returns empty string on EOF
  static std::string read_line();

  // Write a line to stdout
  static void write_line(const std::string &line);

  // Handlers
  void handle_ready(const std::string &line);
  void handle_init(const std::string &line);
  void handle_time(const std::string &line);
  void handle_opp(const std::string &line);

  // QR data.bin: passive fingerprint â€” log opponent observations
  void log_shadow_metrics();

  Board board_;
  bool i_am_first_ = false;
  bool running_ = true;
  int opp_consecutive_passes_ = 0; // P0b: opponent consecutive passes
  int opp_passes_since_our_move_ =
      0; // detect Always Pass artifact (2 opp passes before our move)

  // QR data.bin: passive opponent fingerprint
  OpponentFingerprint opp_fp_;
  int move_counter_ = 0;     // total moves (ours + opponent)
  int opp_move_counter_ = 0; // opponent moves only
  int ply_counter_ = 0;      // ply (half-moves from start)
  KnownStyle matched_style_ = KnownStyle::UNKNOWN;
  float match_confidence_ = 0.0f;
  bool fingerprint_checked_ = false;
};

// ====== From: opponent_db.hpp ======

#include <cstdint>
#include <cstring>
#include <span>

// QR data.bin loader â€” tiny, read-only, no mmap.
// Format defined in ROADMAP_DATA_BIN_FINAL_QR.md section 2.
// Missing/corrupt => fallback p1a (no crash).

// === Binary Section Types ===
constexpr uint32_t DB_MAGIC = 0x44415441;      // "DATA" LE
constexpr uint32_t DB_FOOTER_MAGIC = 0x544144; // "DAT" reversed
constexpr uint32_t DB_VERSION = 3;
constexpr uint32_t DB_SECTION_FINGERPRINTS = 1;
constexpr uint32_t DB_SECTION_CENTROIDS = 2;
constexpr uint32_t DB_SECTION_PRIOR_CONFIGS = 3;
constexpr uint32_t DB_SECTION_METADATA = 4;

// === Header ===
struct DBHeader {
  uint32_t magic;
  uint32_t version;
  uint32_t section_count;
  uint32_t crc32;
  uint64_t build_id;
  uint64_t baseline_id;
  uint32_t reserved[2];
};

// === Section Header ===
struct DBSection {
  uint32_t type;
  uint32_t count;
  uint32_t data_size;
  uint32_t reserved;
};

// === Footer ===
struct DBFooter {
  uint32_t magic;
  uint32_t total_size;
};

// === data.bin Loader ===
class OpponentDB {
public:
  OpponentDB() = default;

  // Load from memory buffer (embedded in binary or read from file)
  bool load(const uint8_t *data, size_t len);

  // Load from file path
  bool load_from_file(const char *path);

  // Get known fingerprints
  std::span<const KnownFingerprint> fingerprints() const {
    return {fingerprints_, static_cast<size_t>(fingerprint_count_)};
  }

  // Get prior config by ID (returns nullptr if not found)
  const MovePriorConfig *get_prior_config(uint32_t id) const;

  // Get default prior config (id=0 or first available)
  const MovePriorConfig *default_prior_config() const;

  bool is_loaded() const { return loaded_; }
  uint64_t build_id() const { return header_.build_id; }
  uint64_t baseline_id() const { return header_.baseline_id; }

private:
  bool validate_crc32() const;
  bool parse_sections();

  DBHeader header_ = {};
  bool loaded_ = false;
  const uint8_t *data_ = nullptr;
  size_t data_size_ = 0;

  static constexpr int MAX_FINGERPRINTS = 16;
  static constexpr int MAX_PRIOR_CONFIGS = 16;
  KnownFingerprint fingerprints_[MAX_FINGERPRINTS] = {};
  int fingerprint_count_ = 0;
  LoadedPriorConfig prior_configs_[MAX_PRIOR_CONFIGS] = {};
  int prior_config_count_ = 0;
};

// Global instance (initialized in main.cpp)
extern OpponentDB g_opponent_db;

// Active prior config (set by OpponentDB::load or defaults)
// Used by board.cpp priority calculation
extern const MovePriorConfig *g_active_prior_config;

// ====== From: board.cpp ======
#include <sstream>
#include <stdexcept>

Board::Board() {
  for (auto &row : owners_) {
    row.fill(NO_OWNER);
  }
}

void Board::init_from_string(const std::string &board_str) {
  std::istringstream iss(board_str);
  std::string row_str;

  for (int r = 0; r < ROWS; ++r) {
    if (!(iss >> row_str)) {
      throw std::runtime_error("INIT: insufficient rows (expected 10)");
    }
    if (row_str.length() != COLS) {
      throw std::runtime_error("INIT: row " + std::to_string(r) + " has " +
                               std::to_string(row_str.length()) +
                               " columns, expected " + std::to_string(COLS));
    }
    for (int c = 0; c < COLS; ++c) {
      char ch = row_str[c];
      if (ch < '0' || ch > '9') {
        throw std::runtime_error("INIT: invalid digit '" + std::string(1, ch) +
                                 "' at (" + std::to_string(r) + "," +
                                 std::to_string(c) + ")");
      }
      values_[r][c] = static_cast<int8_t>(ch - '0');
    }
  }

  for (auto &row : owners_) {
    row.fill(NO_OWNER);
  }
  consecutive_passes_ = 0;
}

void Board::apply_pass() {
  ++consecutive_passes_;
  player_ = opponent(player_);
}

void Board::apply_move(const Move &move) {
  if (move.is_pass()) {
    apply_pass();
    return;
  }

  for (int r = move.r1; r <= move.r2; ++r) {
    for (int c = move.c1; c <= move.c2; ++c) {
      values_[r][c] = 0;
      owners_[r][c] = player_;
    }
  }

  consecutive_passes_ = 0;
  player_ = opponent(player_);
}

// Phase 10: make_move â€” apply + record for undo
void Board::make_move(const Move &move, MoveRecord &record) {
  record.old_player = player_;
  record.old_consecutive_passes = consecutive_passes_;
  record.num_changes = 0;

  if (move.is_pass()) {
    record.was_pass = true;
    ++consecutive_passes_;
    player_ = opponent(player_);
    return;
  }

  record.was_pass = false;
  for (int r = move.r1; r <= move.r2; ++r) {
    for (int c = move.c1; c <= move.c2; ++c) {
      auto &ch = record.changes[record.num_changes++];
      ch.r = static_cast<int8_t>(r);
      ch.c = static_cast<int8_t>(c);
      ch.old_value = values_[r][c];
      ch.old_owner = owners_[r][c];
      values_[r][c] = 0;
      owners_[r][c] = player_;
    }
  }

  consecutive_passes_ = 0;
  player_ = opponent(player_);
}

// Phase 10: unmake_move â€” restore from record
void Board::unmake_move(const MoveRecord &record) {
  player_ = record.old_player;
  consecutive_passes_ = record.old_consecutive_passes;

  if (record.was_pass)
    return;

  for (int i = 0; i < record.num_changes; ++i) {
    const auto &ch = record.changes[i];
    values_[ch.r][ch.c] = ch.old_value;
    owners_[ch.r][ch.c] = ch.old_owner;
  }
}

bool Board::is_legal_move(const Move &move) const {
  if (move.is_pass())
    return true;

  if (move.r1 < 0 || move.r2 >= ROWS || move.c1 < 0 || move.c2 >= COLS)
    return false;
  if (move.r1 > move.r2 || move.c1 > move.c2)
    return false;

  int sum = 0;
  for (int r = move.r1; r <= move.r2; ++r) {
    for (int c = move.c1; c <= move.c2; ++c) {
      if (values_[r][c] > 0) {
        sum += values_[r][c];
        if (sum > 10)
          return false;
      }
    }
  }
  if (sum != 10)
    return false;

  bool top = false, bottom = false, left = false, right = false;
  for (int c = move.c1; c <= move.c2; ++c) {
    if (values_[move.r1][c] > 0)
      top = true;
    if (values_[move.r2][c] > 0)
      bottom = true;
  }
  for (int r = move.r1; r <= move.r2; ++r) {
    if (values_[r][move.c1] > 0)
      left = true;
    if (values_[r][move.c2] > 0)
      right = true;
  }
  return top && bottom && left && right;
}

std::vector<Move> Board::legal_moves() const {
  auto moves = generate_legal_moves(values_);
  for (auto &m : moves) {
    if (m.is_pass()) {
      m.priority = 0;
      continue;
    }
    int steal = 0;
    int area = (m.r2 - m.r1 + 1) * (m.c2 - m.c1 + 1);
    for (int r = m.r1; r <= m.r2; ++r)
      for (int c = m.c1; c <= m.c2; ++c)
        if (owners_[r][c] == opponent(player_))
          ++steal;
    int height = m.r2 - m.r1 + 1;
    int width = m.c2 - m.c1 + 1;
    int portrait_bonus = (height > width) ? 500 : 0;
    int small_bonus = (area <= 4) ? 300 : 0;
    m.priority = steal * 1000 + area + portrait_bonus + small_bonus;

    // QR data.bin: apply active prior config (move ordering only)
    const auto *cfg = g_active_prior_config;
    if (cfg && cfg != nullptr) {
      int adj = 0;
      ShapeClass sc = classify_shape(m.r1, m.c1, m.r2, m.c2);
      adj += cfg->shape_boost[static_cast<int>(sc)];
      if (area >= 5 && area <= 10)
        adj += cfg->medium_rect_boost;
      if (barrier_potential(m.r1, m.c1, m.r2, m.c2) > 0)
        adj += cfg->barrier_boost;
      adj += connectivity_boost(m.r1, m.c1, m.r2, m.c2) *
             cfg->connection_boost / 4;
      adj -= dead_cell_risk_proxy(m.r1, m.c1, m.r2, m.c2) *
             cfg->dead_cell_risk_penalty / 4;
      // Cap max adjustment to keep killer/history dominant
      if (adj > static_cast<int>(cfg->max_total_adjustment))
        adj = cfg->max_total_adjustment;
      else if (adj < -static_cast<int>(cfg->max_total_adjustment))
        adj = -static_cast<int>(cfg->max_total_adjustment);
      m.priority += adj;
    }
  }
  return moves;
}

int Board::owned_cells(int player) const {
  int count = 0;
  for (const auto &row : owners_) {
    for (int8_t owner : row) {
      if (owner == player)
        ++count;
    }
  }
  return count;
}

// QR: Barrier potential â€” how much this move blocks opponent expansion.
// A tall/narrow rectangle in center separates board halves.
int Board::barrier_potential(int r1, int c1, int r2, int c2) const {
  int h = r2 - r1 + 1;
  int w = c2 - c1 + 1;
  // Vertical barrier (portrait): splits board left/right
  if (h >= 4 && w <= 2) {
    int cc = (c1 + c2) / 2;
    if (cc >= 4 && cc <= 12)
      return 2; // strong barrier
    return 1;
  }
  // Horizontal barrier (landscape): splits board top/bottom
  if (w >= 6 && h <= 2) {
    int cr = (r1 + r2) / 2;
    if (cr >= 3 && cr <= 6)
      return 2;
    return 1;
  }
  return 0;
}

// QR: Dead cell risk â€” cells in this move that won't connect to our
// territory. Higher risk = more likely opponent steals them later.
int Board::dead_cell_risk_proxy(int r1, int c1, int r2, int c2) const {
  int risk = 0;
  for (int r = r1; r <= r2; ++r) {
    for (int c = c1; c <= c2; ++c) {
      if (owners_[r][c] == player_)
        continue; // already ours, no new risk
      // Check how many sides are protected by our cells
      int protection = 0;
      if (r > 0 && owners_[r - 1][c] == player_)
        ++protection;
      if (r < ROWS - 1 && owners_[r + 1][c] == player_)
        ++protection;
      if (c > 0 && owners_[r][c - 1] == player_)
        ++protection;
      if (c < COLS - 1 && owners_[r][c + 1] == player_)
        ++protection;
      // Also check values: if cell has high value, opponent may target it
      bool adjacent_live = false;
      if (r > 0 && values_[r - 1][c] > 0)
        adjacent_live = true;
      if (r < ROWS - 1 && values_[r + 1][c] > 0)
        adjacent_live = true;
      if (c > 0 && values_[r][c - 1] > 0)
        adjacent_live = true;
      if (c < COLS - 1 && values_[r][c + 1] > 0)
        adjacent_live = true;
      // Risk = unprotected or vulnerable to steal
      if (protection == 0 && adjacent_live)
        risk += 2;
      else if (protection <= 1 && adjacent_live)
        risk += 1;
    }
  }
  return risk;
}

// QR: Connectivity â€” adjacent own cells around the move
int Board::connectivity_boost(int r1, int c1, int r2, int c2) const {
  int boost = 0;
  for (int r = r1; r <= r2; ++r) {
    for (int c = c1; c <= c2; ++c) {
      if (owners_[r][c] == player_)
        continue;
      if (r > 0 && owners_[r - 1][c] == player_)
        ++boost;
      if (r < ROWS - 1 && owners_[r + 1][c] == player_)
        ++boost;
      if (c > 0 && owners_[r][c - 1] == player_)
        ++boost;
      if (c < COLS - 1 && owners_[r][c + 1] == player_)
        ++boost;
    }
  }
  return boost;
}

// ====== From: movegen.cpp ======

// 2D prefix sum array: pref[r+1][c+1] = sum of values_[0..r][0..c]
// Size: (ROWS+1) x (COLS+1) for easier boundary handling
using PrefixSum = std::array<std::array<int, COLS + 1>, ROWS + 1>;

// Build 2D prefix sum from value grid
static PrefixSum build_prefix_sum(const ValueGrid &values) {
  PrefixSum pref{};
  for (int r = 0; r < ROWS; ++r) {
    for (int c = 0; c < COLS; ++c) {
      int val = (values[r][c] > 0) ? values[r][c] : 0;
      pref[r + 1][c + 1] = val + pref[r][c + 1] + pref[r + 1][c] - pref[r][c];
    }
  }
  return pref;
}

// O(1) rectangle sum using prefix sum
static int rect_sum(const PrefixSum &pref, int r1, int c1, int r2, int c2) {
  return pref[r2 + 1][c2 + 1] - pref[r1][c2 + 1] - pref[r2 + 1][c1] +
         pref[r1][c1];
}

// Check inscribed rule: all 4 edges must touch >=1 live mushroom
static bool check_inscribed(const ValueGrid &values, int r1, int c1, int r2,
                            int c2) {
  bool top = false, bottom = false, left = false, right = false;
  for (int c = c1; c <= c2; ++c) {
    if (values[r1][c] > 0)
      top = true;
    if (values[r2][c] > 0)
      bottom = true;
  }
  for (int r = r1; r <= r2; ++r) {
    if (values[r][c1] > 0)
      left = true;
    if (values[r][c2] > 0)
      right = true;
  }
  return top && bottom && left && right;
}

std::vector<Move> generate_legal_moves(const ValueGrid &values) {
  std::vector<Move> moves;
  PrefixSum pref = build_prefix_sum(values);

  // Row-band early break algorithm (from Python Phase 27)
  // For each top row r1, bottom row r2, left col c1:
  //   Expand c2 rightward until sum > 10, then break (can only grow)
  for (int r1 = 0; r1 < ROWS; ++r1) {
    for (int r2 = r1; r2 < ROWS; ++r2) {
      for (int c1 = 0; c1 < COLS; ++c1) {
        for (int c2 = c1; c2 < COLS; ++c2) {
          int sum = rect_sum(pref, r1, c1, r2, c2);
          if (sum > 10) {
            // Row-band early break: sum only increases as c2 expands
            break;
          }
          if (sum == 10 && check_inscribed(values, r1, c1, r2, c2)) {
            moves.push_back(Move{r1, c1, r2, c2});
          }
        }
      }
    }
  }

  return moves;
}

// ====== From: eval.cpp ======
#include <algorithm>
#include <cstdlib>
#include <fstream>

// P0.1: Instrumentation
uint64_t eval_calls = 0;

// === Board-level evaluation (used for 1-ply lookahead) ===

int evaluate(const Board &board, bool /*is_first*/) {
  ++eval_calls;
  // Phase 8c: symmetric eval (is_first flag threaded for future use, not
  // activated) REVERTED Phase 8b asymmetric weights â€” caused SECOND
  // regression vs Cordyceps (80% vs 84%)
  const auto &owners = board.owners();
  const auto &values = board.values();
  int player = board.player();
  int opp = opponent(player);

  // 1. Territory: owned cells â€” Phase 8d.3: branchless counting
  // trueâ†’1, falseâ†’0 via bool-to-int conversion (SETcc + ADD, no branch)
  int own_cells = 0;
  int opp_cells = 0;
  for (int r = 0; r < ROWS; ++r) {
    for (int c = 0; c < COLS; ++c) {
      own_cells += (owners[r][c] == player);
      opp_cells += (owners[r][c] == opp);
    }
  }
  int territory = own_cells - opp_cells;

  // 2. Mobility: P0.5 â€” own==opp always (legal_moves depends only on values_,
  // not owners_/player_) Removed redundant legal_moves() call â†’ saves ~50%
  // eval time mobility_score = own - opp = 0 always; kept for weight file
  // compatibility
  int mobility_score = 0;

  // 3. Connectivity: adjacent owned pairs (harder to steal)
  int connectivity = 0;
  for (int r = 0; r < ROWS; ++r) {
    for (int c = 0; c < COLS; ++c) {
      if (owners[r][c] != player)
        continue;
      if (c + 1 < COLS && owners[r][c + 1] == player)
        ++connectivity;
      if (r + 1 < ROWS && owners[r + 1][c] == player)
        ++connectivity;
    }
  }

  // 4. Positional: corners and edges â€” Phase 8d.6: constexpr positions
  int corners = 0;
  int edges = 0;
  constexpr int corner_positions[4][2] = {
      {0, 0}, {0, COLS - 1}, {ROWS - 1, 0}, {ROWS - 1, COLS - 1}};
  for (const auto &cp : corner_positions) {
    if (owners[cp[0]][cp[1]] == player)
      ++corners;
    else if (owners[cp[0]][cp[1]] == opp)
      --corners;
  }
  for (int c = 0; c < COLS; ++c) {
    if (owners[0][c] == player)
      ++edges;
    else if (owners[0][c] == opp)
      --edges;
    if (owners[ROWS - 1][c] == player)
      ++edges;
    else if (owners[ROWS - 1][c] == opp)
      --edges;
  }
  for (int r = 1; r < ROWS - 1; ++r) {
    if (owners[r][0] == player)
      ++edges;
    else if (owners[r][0] == opp)
      --edges;
    if (owners[r][COLS - 1] == player)
      ++edges;
    else if (owners[r][COLS - 1] == opp)
      --edges;
  }

  // 5. Recapture swing: bonus for opponent cells we can steal
  int recapture_swing = 0;
  // 6. Vulnerability: penalty for our cells opponent can steal
  int vulnerability = 0;

  for (int r = 0; r < ROWS; ++r) {
    for (int c = 0; c < COLS; ++c) {
      bool adjacent_to_live = false;
      if (r > 0 && values[r - 1][c] > 0)
        adjacent_to_live = true;
      if (r < ROWS - 1 && values[r + 1][c] > 0)
        adjacent_to_live = true;
      if (c > 0 && values[r][c - 1] > 0)
        adjacent_to_live = true;
      if (c < COLS - 1 && values[r][c + 1] > 0)
        adjacent_to_live = true;

      if (adjacent_to_live) {
        if (owners[r][c] == opp)
          ++recapture_swing; // we can steal
        else if (owners[r][c] == player)
          ++vulnerability; // they can steal
      }
    }
  }

  // P2: p1a tuned weights (148/20/19/18/3/39/9).
  // EVAL_WEIGHTS_FILE env var for local testing override.
  static int w_territory = 148, w_mobility = 20, w_connectivity = 19;
  static int w_corners = 18, w_edges = 3, w_recapture = 39, w_vulnerability = 9;
  static bool w_loaded = false;

  if (!w_loaded) {
    const char *fname = std::getenv("EVAL_WEIGHTS_FILE");
    if (fname) {
      std::ifstream wf(fname);
      if (wf.is_open()) {
        wf >> w_territory >> w_mobility >> w_connectivity >> w_corners >>
            w_edges >> w_recapture >> w_vulnerability;
        wf.close();
      }
    }
    w_loaded = true;
  }

  int score = territory * w_territory + mobility_score * w_mobility +
              connectivity * w_connectivity + corners * w_corners +
              edges * w_edges + recapture_swing * w_recapture -
              vulnerability * w_vulnerability;

  return score;
}

// === Move scoring via 1-ply lookahead ===

int score_move(const Board &board, const Move &move, bool is_first) {
  if (move.is_pass()) {
    // Phase 8a.2 hotfix: evaluate actual pass value instead of always 0
    Board copy = board;
    copy.apply_pass();
    return evaluate(copy, is_first);
  }

  // Copy board, apply move, evaluate the resulting position
  Board copy = board;
  copy.apply_move(move);
  return evaluate(copy, is_first);
}

// ====== From: search.cpp ======
#include <algorithm>
#include <cstdlib>
#include <string>

// === Zobrist Keys ===
static uint64_t zobrist_value[ROWS][COLS][10];
static uint64_t zobrist_owner[ROWS][COLS][3];
static uint64_t zobrist_player;
static uint64_t zobrist_passes[3]; // P0b-safe: hash consecutive passes (0,1,2)
static bool zobrist_initialized = false;

static uint64_t xorshift64(uint64_t &state) {
  state ^= state << 13;
  state ^= state >> 7;
  state ^= state << 17;
  return state;
}

static void init_zobrist() {
  if (zobrist_initialized)
    return;
  uint64_t seed = 1234567890123456789ULL;
  for (int r = 0; r < ROWS; ++r)
    for (int c = 0; c < COLS; ++c)
      for (int v = 0; v < 10; ++v)
        zobrist_value[r][c][v] = xorshift64(seed);
  for (int r = 0; r < ROWS; ++r)
    for (int c = 0; c < COLS; ++c)
      for (int o = 0; o < 3; ++o)
        zobrist_owner[r][c][o] = xorshift64(seed);
  zobrist_player = xorshift64(seed);
  for (int i = 0; i < 3; ++i)
    zobrist_passes[i] = xorshift64(seed);
  zobrist_initialized = true;
}

uint64_t hash_board(const Board &board) {
  init_zobrist();
  uint64_t h = 0;
  const auto &values = board.values();
  const auto &owners = board.owners();
  for (int r = 0; r < ROWS; ++r) {
    for (int c = 0; c < COLS; ++c) {
      int v = values[r][c];
      if (v > 0 && v <= 9)
        h ^= zobrist_value[r][c][v];
      int o = owners[r][c];
      int oi = (o == FIRST_PLAYER) ? 1 : (o == SECOND_PLAYER) ? 2 : 0;
      h ^= zobrist_owner[r][c][oi];
    }
  }
  if (board.player() == FIRST_PLAYER)
    h ^= zobrist_player;
  int cp = board.consecutive_passes();
  if (cp >= 0 && cp <= 2)
    h ^= zobrist_passes[cp];
  return h;
}

// === Flat Array 2-Slot Transposition Table ===
// P0b-rescue-v1: keep the strong 128 MiB table, but allocate lazily after
// READY. Global allocation made the local/judge handshake time out before OK.
std::vector<TTBucket> tt;
uint8_t tt_age = 1; // start at 1 â€” default age=0 is stale
uint64_t nodes_searched = 0;

// P0.1: Instrumentation counters
static uint64_t tt_probes = 0;
static uint64_t tt_hits = 0;
static uint64_t tt_collisions = 0;

// PASS instrument counters (global, reset per search)
static int pass_nodes = 0;
static int pass_chosen = 0;
static int pass_mode = 0; // 0=current, 1=no-gate, 2=none
static int pass_fix = 0;  // 0=baseline (static_eval>0), 1=fix (margin>0).
                          // Default OFF â€” causes -10% regression

// Killer moves + history heuristic: better move ordering â†’ deeper search.
// Killer: 2 moves per depth that caused beta cutoffs (most recent first).
// History: increment counter when move causes cutoff, order by frequency.
static uint32_t killer1[64] = {};
static uint32_t killer2[64] = {};
static int history_score[ROWS][COLS] = {};
static int killer_history_on =
    1; // env toggle (default ON â€” proven +2.5% vs Rust old)
static int nullmove_on = 0; // env toggle

void ensure_tt_ready() {
  if (tt.size() != TT_SIZE) {
    tt.assign(TT_SIZE, TTBucket{});
  }
}

// P0.3: Pack/unpack Move into 32-bit (19 bits used), 0xFFFFFFFF = PASS
uint32_t pack_move(const Move &m) {
  if (m.is_pass())
    return 0xFFFFFFFF;
  return (static_cast<uint32_t>(m.r1 & 0xF) << 15) |
         (static_cast<uint32_t>(m.c1 & 0x1F) << 10) |
         (static_cast<uint32_t>(m.r2 & 0xF) << 6) |
         (static_cast<uint32_t>(m.c2 & 0x1F) << 1);
  // bit 0 = 0 means not a pass
}

Move unpack_move(uint32_t packed) {
  if (packed == 0xFFFFFFFF)
    return PASS_MOVE;
  int r1 = static_cast<int>((packed >> 15) & 0xF);
  int c1 = static_cast<int>((packed >> 10) & 0x1F);
  int r2 = static_cast<int>((packed >> 6) & 0xF);
  int c2 = static_cast<int>((packed >> 1) & 0x1F);
  return Move{r1, c1, r2, c2};
}

// P0.4: 2-slot TT store â€” slot0 always-replace, slot1 depth-preferred+stale
void tt_store(uint64_t key, int depth, int value, uint8_t flag,
              const Move &best_move) {
  ensure_tt_ready();
  uint32_t ksig = static_cast<uint32_t>(key >> 32);
  size_t idx = key & (TT_SIZE - 1);
  auto &b = tt[idx];
  uint32_t pm = pack_move(best_move);
  int16_t d16 = static_cast<int16_t>(depth);

  // Slot0: always replace (most recent)
  b.slot0.key_sig = ksig;
  b.slot0.depth = d16;
  b.slot0.value = value;
  b.slot0.flag = flag;
  b.slot0.age = tt_age;
  b.slot0.packed_move = pm;

  // Slot1: depth-preferred
  // - Same key_sig â†’ replace if new_depth >= stored (update same pos)
  // - Different key_sig â†’ replace if stale OR deeper
  bool same_key = (b.slot1.key_sig == ksig);
  bool stale = (b.slot1.age != tt_age);
  bool deeper = (depth >= b.slot1.depth);

  if (same_key ? deeper : (stale || deeper)) {
    b.slot1.key_sig = ksig;
    b.slot1.depth = d16;
    b.slot1.value = value;
    b.slot1.flag = flag;
    b.slot1.age = tt_age;
    b.slot1.packed_move = pm;
  }
}

// P0.4: 2-slot TT probe â€” checks both slots, prefers deeper match
bool tt_probe(uint64_t key, int depth, int alpha, int beta, int &value,
              Move &best_move) {
  ensure_tt_ready();
  ++tt_probes;
  uint32_t ksig = static_cast<uint32_t>(key >> 32);
  size_t idx = key & (TT_SIZE - 1);
  const auto &b = tt[idx];

  // Find best matching slot (prefer deeper)
  const CompactTTEntry *best = nullptr;
  if (b.slot0.key_sig == ksig && b.slot0.depth >= depth) {
    best = &b.slot0;
  }
  if (b.slot1.key_sig == ksig && b.slot1.depth >= depth) {
    if (!best || b.slot1.depth > best->depth) {
      best = &b.slot1;
    }
  }

  if (!best) {
    ++tt_collisions;
    return false;
  }

  ++tt_hits;
  best_move = unpack_move(best->packed_move);
  int stored = best->value;

  if (best->flag == TTFlag::EXACT) {
    value = stored;
    return true;
  }
  if (best->flag == TTFlag::LOWER_BOUND && stored >= beta) {
    value = stored;
    return true;
  }
  if (best->flag == TTFlag::UPPER_BOUND && stored <= alpha) {
    value = stored;
    return true;
  }
  return false;
}

// === Move Ordering ===
static void order_moves(std::vector<Move> &moves, const Move &pv_move,
                        int depth) {
  // Pre-boost: add killer/history bonus to priority
  if (killer_history_on && depth >= 0 && depth < 64) {
    for (auto &m : moves) {
      uint32_t pm = pack_move(m);
      if (pm == killer1[depth])
        m.priority += 9000;
      else if (pm == killer2[depth])
        m.priority += 8000;
      else {
        int h = history_score[m.r2][m.c2];
        if (h > 0)
          m.priority += h * 10;
      }
    }
  }
  std::sort(moves.begin(), moves.end(), [&](const Move &a, const Move &b) {
    if (a == pv_move)
      return true;
    if (b == pv_move)
      return false;
    return a.priority > b.priority;
  });
}

// === Alpha-Beta Search (Phase 10: uses make_move/unmake_move, no quiescence)
// ===
static int alpha_beta(Board &board, int depth, int alpha, int beta,
                      bool /*maximizing*/, const Timer &timer,
                      int64_t budget_ms, Move &best_move, bool is_first) {
  // P0b-safe: Terminal check â€” game over if both passed consecutively
  // Terminal score must dominate ALL heuristic eval (max ~30000) to ensure
  // search correctly values guaranteed win/loss over uncertain positions
  if (board.consecutive_passes() >= 2) {
    int p = board.player();
    int opp = opponent(p);
    int margin = board.owned_cells(p) - board.owned_cells(opp);
    if (margin > 0)
      return 100000 + margin;
    if (margin < 0)
      return -100000 + margin;
    return 0;
  }

  if ((nodes_searched & 4095) == 0 && timer.timed_out(budget_ms)) {
    return evaluate(board, is_first);
  }
  ++nodes_searched;

  // Phase 10: Static eval at horizon (quiescence removed)
  if (depth == 0) {
    return evaluate(board, is_first);
  }

  // TT probe
  uint64_t key = hash_board(board);
  int tt_value;
  Move tt_move = PASS_MOVE;
  if (tt_probe(key, depth, alpha, beta, tt_value, tt_move)) {
    best_move = tt_move;
    return tt_value;
  }

  // Null-move pruning: skip our turn, search with reduced depth.
  // If even skipping a turn, opponent can't catch up â†’ prune.
  static bool nullmove_ok = true; // forbid two null-moves in a row
  if (nullmove_on && depth >= 3 && !nullmove_ok)
    nullmove_ok = true;
  if (nullmove_on && depth >= 3 && nullmove_ok && alpha == beta - 1) {
    // Only try null-move at non-PV nodes (alpha == beta-1 means zero-width
    // window) Quick eval: if we're clearly losing, don't bother
    int static_eval = evaluate(board, is_first);
    if (static_eval >= beta) {
      MoveRecord record;
      board.make_move(PASS_MOVE, record);
      nullmove_ok = false;
      int score = -alpha_beta(board, depth - 4, -beta, -beta + 1, false, timer,
                              budget_ms, best_move, is_first);
      nullmove_ok = true;
      board.unmake_move(record);
      if (score >= beta) {
        best_move = PASS_MOVE;
        return score; // prune
      }
    }
  }

  auto moves = board.legal_moves();
  if (moves.empty()) {
    MoveRecord record;
    board.make_move(PASS_MOVE, record);
    int score = -alpha_beta(board, depth - 1, -beta, -alpha, false, timer,
                            budget_ms, best_move, is_first);
    board.unmake_move(record);
    return score;
  }

  order_moves(moves, tt_move, depth);

  int best_value = -999999;
  Move local_best = moves[0];
  uint8_t flag = TTFlag::UPPER_BOUND;

  for (const auto &move : moves) {
    MoveRecord record;
    board.make_move(move, record);

    int score = -alpha_beta(board, depth - 1, -beta, -alpha, false, timer,
                            budget_ms, best_move, is_first);

    board.unmake_move(record);

    if (score > best_value) {
      best_value = score;
      local_best = move;
    }
    if (score > alpha) {
      alpha = score;
      flag = TTFlag::EXACT;
    }
    if (alpha >= beta) {
      flag = TTFlag::LOWER_BOUND;
      // Killer/history: record move that caused cutoff
      if (killer_history_on && depth < 64 && !move.is_pass()) {
        uint32_t pm = pack_move(move);
        if (pm != killer1[depth]) {
          killer2[depth] = killer1[depth];
          killer1[depth] = pm;
        }
        ++history_score[move.r2][move.c2];
      }
      break;
    }
  }

  // PASS search heuristic: 3 modes via PASS_MODE env
  // 0=current: depth>=3 && moves<=5 && margin>0 (fixed: was static_eval>0)
  // 1=no-gate: depth>=3 && moves<=5 (let pass_score decide)
  // 2=none: only forced pass (moves.empty())
  if (pass_mode != 2 && depth >= 3 && static_cast<int>(moves.size()) <= 5) {
    ++pass_nodes;
    bool try_pass = (pass_mode == 1); // no-gate: always try
    if (!try_pass) {
      if (pass_fix) {
        // Rule #7b: only PASS when ahead in actual territory.
        int p = board.player();
        int opp = opponent(p);
        int margin = board.owned_cells(p) - board.owned_cells(opp);
        try_pass = (margin > 0);
      } else {
        // Baseline: PASS if eval thinks we're ahead
        int static_eval = evaluate(board, is_first);
        try_pass = (static_eval > 0);
      }
    }
    if (try_pass) {
      MoveRecord record;
      board.make_move(PASS_MOVE, record);
      int pass_score = -alpha_beta(board, depth - 1, -beta, -alpha, false,
                                   timer, budget_ms, best_move, is_first);
      board.unmake_move(record);

      if (pass_score > best_value) {
        best_value = pass_score;
        local_best = PASS_MOVE;
        flag = TTFlag::EXACT;
        ++pass_chosen;
      }
    }
  }

  best_move = local_best;
  tt_store(key, depth, best_value, flag, local_best);
  return best_value;
}

// === Iterative Deepening with Aspiration Windows ===
Move search_best_move(const Board &board, int time_budget_ms, bool is_first) {
  Timer timer;
  timer.start();
  nodes_searched = 0;
  ensure_tt_ready();

  // Aspiration instrument counters (reset each search)
  int asp_fail_high = 0;
  int asp_fail_low = 0;
  int full_researches = 0;
  int depth_reached = 0;
  int asp_window = 50; // default

  // PASS instrument (reset each search)
  pass_nodes = 0;
  pass_chosen = 0;

  // Killer/history toggle: default ON. Missing env must keep p1a-killer
  // behavior.
  {
    const char *kh = std::getenv("KILLER_HISTORY");
    if (kh)
      killer_history_on = (std::atoi(kh) > 0) ? 1 : 0;
  }
  // Null-move toggle
  {
    const char *nm = std::getenv("NULLMOVE");
    nullmove_on = (nm && std::atoi(nm) > 0) ? 1 : 0;
  }
  // Reset killer arrays for new search
  if (killer_history_on) {
    for (int i = 0; i < 64; ++i)
      killer1[i] = killer2[i] = 0;
  }

  // Read ASP_WINDOW env: "0"=full window, "200"=fixed, "adaptive"=dynamic,
  // else=default 50
  {
    const char *aw = std::getenv("ASP_WINDOW");
    if (aw) {
      std::string aws(aw);
      if (aws == "0" || aws == "full")
        asp_window = 0;
      else if (aws == "adaptive")
        asp_window = -1;
      else
        asp_window = std::atoi(aw);
      if (asp_window < -1)
        asp_window = 50;
    }
  }

  // Read PASS_MODE env: "current" (default), "no-gate" (remove static_eval
  // check), "none" (disable PASS heuristic)
  pass_mode = 0;
  {
    const char *pm = std::getenv("PASS_MODE");
    if (pm) {
      std::string pms(pm);
      if (pms == "no-gate")
        pass_mode = 1;
      else if (pms == "none")
        pass_mode = 2;
    }
  }

  // PASS_FIX env: 0=baseline (static_eval>0), 1=fix (margin>0). Default=0 (p1a
  // vanilla).
  {
    const char *pf = std::getenv("PASS_FIX");
    if (pf)
      pass_fix = std::atoi(pf);
    else
      pass_fix = 0;
  }

  // SECOND_STEAL env: 0=baseline, 1=boost steal-heavy moves for SECOND.
  // Default=0.
  static int second_steal = 0;
  {
    const char *ss = std::getenv("SECOND_STEAL");
    if (ss)
      second_steal = std::atoi(ss);
  }

  // P0.3: Age-based TT â€” increment generation instead of O(N) tt_clear()
  ++tt_age; // wraps naturally on uint8_t overflow

  // Phase 10: Copy board once at entry â€” alpha_beta uses
  // make_move/unmake_move on the copy
  Board work_board = board;

  auto moves = work_board.legal_moves();
  if (moves.empty())
    return PASS_MOVE;

  order_moves(moves, PASS_MOVE, 0);

  // Action 2b: SECOND_STEAL â€” boost steal-heavy moves when SECOND.
  // Count opponent-owned cells adjacent to live mushrooms in move rectangle.
  // Higher recapture potential = earlier search â†’ better alpha-beta pruning.
  if (second_steal && !is_first) {
    const auto &owners = work_board.owners();
    const auto &values = work_board.values();
    int player = work_board.player();
    int opp = opponent(player);
    for (auto &m : moves) {
      int steal = 0;
      for (int r = m.r1; r <= m.r2; ++r) {
        for (int c = m.c1; c <= m.c2; ++c) {
          if (owners[r][c] != opp)
            continue;
          // Check adjacent to live mushroom (recapturable)
          bool adj = false;
          if (r > 0 && values[r - 1][c] > 0)
            adj = true;
          else if (r < ROWS - 1 && values[r + 1][c] > 0)
            adj = true;
          else if (c > 0 && values[r][c - 1] > 0)
            adj = true;
          else if (c < COLS - 1 && values[r][c + 1] > 0)
            adj = true;
          if (adj)
            ++steal;
        }
      }
      m.priority += steal * 30; // boost recapture moves
    }
    order_moves(moves, PASS_MOVE, 0); // re-sort with steal bonus
  }

  Move best_move = moves[0];
  int max_depth = 12;

  int64_t budget = std::max<int64_t>(1, time_budget_ms * 80 / 100);

  // Endgame: flat depth-8 full-window search for positions with <=5 moves.
  // Falls through to iterative deepening if timeout.
  // NOTE: Reverted PASS guard (2026-06-15) â€” blocking endgame PASS caused
  // -16% regression. The search correctly evaluates PASS value. DO NOT add
  // PASS-blocking logic here. (DO NOT TRY #14: PASS_FIX margin>0 = -10% alone.
  // Endgame version = -16%.)
  if (static_cast<int>(moves.size()) <= 5) {
    int endgame_depth = 8;
    Move endgame_best = best_move;
    alpha_beta(work_board, endgame_depth, -999999, 999999, true, timer, budget,
               endgame_best, is_first);
    if (!timer.timed_out(budget)) {
      return endgame_best;
    }
    best_move = endgame_best;
  }

  try {
    int alpha = -999999;
    int beta = 999999;
    int prev_score = 0;
    bool first_iteration = true;

    for (int depth = 1; depth <= max_depth; ++depth) {
      depth_reached = depth;
      Timer depth_timer;
      depth_timer.start();

      Move depth_best = best_move;
      int score = prev_score;

      if (first_iteration || asp_window == 0) {
        // Full window: depth 1, or ASP_WINDOW=0 (remove aspiration)
        score = alpha_beta(work_board, depth, alpha, beta, true, timer, budget,
                           depth_best, is_first);
        first_iteration = false;
      } else {
        int w = (asp_window == -1) ? std::max(200, std::abs(prev_score) / 4)
                                   : asp_window;
        alpha = prev_score - w;
        beta = prev_score + w;

        score = alpha_beta(work_board, depth, alpha, beta, true, timer, budget,
                           depth_best, is_first);

        if (score <= alpha || score >= beta) {
          if (score <= alpha)
            ++asp_fail_low;
          if (score >= beta)
            ++asp_fail_high;
          ++full_researches;
          score = alpha_beta(work_board, depth, -999999, 999999, true, timer,
                             budget, depth_best, is_first);
        }
      }

      if (!timer.timed_out(budget)) {
        best_move = depth_best;
        prev_score = score;
      }

      if (timer.timed_out(budget) || depth_timer.elapsed_ms() > budget / 2) {
        break;
      }
    }
  } catch (...) {
  }

  // NOTE: Safety net REMOVED (2026-06-15) â€” overriding search-decided PASS
  // caused -16% regression. The alpha-beta search correctly evaluates PASS
  // value for endgame. Reverted to p1a-finding-v1 behavior. DO NOT TRY #14,
  // #27.
  return best_move;
}

// ====== From: protocol.cpp ======
#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <sstream>
#include <string>


int Protocol::run() {
  std::ios::sync_with_stdio(false);
  std::cin.tie(nullptr);

  while (running_) {
    std::string line = read_line();
    if (line.empty()) {
      break; // EOF
    }

    if (line.starts_with("READY")) {
      handle_ready(line);
    } else if (line.starts_with("INIT")) {
      handle_init(line);
    } else if (line.starts_with("TIME")) {
      handle_time(line);
    } else if (line.starts_with("OPP")) {
      handle_opp(line);
    } else if (line.starts_with("FINISH")) {
      log_shadow_metrics();
      running_ = false;
    }
  }

  return 0;
}

// --- Private helpers ---

std::string Protocol::read_line() {
  std::string line;
  if (!std::getline(std::cin, line)) {
    return ""; // EOF
  }
  // Trim trailing \r (Windows line endings)
  if (!line.empty() && line.back() == '\r') {
    line.pop_back();
  }
  return line;
}

void Protocol::write_line(const std::string &line) {
  std::cout << line << '\n';
  std::cout.flush();
}

void Protocol::handle_ready(const std::string &line) {
  if (line.find("FIRST") != std::string::npos) {
    i_am_first_ = true;
    board_.set_player(FIRST_PLAYER);
  } else {
    i_am_first_ = false;
    board_.set_player(SECOND_PLAYER);
  }
  write_line("OK");
}

void Protocol::handle_init(const std::string &line) {
  // Format: "INIT row1 row2 ... row10"
  std::string board_str = line.substr(5); // Skip "INIT "
  board_.init_from_string(board_str);
  opp_consecutive_passes_ = 0; // P0b: reset counter on new game
  ensure_tt_ready(); // Allocate large TT after READY, before timed moves.

  // QR data.bin: reset fingerprint state per game
  opp_fp_ = OpponentFingerprint{};
  move_counter_ = 0;
  opp_move_counter_ = 0;
  ply_counter_ = 0;
  matched_style_ = KnownStyle::UNKNOWN;
  match_confidence_ = 0.0f;
  fingerprint_checked_ = false;
}

void Protocol::handle_time(const std::string &line) {
  // Format: "TIME our_remaining opp_remaining"
  std::istringstream iss(line);
  std::string cmd;
  int our_time = 0, opp_time = 0;
  if (!(iss >> cmd >> our_time >> opp_time)) {
    write_line("-1 -1 -1 -1");
    board_.apply_pass();
    return;
  }

  // Reset: opponent's passes since our last move (for Always Pass artifact
  // detection)
  opp_passes_since_our_move_ = 0;

  // Phase 8a.1: Dynamic time budget based on estimated remaining moves
  // Count live mushrooms to estimate game length
  int live = 0;
  for (int r = 0; r < ROWS; ++r)
    for (int c = 0; c < COLS; ++c)
      if (board_.values()[r][c] > 0)
        ++live;
  int est_moves_left = std::max(4, live / 4);
  int time_budget = our_time / est_moves_left;
  if (time_budget < 20)
    time_budget = 20; // Minimum 20ms
  if (time_budget > 2500)
    time_budget = 2500; // P0b-rescue-v1: restore P0b search budget

  // Edge: TIME after terminal. Only auto-PASS if WE passed last.
  // If opp_consecutive_passes_ >= 2, opponent doubled but we didn't pass â†’
  // play.
  if (board_.consecutive_passes() >= 2 && opp_consecutive_passes_ < 2) {
    write_line("-1 -1 -1 -1");
    return;
  }

  // P0b: If opponent passed twice and we're winning â†’ lock in win.
  // If losing/tied â†’ keep fighting.
  if (opp_consecutive_passes_ >= 2) {
    int opp = opponent(board_.player());
    int margin = board_.owned_cells(board_.player()) - board_.owned_cells(opp);
    if (margin > 0) {
      write_line("-1 -1 -1 -1");
      board_.apply_pass();
      return;
    }
  }

  // QR data.bin: move counter
  ++move_counter_;
  ++ply_counter_;

  Move best = search_best_move(board_, time_budget, i_am_first_);
  if (!best.is_pass() && !board_.is_legal_move(best)) {
    auto fallback = board_.legal_moves();
    best = fallback.empty() ? PASS_MOVE : fallback[0];
  }

  if (best.is_pass()) {
    write_line("-1 -1 -1 -1");
  } else {
    write_line(std::to_string(best.r1) + " " + std::to_string(best.c1) + " " +
               std::to_string(best.r2) + " " + std::to_string(best.c2));
  }
  board_.apply_move(best);
}

void Protocol::log_shadow_metrics() {
  // QR data.bin: write passive fingerprint data for offline analysis
  // Only when NOT in ONLINE_JUDGE mode
#ifndef ONLINE_JUDGE
  static int game_count = 0;
  ++game_count;
  auto fv = opp_fp_.to_feature_vector();
  std::string side = i_am_first_ ? "FIRST" : "SECOND";
  std::string style = "UNKNOWN";
  switch (matched_style_) {
  case KnownStyle::CORDYCEPS_ATTACK:
    style = "CORDYCEPS_ATTACK";
    break;
  case KnownStyle::CORDYCEPS_DEFENSE:
    style = "CORDYCEPS_DEFENSE";
    break;
  case KnownStyle::CORDYCEPS_BALANCED:
    style = "CORDYCEPS_BALANCED";
    break;
  case KnownStyle::RUST_OLD:
    style = "RUST_OLD";
    break;
  case KnownStyle::RUST_UPDATE:
    style = "RUST_UPDATE";
    break;
  default:
    break;
  }
  FILE *f = std::fopen("fingerprint_shadow.log", "a");
  if (f) {
    std::fprintf(f,
                 "G%d %s moves=%d opp_moves=%d matched=%s conf=%.2f "
                 "fv=[%d,%d,%d,%d,%d,%d,%d,%d] ply=%d\n",
                 game_count, side.c_str(), move_counter_, opp_move_counter_,
                 style.c_str(), match_confidence_, fv.dim[0], fv.dim[1],
                 fv.dim[2], fv.dim[3], fv.dim[4], fv.dim[5], fv.dim[6],
                 fv.dim[7], ply_counter_);
    std::fclose(f);
  }
#endif
}

void Protocol::handle_opp(const std::string &line) {
  // Format: "OPP r1 c1 r2 c2 time_ms"
  std::istringstream iss(line);
  std::string cmd;
  int r1 = -1, c1 = -1, r2 = -1, c2 = -1, t = 0;
  if (!(iss >> cmd >> r1 >> c1 >> r2 >> c2 >> t)) {
    return;
  }

  Move opp_move{r1, c1, r2, c2};

  // P0b: track opponent consecutive passes
  if (opp_move.is_pass()) {
    ++opp_consecutive_passes_;
    ++opp_passes_since_our_move_;
    // Only apply pass to board state if first opp pass since our move.
    // Second+ passes before our TIME = Always Pass artifact â†’ skip board
    // update. Otherwise board gets cp=2 and guard/search treat it as game-over.
    if (opp_passes_since_our_move_ <= 1) {
      board_.apply_move(opp_move);
    }
  } else {
    // QR data.bin: measure steal BEFORE apply (opp cells we owned)
    int steal_before = 0;
    for (int r = opp_move.r1; r <= opp_move.r2; ++r)
      for (int c = opp_move.c1; c <= opp_move.c2; ++c)
        if (board_.owners()[r][c] == board_.player())
          ++steal_before;

    opp_consecutive_passes_ = 0;
    opp_passes_since_our_move_ = 0;
    board_.apply_move(opp_move);

    // Track steal after apply (steal_before was measured pre-apply)
    if (steal_before > 0)
      opp_fp_.steal_seen++;
  }

  // QR data.bin: passive fingerprint tracking (opponent moves only)
  ++ply_counter_;
  if (!opp_move.is_pass()) {
    ++opp_move_counter_;
    opp_fp_.move_count = opp_move_counter_;
    opp_fp_.side_ply = ply_counter_;
    opp_fp_.we_are_first = i_am_first_;

    int area = move_area(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
    opp_fp_.total_area += area;
    ShapeClass sc =
        classify_shape(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
    opp_fp_.shape_counts[static_cast<int>(sc)]++;
    if (area >= 5 && area <= 10)
      opp_fp_.medium_count++;
    if (area >= 11)
      opp_fp_.large_count++;

    Orientation orient = classify_orientation(opp_move.r1, opp_move.c1,
                                              opp_move.r2, opp_move.c2);
    if (orient == Orientation::PORTRAIT)
      opp_fp_.tall_count++;
    if (orient == Orientation::LANDSCAPE)
      opp_fp_.wide_count++;

    RegionTag rt =
        classify_region(opp_move.r1, opp_move.c1, opp_move.r2, opp_move.c2);
    if (rt != RegionTag::NONE)
      opp_fp_.region_counts[static_cast<int>(rt)]++;

    // Steal already counted pre-apply (see handle_opp else block above)

    // Barrier detection
    if (board_.barrier_potential(opp_move.r1, opp_move.c1, opp_move.r2,
                                 opp_move.c2) > 0)
      opp_fp_.barrier_freq++;
  } else {
    opp_fp_.pass_seen++;
    if (opp_fp_.first_pass_ply == 0)
      opp_fp_.first_pass_ply = ply_counter_;
  }

  // Try to match known fingerprint after sufficient observations
  if (!fingerprint_checked_ && opp_move_counter_ >= 5) {
    auto fps = g_opponent_db.fingerprints();
    float conf = 0.0f, margin = 0.0f;
    KnownStyle style = opp_fp_.match_fingerprint(
        fps.data(), static_cast<int>(fps.size()), conf, margin);
    if (style != KnownStyle::UNKNOWN && conf >= 0.50f) {
      matched_style_ = style;
      match_confidence_ = conf;
      fingerprint_checked_ = true;

      // Select prior config based on matched fingerprint
      for (const auto &fp : fps) {
        if (fp.style == style) {
          const auto *cfg = g_opponent_db.get_prior_config(fp.prior_config_id);
          if (cfg)
            g_active_prior_config = cfg;
          break;
        }
      }
    }
  }
}

// ====== From: opponent_db.cpp ======
#include <cstdio>
#include <cstring>

OpponentDB g_opponent_db;

// Default prior config (used when no data.bin loaded)
static const MovePriorConfig kDefaultPriorConfig = {
    .shape_boost = {0, 0, 0, 0, 0, 0, 0, 0},
    .medium_rect_boost = 0,
    .barrier_boost = 0,
    .connection_boost = 0,
    .dead_cell_risk_penalty = 0,
    .side_boost_FIRST = 0,
    .side_boost_SECOND = 0,
    .max_total_adjustment = 3000,
    .confidence_min = 60};

const MovePriorConfig *g_active_prior_config =
    nullptr; // nullptr = NO overhead when no data.bin

// === FeatureVector8 from OpponentFingerprint ===
FeatureVector8 OpponentFingerprint::to_feature_vector() const {
  FeatureVector8 fv = {};
  if (move_count == 0)
    return fv;

  int n = move_count;
  int total = total_area;
  int medium = medium_count;
  int large = large_count;
  int tall = tall_count;
  int wide = wide_count;

  // Q8.7: value * 128
  fv.dim[0] = static_cast<int16_t>((total * 128) / n);        // avg_area
  fv.dim[1] = static_cast<int16_t>((medium * 128) / n);       // medium_ratio
  fv.dim[2] = static_cast<int16_t>((large * 128) / n);        // large_ratio
  fv.dim[3] = static_cast<int16_t>((tall * 128) / n);         // portrait_ratio
  fv.dim[4] = static_cast<int16_t>((wide * 128) / n);         // landscape_ratio
  fv.dim[5] = static_cast<int16_t>((steal_seen * 128) / n);   // steal_ratio
  fv.dim[6] = static_cast<int16_t>((pass_seen * 128) / n);    // pass_ratio
  fv.dim[7] = static_cast<int16_t>((barrier_freq * 128) / n); // barrier_ratio

  return fv;
}

// === Fingerprint matching (nearest-neighbor with variance weighting) ===
KnownStyle OpponentFingerprint::match_fingerprint(const KnownFingerprint *fps,
                                                  int count,
                                                  float &out_confidence,
                                                  float &out_margin) const {
  out_confidence = 0.0f;
  out_margin = 0.0f;
  if (count == 0 || move_count == 0)
    return KnownStyle::UNKNOWN;

  FeatureVector8 fv = to_feature_vector();
  int best_idx = -1;
  float best_dist = 1e30f;
  int second_idx = -1;
  float second_dist = 1e30f;
  (void)second_idx; // may be unused if no second candidate

  for (int i = 0; i < count; ++i) {
    // Check side mask
    bool first_ok = (fps[i].side_mask & 1) && we_are_first;
    bool second_ok = (fps[i].side_mask & 2) && !we_are_first;
    if (!first_ok && !second_ok)
      continue;

    // Check min moves
    if (move_count < fps[i].min_moves)
      continue;

    // Weighted Euclidean distance
    float dist = 0.0f;
    for (int d = 0; d < 8; ++d) {
      float diff = static_cast<float>(fv.dim[d] - fps[i].mean.dim[d]);
      float inv_var = static_cast<float>(fps[i].var.dim[d]);
      if (inv_var > 0.0f)
        diff /= (inv_var / 128.0f);
      dist += diff * diff;
    }

    if (dist < best_dist) {
      second_dist = best_dist;
      second_idx = best_idx;
      best_dist = dist;
      best_idx = i;
    } else if (dist < second_dist) {
      second_dist = dist;
      second_idx = i;
    }
  }

  if (best_idx < 0)
    return KnownStyle::UNKNOWN;

  // Confidence = 1.0 - (best_dist / second_dist)
  if (second_dist < 1e29f && second_dist > 0.0f) {
    out_confidence = 1.0f - (best_dist / second_dist);
  } else {
    out_confidence = 1.0f; // only one match
  }
  out_margin = second_dist - best_dist;

  // Apply confidence threshold
  if (out_confidence * 100.0f < fps[best_idx].confidence_threshold) {
    return KnownStyle::UNKNOWN;
  }

  return fps[best_idx].style;
}

// Simple CRC32 (for verification)
static uint32_t crc32_simple(const uint8_t *data, size_t len) {
  uint32_t crc = 0xFFFFFFFF;
  static const uint32_t table[16] = {
      0x00000000, 0x1DB71064, 0x3B6E20C8, 0x26D930AC, 0x76DC4190, 0x6B6B51F4,
      0x4DB26158, 0x5005713C, 0xEDB88320, 0xF00F9344, 0xD6D6A3E8, 0xCB61B38C,
      0x9B64C2B0, 0x86D392D4, 0xA00AE278, 0xBDBDF21C};
  for (size_t i = 0; i < len; ++i) {
    crc ^= data[i];
    crc = table[crc & 0xF] ^ (crc >> 4);
    crc = table[crc & 0xF] ^ (crc >> 4);
  }
  return ~crc;
}

bool OpponentDB::load(const uint8_t *data, size_t len) {
  if (!data || len < sizeof(DBHeader) + sizeof(DBFooter)) {
    std::fprintf(stderr, "OpponentDB: data too small (%zu bytes)\n", len);
    return false;
  }

  data_ = data;
  data_size_ = len;

  // Parse header
  std::memcpy(&header_, data_, sizeof(DBHeader));
  if (header_.magic != DB_MAGIC || header_.version != DB_VERSION) {
    std::fprintf(stderr, "OpponentDB: bad magic/version (magic=0x%X, ver=%u)\n",
                 header_.magic, header_.version);
    loaded_ = false;
    return false;
  }

  // Verify CRC (covers all data past header up to footer)
  if (!validate_crc32()) {
    std::fprintf(stderr, "OpponentDB: CRC mismatch\n");
    loaded_ = false;
    return false;
  }

  // Parse sections
  if (!parse_sections()) {
    loaded_ = false;
    return false;
  }

  loaded_ = true;
  return true;
}

bool OpponentDB::load_from_file(const char *path) {
  FILE *f = std::fopen(path, "rb");
  if (!f) {
    std::fprintf(stderr, "OpponentDB: cannot open %s\n", path);
    return false;
  }

  std::fseek(f, 0, SEEK_END);
  long sz = std::ftell(f);
  std::fseek(f, 0, SEEK_SET);
  if (sz <= 0) {
    std::fclose(f);
    return false;
  }

  // Allocate + read (small data, heap is fine)
  auto *buf = new uint8_t[static_cast<size_t>(sz)];
  size_t read_sz = std::fread(buf, 1, static_cast<size_t>(sz), f);
  std::fclose(f);

  if (read_sz != static_cast<size_t>(sz)) {
    delete[] buf;
    return false;
  }

  bool ok = load(buf, static_cast<size_t>(sz));
  // Note: buf owned by caller now; in this simple impl we leak if used
  // For QR: data is tiny and persists for program lifetime â€” OK
  // In practice, embed data.bin as static const array
  return ok;
}

bool OpponentDB::validate_crc32() const {
  // CRC covers bytes from end of header to start of footer
  size_t crc_start = sizeof(DBHeader);
  if (crc_start >= data_size_ - sizeof(DBFooter))
    return false;
  size_t crc_len = data_size_ - crc_start - sizeof(DBFooter);
  uint32_t expected = header_.crc32;
  uint32_t actual = crc32_simple(data_ + crc_start, crc_len);
  return expected == actual;
}

bool OpponentDB::parse_sections() {
  fingerprint_count_ = 0;
  prior_config_count_ = 0;

  size_t offset = sizeof(DBHeader);
  for (uint32_t s = 0; s < header_.section_count; ++s) {
    if (offset + sizeof(DBSection) > data_size_ - sizeof(DBFooter))
      return false;

    DBSection sec;
    std::memcpy(&sec, data_ + offset, sizeof(DBSection));
    offset += sizeof(DBSection);

    if (offset + sec.data_size > data_size_ - sizeof(DBFooter))
      return false;

    const uint8_t *sec_data = data_ + offset;

    switch (sec.type) {
    case DB_SECTION_FINGERPRINTS: {
      size_t fp_size = sizeof(KnownFingerprint);
      int count = static_cast<int>(sec.data_size / fp_size);
      if (count > MAX_FINGERPRINTS)
        count = MAX_FINGERPRINTS;
      for (int i = 0; i < count; ++i) {
        KnownFingerprint fp;
        std::memcpy(&fp, sec_data + i * fp_size, fp_size);
        fingerprints_[fingerprint_count_++] = fp;
      }
      break;
    }
    case DB_SECTION_PRIOR_CONFIGS: {
      // Format: each entry is (uint32_t id, MovePriorConfig)
      // Parsed directly from binary
      size_t entry_size = sizeof(uint32_t) + sizeof(MovePriorConfig);
      int count = static_cast<int>(sec.data_size / entry_size);
      if (count > MAX_PRIOR_CONFIGS)
        count = MAX_PRIOR_CONFIGS;
      for (int i = 0; i < count; ++i) {
        uint32_t id;
        std::memcpy(&id, sec_data + i * entry_size, sizeof(uint32_t));
        MovePriorConfig cfg;
        std::memcpy(&cfg, sec_data + i * entry_size + sizeof(uint32_t),
                    sizeof(MovePriorConfig));
        prior_configs_[prior_config_count_++] = {id, cfg};
      }
      break;
    }
    case DB_SECTION_CENTROIDS:
    case DB_SECTION_METADATA:
      // Reserved for future use
      break;
    }
    offset += sec.data_size;
  }

  return true;
}

const MovePriorConfig *OpponentDB::get_prior_config(uint32_t id) const {
  for (int i = 0; i < prior_config_count_; ++i) {
    if (prior_configs_[i].id == id)
      return &prior_configs_[i].config;
  }
  return nullptr;
}

const MovePriorConfig *OpponentDB::default_prior_config() const {
  if (prior_config_count_ > 0)
    return &prior_configs_[0].config;
  return nullptr;
}

// ====== From: main.cpp ======

// Embedded data.bin (from merge_submission.py --databin)
// Defined in merged submission; zero-size if not embedded.
#ifdef EMBEDDED_DATA_BIN
extern const unsigned char kEmbeddedDataBin[];
extern const size_t kEmbeddedDataBinSize;
#else
static const unsigned char kEmbeddedDataBin[1] = {0};
static const size_t kEmbeddedDataBinSize = 0;
#endif

int main() {
  // QR data.bin: try embedded binary first, then file (optional, fallback if
  // missing)
  if (kEmbeddedDataBinSize > 0) {
    g_opponent_db.load(kEmbeddedDataBin, kEmbeddedDataBinSize);
  } else {
    g_opponent_db.load_from_file("data.bin");
  }

  Protocol protocol;
  return protocol.run();
}
