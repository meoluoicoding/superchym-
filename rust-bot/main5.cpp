#include <string>
#include <cstdint>
#include <array>
#include <bit>
#include <vector>
#include <span>
#include <cstdio>
#include <chrono>
#include <cstring>
#include <random>
#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <sstream>




namespace cordyceps {

constexpr int k_rows = 10;
constexpr int k_cols = 17;
constexpr int k_cells = k_rows * k_cols; // 170
constexpr int k_target_sum = 10;
constexpr int k_num_rects = 8415; // all possible rects on 10x17 grid

constexpr int k_player_us = 1;
constexpr int k_player_opp = -1;
constexpr int k_no_owner = 0;

struct Move {
    std::int8_t r1;
    std::int8_t c1;
    std::int8_t r2;
    std::int8_t c2;
    int score_hint{0};  // temporary sort key (set by enhance_root_moves)

    [[nodiscard]] bool is_pass() const noexcept { return r1 == -1; }

    bool operator==(const Move& other) const noexcept {
        return r1 == other.r1 && c1 == other.c1 
            && r2 == other.r2 && c2 == other.c2;
    }

    bool operator!=(const Move& other) const noexcept {
        return !(*this == other);
    }
};

constexpr Move k_pass_move{-1, -1, -1, -1};

enum class GamePhase { kOpening, kMidgame, kLate, kEndgame };

struct SideConfig {
    float time_multiplier;
    float aggression;
    float steal_bonus;
    float defense_bonus;
    bool  prefer_vertical;
};

} // namespace cordyceps




namespace cordyceps {

// 3 × uint64_t covers 170 cells (uses 170/192 bits)
struct Bitboard {
    std::uint64_t lo;   // cells 0-63
    std::uint64_t mid;  // cells 64-127
    std::uint64_t hi;   // cells 128-169 (42 bits used)

    static constexpr Bitboard empty() noexcept { return {0, 0, 0}; }

    [[nodiscard]] int popcount() const noexcept;

    void set(int idx) noexcept;
    void clear(int idx) noexcept;
    [[nodiscard]] bool test(int idx) const noexcept;

    Bitboard& operator&=(const Bitboard& other) noexcept;
    Bitboard& operator|=(const Bitboard& other) noexcept;
    Bitboard& operator^=(const Bitboard& other) noexcept;

    friend Bitboard operator&(Bitboard a, const Bitboard& b) noexcept { a &= b; return a; }
    friend Bitboard operator|(Bitboard a, const Bitboard& b) noexcept { a |= b; return a; }
    friend Bitboard operator^(Bitboard a, const Bitboard& b) noexcept { a ^= b; return a; }

    [[nodiscard]] bool is_empty() const noexcept;
};

} // namespace cordyceps


namespace cordyceps {

int Bitboard::popcount() const noexcept {
    return std::popcount(lo) + std::popcount(mid) + std::popcount(hi);
}

void Bitboard::set(int idx) noexcept {
    if (idx < 64)       lo |= (1ULL << idx);
    else if (idx < 128) mid |= (1ULL << (idx - 64));
    else                hi  |= (1ULL << (idx - 128));
}

void Bitboard::clear(int idx) noexcept {
    if (idx < 64)       lo &= ~(1ULL << idx);
    else if (idx < 128) mid &= ~(1ULL << (idx - 64));
    else                hi  &= ~(1ULL << (idx - 128));
}

bool Bitboard::test(int idx) const noexcept {
    if (idx < 64)       return (lo >> idx) & 1ULL;
    else if (idx < 128) return (mid >> (idx - 64)) & 1ULL;
    else                return (hi >> (idx - 128)) & 1ULL;
}

Bitboard& Bitboard::operator&=(const Bitboard& other) noexcept {
    lo &= other.lo; mid &= other.mid; hi &= other.hi;
    return *this;
}

Bitboard& Bitboard::operator|=(const Bitboard& other) noexcept {
    lo |= other.lo; mid |= other.mid; hi |= other.hi;
    return *this;
}

Bitboard& Bitboard::operator^=(const Bitboard& other) noexcept {
    lo ^= other.lo; mid ^= other.mid; hi ^= other.hi;
    return *this;
}

bool Bitboard::is_empty() const noexcept {
    return (lo | mid | hi) == 0;
}

} // namespace cordyceps

namespace cordyceps {

struct EvalCache {
    int my_territory{0};
    int opp_territory{0};
    int my_corners{0};
    int opp_corners{0};
    int my_edges{0};
    int opp_edges{0};
    int live_adj_my{0};
    int live_adj_opp{0};
    int connectivity_my{0};
    int connectivity_opp{0};
};

static_assert(sizeof(EvalCache) == 40, "EvalCache must be 40 bytes");

struct UndoMove {
    Move mv;
    int  changed_count{0};
    std::array<std::uint8_t, k_cells> changed_indices;
    std::array<std::int8_t, k_cells> old_values;
    Bitboard old_my_mask;
    Bitboard old_opp_mask;
    Bitboard old_live_mask;
    int old_live_count{0};
    int old_my_score{0};
    int old_opp_score{0};
    int old_consecutive_passes{0};
    int old_current_player{0};
    EvalCache old_eval_cache;
};

struct Board {
    // Grid data
    std::array<std::int8_t, k_cells> values{};   // 0=empty, 1-9=mushroom
    std::array<std::int8_t, k_cells> owners{};   // 0=none, 1=us, -1=opp

    // Bitboards
    Bitboard my_mask;
    Bitboard opp_mask;
    Bitboard live_mask;

    // Game state
    int my_score{0};
    int opp_score{0};
    int live_count{0};
    int current_player{0};          // 1=Cordyceps, -1=opponent
    int consecutive_passes{0};

    // Eval cache
    EvalCache eval_cache;

    // Cell access helpers
    [[nodiscard]] std::int8_t& value_at(int r, int c) noexcept { 
        if (r < 0 || r >= k_rows || c < 0 || c >= k_cols) return values[0];
        return values[r * k_cols + c]; 
    }
    [[nodiscard]] const std::int8_t& value_at(int r, int c) const noexcept { 
        if (r < 0 || r >= k_rows || c < 0 || c >= k_cols) return values[0];
        return values[r * k_cols + c]; 
    }
    [[nodiscard]] std::int8_t& owner_at(int r, int c) noexcept { 
        if (r < 0 || r >= k_rows || c < 0 || c >= k_cols) return owners[0];
        return owners[r * k_cols + c]; 
    }
    [[nodiscard]] const std::int8_t& owner_at(int r, int c) const noexcept { 
        if (r < 0 || r >= k_rows || c < 0 || c >= k_cols) return owners[0];
        return owners[r * k_cols + c]; 
    }

    // Operations
    void recalc_live_mask() noexcept;
    [[nodiscard]] UndoMove apply_move(const Move& mv) noexcept;
    void unmake_move(const UndoMove& undo) noexcept;
    [[nodiscard]] bool is_terminal() const noexcept;
    [[nodiscard]] int score_from_perspective(int player) const noexcept;

private:
    void save_old_state(UndoMove& undo) noexcept;
    void apply_move_on_board(const Move& mv, int player) noexcept;
};

} // namespace cordyceps

// Free function evaluate (side-agnostic)
namespace cordyceps {
[[nodiscard]] int evaluate(const Board& board, int player) noexcept;

// Runtime weight loading for tuning (thread-local, zero-overhead when not set)
void set_tune_weights(int score_w, int territory_w, int corner_w, int edge_w,
                      int adj_w, int recapture_w, int vulnerability_w) noexcept;
void clear_tune_weights() noexcept;
}


namespace cordyceps {

static bool is_corner_cell(int r, int c) noexcept {
    return (r == 0 || r == k_rows - 1) && (c == 0 || c == k_cols - 1);
}

static bool is_edge_cell(int r, int c) noexcept {
    return r == 0 || r == k_rows - 1 || c == 0 || c == k_cols - 1;
}

void Board::recalc_live_mask() noexcept {
    live_mask = Bitboard::empty();
    live_count = 0;
    for (int i = 0; i < k_cells; ++i) {
        if (values[i] > 0) {
            live_mask.set(i);
            ++live_count;
        }
    }
}

void Board::save_old_state(UndoMove& undo) noexcept {
    undo.old_my_mask = my_mask;
    undo.old_opp_mask = opp_mask;
    undo.old_live_mask = live_mask;
    undo.old_live_count = live_count;
    undo.old_my_score = my_score;
    undo.old_opp_score = opp_score;
    undo.old_consecutive_passes = consecutive_passes;
    undo.old_current_player = current_player;
    undo.old_eval_cache = eval_cache;
}

UndoMove Board::apply_move(const Move& mv) noexcept {
    UndoMove undo;
    undo.mv = mv;
    save_old_state(undo);

    if (mv.is_pass()) {
        ++consecutive_passes;
        current_player = -current_player;
        return undo;
    }

    consecutive_passes = 0;

    EvalCache* ec = &eval_cache;
    int player = current_player;

    const int r1 = mv.r1, c1 = mv.c1, r2 = mv.r2, c2 = mv.c2;
    int change_idx = 0;
    int sum_collected = 0;

    // Live adj: track stale neighbors before clearing cells
    for (int r = r1; r <= r2; ++r) {
        for (int c = c1; c <= c2; ++c) {
            const int idx = r * k_cols + c;
            if (idx < 0 || idx >= k_cells) continue; // safety
            if (values[idx] > 0) {
                undo.changed_indices[change_idx] = static_cast<std::uint8_t>(idx);
                undo.old_values[change_idx] = values[idx];
                sum_collected += values[idx];

                // Decrement adjacent live counts for both players
                int dr[] = {-1, 1, 0, 0};
                int dc[] = {0, 0, -1, 1};
                for (int d = 0; d < 4; ++d) {
                    int nr = r + dr[d], nc = c + dc[d];
                    if (nr >= 0 && nr < k_rows && nc >= 0 && nc < k_cols) {
                        int nidx = nr * k_cols + nc;
                        if (owners[nidx] == k_player_us) {
                            if (ec->live_adj_my > 0) --ec->live_adj_my;
                        } else if (owners[nidx] == k_player_opp) {
                            if (ec->live_adj_opp > 0) --ec->live_adj_opp;
                        }
                    }
                }

                values[idx] = 0;
                live_mask.clear(idx);
                --live_count;
                ++change_idx;
            }
        }
    }
    undo.changed_count = change_idx;

    // Update ownership and eval
    for (int i = 0; i < change_idx; ++i) {
        const int idx = undo.changed_indices[i];
        int r = idx / k_cols, c = idx % k_cols;

        owners[idx] = static_cast<std::int8_t>(player);

        // Update connectivity: count edges to adjacent same-owner cells
        {
            int adj_dr[] = {-1, 1, 0, 0};
            int adj_dc[] = {0, 0, -1, 1};
            for (int d = 0; d < 4; ++d) {
                int nr2 = r + adj_dr[d], nc2 = c + adj_dc[d];
                if (nr2 >= 0 && nr2 < k_rows && nc2 >= 0 && nc2 < k_cols) {
                    int nidx2 = nr2 * k_cols + nc2;
                    if (owners[nidx2] == player) {
                        if (player == k_player_us) ++ec->connectivity_my;
                        else ++ec->connectivity_opp;
                    }
                }
            }
        }

        if (player == k_player_us) {
            my_mask.set(idx);
            ++ec->my_territory;
            if (is_corner_cell(r, c)) ++ec->my_corners;
            if (is_edge_cell(r, c)) ++ec->my_edges;
            my_score += undo.old_values[i];

            // Add adjacent live cells for my side
            int dr[] = {-1, 1, 0, 0};
            int dc[] = {0, 0, -1, 1};
            for (int d = 0; d < 4; ++d) {
                int nr = r + dr[d], nc = c + dc[d];
                if (nr >= 0 && nr < k_rows && nc >= 0 && nc < k_cols) {
                    int nidx = nr * k_cols + nc;
                    if (values[nidx] > 0 && owners[nidx] == k_no_owner)
                        ++ec->live_adj_my;
                }
            }
        } else {
            opp_mask.set(idx);
            ++ec->opp_territory;
            if (is_corner_cell(r, c)) ++ec->opp_corners;
            if (is_edge_cell(r, c)) ++ec->opp_edges;
            opp_score += undo.old_values[i];

            int dr[] = {-1, 1, 0, 0};
            int dc[] = {0, 0, -1, 1};
            for (int d = 0; d < 4; ++d) {
                int nr = r + dr[d], nc = c + dc[d];
                if (nr >= 0 && nr < k_rows && nc >= 0 && nc < k_cols) {
                    int nidx = nr * k_cols + nc;
                    if (values[nidx] > 0 && owners[nidx] == k_no_owner)
                        ++ec->live_adj_opp;
                }
            }
        }
    }

    current_player = -current_player;
    return undo;
}

void Board::unmake_move(const UndoMove& undo) noexcept {
    if (!undo.mv.is_pass()) {
        for (int i = 0; i < undo.changed_count; ++i) {
            const int idx = undo.changed_indices[i];
            values[idx] = undo.old_values[i];
            owners[idx] = k_no_owner;
        }
    }

    my_mask = undo.old_my_mask;
    opp_mask = undo.old_opp_mask;
    live_mask = undo.old_live_mask;
    live_count = undo.old_live_count;
    my_score = undo.old_my_score;
    opp_score = undo.old_opp_score;
    consecutive_passes = undo.old_consecutive_passes;
    current_player = undo.old_current_player;
    eval_cache = undo.old_eval_cache;
}

bool Board::is_terminal() const noexcept {
    return consecutive_passes >= 2;
}

int Board::score_from_perspective(int player) const noexcept {
    if (player == k_player_us) return my_score - opp_score;
    return opp_score - my_score;
}

// ===== Runtime weight loading for tuning =====
// Thread-local: zero overhead when not in tune mode
// Order: score, territory, corners, edges, live_adj, recapture, vulnerability
static thread_local int g_tune_w0 = 0;
static thread_local int g_tune_w1 = 0;
static thread_local int g_tune_w2 = 0;
static thread_local int g_tune_w3 = 0;
static thread_local int g_tune_w4 = 0;
static thread_local int g_tune_w5 = 0;
static thread_local int g_tune_w6 = 0;
static thread_local bool g_tune_active = false;

void set_tune_weights(int score_w, int territory_w, int corner_w, int edge_w,
                      int adj_w, int recapture_w, int vulnerability_w) noexcept {
    g_tune_w0 = score_w;
    g_tune_w1 = territory_w;
    g_tune_w2 = corner_w;
    g_tune_w3 = edge_w;
    g_tune_w4 = adj_w;
    g_tune_w5 = recapture_w;
    g_tune_w6 = vulnerability_w;
    g_tune_active = true;
}

void clear_tune_weights() noexcept {
    g_tune_active = false;
}

int evaluate(const Board& board, int player) noexcept {
    const auto& ec = board.eval_cache;

    int score = board.score_from_perspective(player);

    int territory_diff = ec.my_territory - ec.opp_territory;
    int corner_diff = ec.my_corners - ec.opp_corners;
    int edge_diff = ec.my_edges - ec.opp_edges;
    int adj_diff = ec.live_adj_my - ec.live_adj_opp;
    int conn_diff = ec.connectivity_my - ec.connectivity_opp;

    if (player == k_player_opp) {
        territory_diff = -territory_diff;
        corner_diff = -corner_diff;
        edge_diff = -edge_diff;
        adj_diff = -adj_diff;
        conn_diff = -conn_diff;
    }

    // Use runtime weights if active
    if (g_tune_active) {
        return score * g_tune_w0
             + territory_diff * g_tune_w1
             + corner_diff * g_tune_w2
             + edge_diff * g_tune_w3
             + adj_diff * g_tune_w4
             + conn_diff * 0;
    }

    // Baseline weights (proven: 38% vs agent+superchym, FIRST 63%)
    // Score *3, Territory *3, Corners *8, Edges *2, LiveAdj *3
    return score * 3
         + territory_diff * 3
         + corner_diff * 8
         + edge_diff * 2
         + adj_diff * 3
         + conn_diff * 0;
}

} // namespace cordyceps

namespace cordyceps {

class RectTable;
class Search;
class Zobrist;

struct PassTracker {
    bool opp_has_passed{false};
    bool we_have_passed{false};
    int  last_pass_player{0};

    [[nodiscard]] bool is_game_over() const noexcept {
        return opp_has_passed && we_have_passed;
    }

    void reset() noexcept {
        opp_has_passed = false;
        we_have_passed = false;
        last_pass_player = 0;
    }
};

class Protocol {
public:
    Protocol();
    ~Protocol();
    void run();

private:
    Board board_;
    RectTable* table_{nullptr};
    Zobrist* zobrist_{nullptr};
    Search* search_{nullptr};
    PassTracker pass_tracker_;
    int our_player_{0};
    bool i_am_first_{false};

    void handle_ready(const std::string& line);
    void handle_init(const std::string& line);
    void handle_time(const std::string& line);
    void handle_opp(const std::string& line);
    void handle_finish();

    void write_move(const Move& mv) const;
};

} // namespace cordyceps



namespace cordyceps {

struct RectInfo {
    std::int8_t r1, c1, r2, c2;
    std::uint16_t cell_count;
    std::uint16_t cell_offset;
    Bitboard top_mask;
    Bitboard bottom_mask;
    Bitboard left_mask;
    Bitboard right_mask;
};

static_assert(sizeof(RectInfo) == 4 + 2 + 2 + 24 + 24 + 24 + 24, "RectInfo size mismatch");

class RectTable {
public:
    RectTable() noexcept = default;

    [[nodiscard]] bool load(const char* filename) noexcept;

    [[nodiscard]] int num_rects() const noexcept { return static_cast<int>(rects_.size()); }

    // O(1) rectangle ID from coordinates
    [[nodiscard]] int rect_id(int r1, int c1, int r2, int c2) const noexcept;

    // O(1) rect info lookup
    [[nodiscard]] const RectInfo& get_rect(int id) const noexcept { return rects_[id]; }

    // O(1) cell membership (span of indices in cell_table_)
    [[nodiscard]] std::span<const std::uint8_t> get_cells(int id) const noexcept;

private:
    std::vector<RectInfo> rects_;
    std::vector<std::uint8_t> cell_table_;
};

} // namespace cordyceps


namespace cordyceps {

bool RectTable::load(const char* filename) noexcept {
    FILE* f = std::fopen(filename, "rb");
    if (!f) return false;

    std::uint32_t magic, num_rects, cell_table_size, checksum;
    if (std::fread(&magic, 4, 1, f) != 1 || magic != 0x43524459) {
        std::fclose(f); return false;
    }
    { auto _ = std::fread(&num_rects, 4, 1, f); (void)_; }
    { auto _ = std::fread(&cell_table_size, 4, 1, f); (void)_; }
    { auto _ = std::fread(&checksum, 4, 1, f); (void)_; }

    rects_.resize(num_rects);

    // Read packed records (same layout as gen_geometry RectRecord)
    #pragma pack(push, 1)
    struct PackedRect {
        std::int8_t r1, c1, r2, c2;
        std::uint16_t cell_count, cell_offset;
        std::uint64_t top_lo, top_mid, top_hi;
        std::uint64_t bottom_lo, bottom_mid, bottom_hi;
        std::uint64_t left_lo, left_mid, left_hi;
        std::uint64_t right_lo, right_mid, right_hi;
    };
    #pragma pack(pop)
    static_assert(sizeof(PackedRect) == 104, "PackedRect must be 104 bytes");

    for (std::uint32_t i = 0; i < num_rects; ++i) {
        PackedRect pr;
        { auto _ = std::fread(&pr, sizeof(PackedRect), 1, f); (void)_; }
        auto& ri = rects_[i];
        ri.r1 = pr.r1; ri.c1 = pr.c1; ri.r2 = pr.r2; ri.c2 = pr.c2;
        ri.cell_count = pr.cell_count;
        ri.cell_offset = pr.cell_offset;
        ri.top_mask    = {pr.top_lo, pr.top_mid, pr.top_hi};
        ri.bottom_mask = {pr.bottom_lo, pr.bottom_mid, pr.bottom_hi};
        ri.left_mask   = {pr.left_lo, pr.left_mid, pr.left_hi};
        ri.right_mask  = {pr.right_lo, pr.right_mid, pr.right_hi};
    }

    cell_table_.resize(cell_table_size);
    { auto _ = std::fread(cell_table_.data(), 1, cell_table_size, f); (void)_; }
    std::fclose(f);
    return true;
}

int RectTable::rect_id(int r1, int c1, int r2, int c2) const noexcept {
    constexpr int k_num_col_pairs = k_cols * (k_cols + 1) / 2; // 153

    int row_offset = r1 * k_rows - r1 * (r1 - 1) / 2;
    int row_pair = row_offset + (r2 - r1);

    int col_offset = c1 * k_cols - c1 * (c1 - 1) / 2;
    int col_pair = col_offset + (c2 - c1);

    return row_pair * k_num_col_pairs + col_pair;
}

std::span<const std::uint8_t> RectTable::get_cells(int id) const noexcept {
    const auto& info = rects_[id];
    return {cell_table_.data() + info.cell_offset, info.cell_count};
}

} // namespace cordyceps



namespace cordyceps {

class RectTable;

// O(1) rectangle sum via 2D prefix sum
[[nodiscard]] int rect_sum(const Board& board, int r1, int c1, int r2, int c2) noexcept;

// Check inscribed rule: all 4 edges must touch at least one remaining mushroom
[[nodiscard]] bool check_inscribed(const Board& board, int r1, int c1, int r2, int c2) noexcept;

// Generate all valid (sum=10, inscribed) legal moves (brute-force)
[[nodiscard]] std::vector<Move> generate_legal_moves(const Board& board);

// Optimized: uses RectTable + PrefixSum for fast generation
[[nodiscard]] std::vector<Move> generate_legal_moves_optimized(const Board& board, const RectTable& table);

} // namespace cordyceps



namespace cordyceps {

// 2D prefix sum for O(1) rectangle sum queries
// Dimensions: (k_rows+1) x (k_cols+1) with sentinel row/col
class PrefixSum {
public:
    PrefixSum() noexcept;

    // Set a cell value (before build)
    void set(int r, int c, std::int8_t val) noexcept;

    // Build prefix sum table from stored values
    void build() noexcept;

    // O(1) sum of rectangle (r1,c1) to (r2,c2) inclusive
    [[nodiscard]] int sum(int r1, int c1, int r2, int c2) const noexcept;

    // Factory: build from Board values
    [[nodiscard]] static PrefixSum from_board(const Board& board) noexcept;

private:
    std::int8_t raw_[k_rows][k_cols]{};
    int pref_[(k_rows + 1) * (k_cols + 1)]{};
    bool built_{false};
};

} // namespace cordyceps


namespace cordyceps {

PrefixSum::PrefixSum() noexcept {
    for (int r = 0; r < k_rows; ++r)
        for (int c = 0; c < k_cols; ++c)
            raw_[r][c] = 0;
}

void PrefixSum::set(int r, int c, std::int8_t val) noexcept {
    raw_[r][c] = val;
    built_ = false;
}

void PrefixSum::build() noexcept {
    for (int r = 0; r <= k_rows; ++r) {
        for (int c = 0; c <= k_cols; ++c) {
            if (r == 0 || c == 0) {
                pref_[r * (k_cols + 1) + c] = 0;
            } else {
                pref_[r * (k_cols + 1) + c] = 
                    pref_[(r - 1) * (k_cols + 1) + c]
                    + pref_[r * (k_cols + 1) + (c - 1)]
                    - pref_[(r - 1) * (k_cols + 1) + (c - 1)]
                    + raw_[r - 1][c - 1];
            }
        }
    }
    built_ = true;
}

int PrefixSum::sum(int r1, int c1, int r2, int c2) const noexcept {
    const int R1 = r1, C1 = c1, R2 = r2 + 1, C2 = c2 + 1;
    return pref_[R2 * (k_cols + 1) + C2]
         - pref_[R1 * (k_cols + 1) + C2]
         - pref_[R2 * (k_cols + 1) + C1]
         + pref_[R1 * (k_cols + 1) + C1];
}

PrefixSum PrefixSum::from_board(const Board& board) noexcept {
    PrefixSum ps;
    for (int r = 0; r < k_rows; ++r)
        for (int c = 0; c < k_cols; ++c)
            ps.set(r, c, board.value_at(r, c));
    ps.build();
    return ps;
}

} // namespace cordyceps

namespace cordyceps {

int rect_sum(const Board& board, int r1, int c1, int r2, int c2) noexcept {
    int sum = 0;
    for (int r = r1; r <= r2; ++r) {
        for (int c = c1; c <= c2; ++c) {
            sum += board.value_at(r, c);
        }
    }
    return sum;
}

bool check_inscribed(const Board& board, int r1, int c1, int r2, int c2) noexcept {
    // Top edge
    bool ok = false;
    for (int c = c1; c <= c2 && !ok; ++c) {
        if (board.value_at(r1, c) > 0) ok = true;
    }
    if (!ok) return false;

    ok = false;
    for (int c = c1; c <= c2 && !ok; ++c) {
        if (board.value_at(r2, c) > 0) ok = true;
    }
    if (!ok) return false;

    ok = false;
    for (int r = r1; r <= r2 && !ok; ++r) {
        if (board.value_at(r, c1) > 0) ok = true;
    }
    if (!ok) return false;

    ok = false;
    for (int r = r1; r <= r2 && !ok; ++r) {
        if (board.value_at(r, c2) > 0) ok = true;
    }
    return ok;
}

std::vector<Move> generate_legal_moves(const Board& board) {
    std::vector<Move> moves;
    moves.reserve(256);

    for (int r1 = 0; r1 < k_rows; ++r1) {
        for (int r2 = r1; r2 < k_rows; ++r2) {
            int col_sums[k_cols]{};
            for (int c = 0; c < k_cols; ++c) {
                for (int r = r1; r <= r2; ++r) {
                    col_sums[c] += board.value_at(r, c);
                }
            }

            for (int c1 = 0; c1 < k_cols; ++c1) {
                int wsum = 0;
                for (int c2 = c1; c2 < k_cols; ++c2) {
                    wsum += col_sums[c2];
                    if (wsum > k_target_sum) break;

                    if (wsum == k_target_sum && check_inscribed(board, r1, c1, r2, c2)) {
                        moves.push_back({
                            static_cast<std::int8_t>(r1),
                            static_cast<std::int8_t>(c1),
                            static_cast<std::int8_t>(r2),
                            static_cast<std::int8_t>(c2)
                        });
                    }
                }
            }
        }
    }

    return moves;
}

std::vector<Move> generate_legal_moves_optimized(const Board& board, const RectTable& table) {
    PrefixSum ps = PrefixSum::from_board(board);
    std::vector<Move> moves;
    moves.reserve(256);

    int n = table.num_rects();
    for (int i = 0; i < n; ++i) {
        const auto& ri = table.get_rect(i);

        // Validate ranges
        if (ri.r1 < 0 || ri.r2 >= k_rows || ri.c1 < 0 || ri.c2 >= k_cols) continue;

        // Fast sum check with prefix sum
        if (ps.sum(ri.r1, ri.c1, ri.r2, ri.c2) != k_target_sum) continue;

        // Fast inscribed check with border masks
        Bitboard live = board.live_mask;
        if ((ri.top_mask & live).is_empty()) continue;
        if ((ri.bottom_mask & live).is_empty()) continue;
        if ((ri.left_mask & live).is_empty()) continue;
        if ((ri.right_mask & live).is_empty()) continue;

        moves.push_back({ri.r1, ri.c1, ri.r2, ri.c2});
    }

    return moves;
}

} // namespace cordyceps


namespace cordyceps {

class Zobrist {
public:
    Zobrist();

    [[nodiscard]] std::uint64_t compute(const Board& board) const noexcept;

private:
    std::uint64_t z_value_[k_cells][10]{};   // [cell_idx][value 1-9]
    std::uint64_t z_owner_[k_cells][2]{};    // [cell_idx][0=US, 1=OPP]
    std::uint64_t z_player_[2]{};            // [0=US, 1=OPP] — current_player
    std::uint64_t z_passes_[k_cells]{};      // [consecutive_passes]
};

} // namespace cordyceps


namespace cordyceps {

static std::uint64_t random_u64() {
    static std::mt19937_64 rng(123456789);
    return rng();
}

Zobrist::Zobrist() {
    for (int i = 0; i < k_cells; ++i) {
        for (int v = 0; v < 10; ++v) {
            z_value_[i][v] = random_u64();
        }
        z_owner_[i][0] = random_u64(); // US
        z_owner_[i][1] = random_u64(); // OPP
    }
    z_player_[0] = random_u64();
    z_player_[1] = random_u64();
    for (int i = 0; i < k_cells; ++i) {
        z_passes_[i] = random_u64();
    }
}

std::uint64_t Zobrist::compute(const Board& board) const noexcept {
    std::uint64_t h = 0;

    for (int i = 0; i < k_cells; ++i) {
        std::int8_t v = board.values[i];
        std::int8_t o = board.owners[i];

        if (v > 0) {
            h ^= z_value_[i][v];
        }
        if (o == k_player_us) {
            h ^= z_owner_[i][0];
        } else if (o == k_player_opp) {
            h ^= z_owner_[i][1];
        }
    }

    // Current player
    if (board.current_player == k_player_us) {
        h ^= z_player_[0];
    } else if (board.current_player == k_player_opp) {
        h ^= z_player_[1];
    }

    h ^= z_passes_[board.consecutive_passes];
    return h;
}

} // namespace cordyceps


namespace cordyceps {

struct TTEntry {
    enum Flag : std::uint8_t { EMPTY = 0, EXACT = 1, ALPHA = 2, BETA = 3 };

    std::uint64_t key{0};
    Move best_move{k_pass_move};
    int score{0};
    std::int8_t depth{0};
    Flag flag{EMPTY};
};

static_assert(sizeof(TTEntry) <= 32, "TTEntry should be <= 32 bytes");

class TranspositionTable {
public:
    explicit TranspositionTable(int size_power) noexcept;

    // Probe: returns flag. If valid for given depth, fills score and move.
    [[nodiscard]] TTEntry::Flag probe(std::uint64_t key, int depth, int& score, Move& mv) noexcept;

    // Store entry (always replaces)
    void store(std::uint64_t key, int depth, TTEntry::Flag flag, int score, const Move& mv) noexcept;

    // Clear all entries
    void clear() noexcept;

    [[nodiscard]] int size() const noexcept { return static_cast<int>(table_.size()); }

private:
    std::vector<TTEntry> table_;
    int mask_;
};

} // namespace cordyceps


namespace cordyceps {

TranspositionTable::TranspositionTable(int size_power) noexcept {
    int sz = 1 << size_power;
    table_.assign(sz, {});
    mask_ = sz - 1;
}

TTEntry::Flag TranspositionTable::probe(std::uint64_t key, int depth, int& score, Move& mv) noexcept {
    int idx = key & mask_;
    const auto& entry = table_[idx];

    if (entry.key != key) return TTEntry::EMPTY;
    if (entry.depth < depth) return TTEntry::EMPTY;

    score = entry.score;
    mv = entry.best_move;
    return entry.flag;
}

void TranspositionTable::store(std::uint64_t key, int depth, TTEntry::Flag flag, int score, const Move& mv) noexcept {
    int idx = key & mask_;
    auto& entry = table_[idx];

    // Depth-preferred: keep deeper entry from a different position
    // Always replace same position
    if (entry.flag != TTEntry::EMPTY && entry.key != key && entry.depth > depth) {
        return;
    }

    entry.key = key;
    entry.depth = static_cast<std::int8_t>(depth);
    entry.flag = flag;
    entry.score = score;
    entry.best_move = mv;
}

void TranspositionTable::clear() noexcept {
    for (auto& e : table_) {
        e.key = 0;
        e.flag = TTEntry::EMPTY;
    }
}

} // namespace cordyceps

namespace cordyceps {

struct EvalWeights {
    // Static eval weights (applied to evaluate())
    // -- not configurable, uses the hardcoded multipliers in evaluate()

    // Geometry eval weights (enabled with conservative values)
    int mobility = 1;       // per legal move difference (capped)
    int safe_cell = 1;      // per safe cell (our territory not in threat)
    int steal = 2;          // per steal opportunity (rects with only opponent cells)
};

// Default weights tuned for balanced play
constexpr EvalWeights k_default_weights{};

struct SearchResult {
    Move move;
    int eval;
    int max_depth{0};
    long long tt_probes{0};
    long long tt_hits{0};
    long long nodes{0};
};

struct SearchBenchmark {
    double avg_depth;
    double avg_hit_rate;
    double avg_nodes;
    double avg_ms;
    int samples;
};

class Search {
public:
    Search(const RectTable& table, const Zobrist& zobrist) noexcept
        : table_(table), zobrist_(zobrist), tt_(18) {} // 2^18 = 256K entries

    [[nodiscard]] SearchResult simple_search(const Board& board, const SideConfig& config) noexcept;
    
    // Iterative deepening with negamax
    [[nodiscard]] SearchResult iterative_deepening(Board& board, int time_ms, const SideConfig& config) noexcept;

    // Benchmark helper: run iterative_deepening on multiple random boards
    [[nodiscard]] static SearchBenchmark benchmark(const RectTable& table, const Zobrist& zobrist, int time_ms, int samples) noexcept;

private:
    const RectTable& table_;
    const Zobrist& zobrist_;
    TranspositionTable tt_;

    // Killer moves: 2 per depth (max depth 64)
    Move killer1_[64]{};
    Move killer2_[64]{};

    // Time management
    std::chrono::steady_clock::time_point start_time_;
    int time_limit_ms_{0};
    int node_count_{0};
    bool timed_out_{false};

    // TT statistics (per search)
    long long tt_probes_{0};
    long long tt_hits_{0};
    int max_depth_reached_{0};

    // Side-aware configuration (set per iterative_deepening call)
    bool prefer_vertical_{false};
    float aggression_{0.5f};
    float steal_bonus_{1.0f};
    float defense_bonus_{1.0f};

    // History heuristic: tracks which (r1,c1,r2,c2) pairs are successful
    int history_[k_rows][k_cols][k_rows][k_cols]{};

    [[nodiscard]] bool time_check() noexcept;

    // Move ordering score
    [[nodiscard]] int order_score(const Board& board, const Move& mv, int depth) noexcept;

    // Negamax α-β
    int negamax(Board& board, int depth, int alpha, int beta, bool allow_pass) noexcept;

    // Endgame exact solver (no depth limit, for live_count ≤ 12)
    int negamax_endgame(Board& board, int alpha, int beta, bool allow_pass) noexcept;

    // Root-level geometry enhancement: score each root move with eval+geometry
    void enhance_root_moves(Board& board, std::vector<Move>& moves, int player) noexcept;

    // Move ordering: sort moves before search loop
    void sort_moves(Board& board, std::vector<Move>& moves, int depth, const Move& tt_move) noexcept;
};

} // namespace cordyceps



namespace cordyceps {

// Detect game phase based on live_count (mushroom-bot thresholds)
// live > 32  -> Opening
// 20-32      -> Midgame
// 13-19      -> Late
// ≤ 12       -> Endgame
[[nodiscard]] GamePhase detect_phase(const Board& board) noexcept;
[[nodiscard]] int estimate_moves_left(int live_count) noexcept;

class TimeManager {
public:
    TimeManager() noexcept = default;

    // Per-move budget = remaining_ms * phase_pct / 100 * side_mult * margin_factor
    // phase_pct: 6% opening, 10% midgame, 12% late, 18% endgame
    [[nodiscard]] int get_budget(int live_count, const SideConfig& config,
                                  int remaining_ms, int margin) const noexcept;

private:
    [[nodiscard]] static float margin_factor(int margin) noexcept;
};

} // namespace cordyceps


namespace cordyceps {

GamePhase detect_phase(const Board& board) noexcept {
    int live = board.live_count;
    if (live > 32) return GamePhase::kOpening;
    if (live >= 20) return GamePhase::kMidgame;
    if (live > 12) return GamePhase::kLate;
    return GamePhase::kEndgame;
}

int estimate_moves_left(int live_count) noexcept {
    if (live_count > 60) return 22;
    if (live_count > 40) return 17;
    if (live_count > 25) return 12;
    if (live_count > 12) return 8;
    return 5;
}

float TimeManager::margin_factor(int margin) noexcept {
    if (margin > 40)  return 0.6f;
    if (margin > 20)  return 0.7f;
    if (margin > 5)   return 0.85f;
    if (margin > -5)  return 1.0f;
    if (margin > -20) return 1.2f;
    if (margin > -40) return 1.35f;
    return 1.5f;
}

static float phase_pct(int live_count, const SideConfig& /*config*/) noexcept {
    // Phase-based % of remaining per move (from log analysis of winning engines)
    // FIRST: 6% opening, 10% midgame, 12% late, 18% endgame
    // SECOND: adjusts via time_multiplier
    GamePhase phase;
    if (live_count > 32)      phase = GamePhase::kOpening;
    else if (live_count >= 20) phase = GamePhase::kMidgame;
    else if (live_count > 12)  phase = GamePhase::kLate;
    else                       phase = GamePhase::kEndgame;

    switch (phase) {
        case GamePhase::kOpening: return 6.0f;
        case GamePhase::kMidgame: return 10.0f;
        case GamePhase::kLate:    return 12.0f;
        case GamePhase::kEndgame: return 18.0f;
    }
    return 6.0f;
}

int TimeManager::get_budget(int live_count, const SideConfig& config,
                              int remaining_ms, int margin) const noexcept {
    // Emergency: <500ms remaining → fixed tiny budget
    if (remaining_ms < 500) {
        return 15;
    }

    // Base: % of remaining per move (from log analysis)
    float pct = phase_pct(live_count, config);
    float budget_f = remaining_ms * (pct / 100.0f);

    // Side multiplier (FIRST=1.0, SECOND=1.5)
    budget_f *= config.time_multiplier;

    // Margin factor (winning=save, losing=invest)
    budget_f *= margin_factor(margin);

    // Clamp to reasonable range
    if (budget_f < 10.0f) budget_f = 10.0f;

    // Generous cap: winning bots in logs spend up to 33%+
    // At 10000ms: SECOND 15% = 1500ms, so cap must allow this
    float max_budget = (live_count <= 12) ? 2500.0f : 2000.0f;
    if (budget_f > max_budget) budget_f = max_budget;

    // Hard limit: never use > 90% of remaining
    float hard_limit = remaining_ms * 0.9f;
    if (budget_f > hard_limit) budget_f = hard_limit;

    return static_cast<int>(budget_f);
}

} // namespace cordyceps

namespace cordyceps {

// ===== Time management =====

bool Search::time_check() noexcept {
    if (++node_count_ % 1024 == 0) {
        auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(
            std::chrono::steady_clock::now() - start_time_).count();
        if (elapsed >= time_limit_ms_) {
            timed_out_ = true;
            return false;
        }
    }
    return true;
}

// ===== Move ordering =====

int Search::order_score(const Board& board, const Move& mv, int depth) noexcept {
    if (mv.is_pass()) return -1;

    if (mv == killer1_[depth]) return 200;
    if (mv == killer2_[depth]) return 100;

    int h = history_[mv.r1][mv.c1][mv.r2][mv.c2];
    if (h > 0) return 50 + h;

    if (prefer_vertical_) {
        int height = mv.r2 - mv.r1 + 1;
        int width = mv.c2 - mv.c1 + 1;
        if (height > width) return 25;
    } else if (aggression_ < 0.4f) {
        int height = mv.r2 - mv.r1 + 1;
        int width = mv.c2 - mv.c1 + 1;
        if (width >= height) return 25;
    }

    int sum = 0;
    for (int r = mv.r1; r <= mv.r2; ++r)
        for (int c = mv.c1; c <= mv.c2; ++c)
            sum += board.value_at(r, c);
    return sum;
}

void Search::sort_moves(Board& board, std::vector<Move>& moves, int depth, const Move& tt_move) noexcept {
    std::sort(moves.begin(), moves.end(), [&](const Move& a, const Move& b) {
        int sa = order_score(board, a, depth);
        int sb = order_score(board, b, depth);
        if (a == tt_move) sa = 10000;
        if (b == tt_move) sb = 10000;
        return sa > sb;
    });
}

// ===== Root-level geometry enhancement =====

void Search::enhance_root_moves(Board& board, std::vector<Move>& moves, int player) noexcept {
    // Score each root move with static eval + geometry features
    // This improves initial best_move selection without search-speed penalty
    for (auto& mv : moves) {
        if (mv.is_pass()) continue;

        auto undo = board.apply_move(mv);
        int eval_score = evaluate(board, player);
        board.unmake_move(undo);

        // Mobility bonus: how many legal moves does opponent have after our move?
        auto opp_moves = generate_legal_moves_optimized(board, table_);
        int mobility_bonus = -static_cast<int>(opp_moves.size()) / 3;
        eval_score += mobility_bonus;

        // Steal bonus: does this rect contain any opponent cells?
        int rect_id_val = table_.rect_id(mv.r1, mv.c1, mv.r2, mv.c2);
        auto cells = table_.get_cells(rect_id_val);
        int opp_cells = 0;
        for (auto cidx : cells) {
            if (board.owners[cidx] == -player) ++opp_cells;
        }
        eval_score += opp_cells * 2;  // steal bonus

        mv.score_hint = eval_score;
    }
}

// ===== Futility pruning check =====

static bool is_futile(const Board&, int, int) noexcept {
    return false;
}

// ===== Negamax (original, no geometry — for backward compat) =====

static constexpr int MAX_DEPTH = 64;
static constexpr int INF = 999999;

int Search::negamax(Board& board, int depth, int alpha, int beta, bool allow_pass) noexcept {
    if (timed_out_) return 0;
    if (!time_check()) return 0;

    bool terminal = board.is_terminal();
    if (depth <= 0 || terminal) {
        return evaluate(board, board.current_player);
    }

    // TT probe
    ++tt_probes_;
    std::uint64_t hash = zobrist_.compute(board);
    int tt_score;
    Move tt_move;
    auto tt_flag = tt_.probe(hash, depth, tt_score, tt_move);
    if (tt_flag != TTEntry::EMPTY) ++tt_hits_;
    if (tt_flag == TTEntry::EXACT) return tt_score;
    if (tt_flag == TTEntry::ALPHA && tt_score <= alpha) return alpha;
    if (tt_flag == TTEntry::BETA && tt_score >= beta) return beta;

    // Futility pruning: skip unpromising branches at shallow depth
    if (is_futile(board, depth, alpha)) {
        return evaluate(board, board.current_player);
    }

    // Null-move pruning
    if (allow_pass && depth >= 3 && !terminal && board.consecutive_passes < 1) {
        auto undo = board.apply_move(k_pass_move);
        int score = -negamax(board, depth - 1 - 2, -beta, -beta + 1, false);
        board.unmake_move(undo);
        if (score >= beta) return beta;
    }

    auto moves = generate_legal_moves_optimized(board, table_);
    moves.push_back(k_pass_move);
    sort_moves(board, moves, depth, tt_move);

    int best_score = -INF;
    Move best_move = k_pass_move;
    int alpha_orig = alpha;
    int searched = 0;

    for (int i = 0; i < static_cast<int>(moves.size()); ++i) {
        const auto& mv = moves[i];
        auto undo = board.apply_move(mv);

        int score;
        bool is_full_search = false;

        if (searched >= 4 && depth >= 3 && !mv.is_pass() && mv != tt_move) {
            int R = 1 + (searched / 4);
            if (R > depth / 2) R = depth / 2;
            if (R < 1) R = 1;

            score = -negamax(board, depth - 1 - R, -alpha - 1, -alpha, true);
            if (score > alpha && score < beta) {
                score = -negamax(board, depth - 1, -beta, -alpha, true);
                is_full_search = true;
            }
        } else {
            score = -negamax(board, depth - 1, -beta, -alpha, true);
            is_full_search = true;
        }

        board.unmake_move(undo);

        if (score > best_score) {
            best_score = score;
            best_move = mv;
        }
        ++searched;

        if (score > alpha) {
            alpha = score;
        }
        if (alpha >= beta) {
            if (!mv.is_pass()) {
                if (mv != killer1_[depth]) {
                    killer2_[depth] = killer1_[depth];
                    killer1_[depth] = mv;
                }
                if (is_full_search) {
                    history_[mv.r1][mv.c1][mv.r2][mv.c2] += depth * depth;
                }
            }
            break;
        }
    }

    if (best_move != moves[0] && !best_move.is_pass() && !moves[0].is_pass()) {
        history_[moves[0].r1][moves[0].c1][moves[0].r2][moves[0].c2] -= depth;
    }

    TTEntry::Flag flag;
    if (best_score <= alpha_orig) flag = TTEntry::ALPHA;
    else if (best_score >= beta) flag = TTEntry::BETA;
    else flag = TTEntry::EXACT;
    tt_.store(hash, depth, flag, best_score, best_move);

    return best_score;
}

// ===== Endgame exact solver =====

int Search::negamax_endgame(Board& board, int alpha, int beta, bool allow_pass) noexcept {
    if (timed_out_) return 0;
    if (!time_check()) return 0;

    bool terminal = board.is_terminal();

    // Generate moves
    auto moves = generate_legal_moves_optimized(board, table_);

    if (terminal) {
        return evaluate(board, board.current_player);
    }

    // If no legal moves, must pass
    if (moves.empty()) {
        if (!allow_pass) {
            // Can't pass (previous move was already a pass)
            return evaluate(board, board.current_player);
        }
        auto undo = board.apply_move(k_pass_move);
        int score = -negamax_endgame(board, -beta, -alpha, true);
        board.unmake_move(undo);
        return score;
    }

    // TT probe with max depth
    ++tt_probes_;
    std::uint64_t hash = zobrist_.compute(board);
    int tt_score;
    Move tt_move;
    auto tt_flag = tt_.probe(hash, 64, tt_score, tt_move);
    if (tt_flag != TTEntry::EMPTY) ++tt_hits_;
    if (tt_flag == TTEntry::EXACT) return tt_score;
    if (tt_flag == TTEntry::ALPHA && tt_score <= alpha) return alpha;
    if (tt_flag == TTEntry::BETA && tt_score >= beta) return beta;

    // Null-move pruning with reduced R in endgame
    if (allow_pass && board.consecutive_passes < 1 && moves.size() > 1) {
        auto undo = board.apply_move(k_pass_move);
        int score = -negamax_endgame(board, -beta, -beta + 1, false);
        board.unmake_move(undo);
        if (score >= beta) return beta;
    }

    moves.push_back(k_pass_move);
    sort_moves(board, moves, 64, tt_move);

    int best_score = -INF;
    Move best_move = k_pass_move;
    int alpha_orig = alpha;

    for (int i = 0; i < static_cast<int>(moves.size()); ++i) {
        const auto& mv = moves[i];
        auto undo = board.apply_move(mv);

        int score;
        if (i == 0) {
            // Full search for PV move
            score = -negamax_endgame(board, -beta, -alpha, mv.is_pass());
        } else {
            // Zero window search for remaining moves
            score = -negamax_endgame(board, -alpha - 1, -alpha, mv.is_pass());
            if (score > alpha && score < beta) {
                score = -negamax_endgame(board, -beta, -alpha, mv.is_pass());
            }
        }

        board.unmake_move(undo);

        if (score > best_score) {
            best_score = score;
            best_move = mv;
        }

        if (score > alpha) {
            alpha = score;
        }
        if (alpha >= beta) {
            break;
        }
    }

    TTEntry::Flag flag;
    if (best_score <= alpha_orig) flag = TTEntry::ALPHA;
    else if (best_score >= beta) flag = TTEntry::BETA;
    else flag = TTEntry::EXACT;
    tt_.store(hash, 64, flag, best_score, best_move);

    return best_score;
}

// ===== Iterative Deepening =====

SearchResult Search::iterative_deepening(Board& board, int time_ms, const SideConfig& config) noexcept {
    start_time_ = std::chrono::steady_clock::now();
    time_limit_ms_ = time_ms;
    timed_out_ = false;
    node_count_ = 0;
    tt_probes_ = 0;
    tt_hits_ = 0;
    max_depth_reached_ = 0;
    tt_.clear();

    prefer_vertical_ = config.prefer_vertical;
    aggression_ = config.aggression;
    steal_bonus_ = config.steal_bonus;
    defense_bonus_ = config.defense_bonus;

    std::memset(history_, 0, sizeof(history_));
    for (int i = 0; i < 64; ++i) {
        killer1_[i] = {};
        killer2_[i] = {};
    }

    Move best_move = k_pass_move;
    int best_eval = evaluate(board, board.current_player);

    auto moves = generate_legal_moves_optimized(board, table_);
    if (moves.empty()) {
        return {k_pass_move, best_eval};
    }

    // Root-level geometry enhancement: score moves with eval + geometry
    enhance_root_moves(board, moves, board.current_player);

    // Sort by geometry-enhanced score
    std::sort(moves.begin(), moves.end(), [](const Move& a, const Move& b) {
        return a.score_hint > b.score_hint;
    });

    if (board.consecutive_passes >= 1) {
        int margin = board.score_from_perspective(board.current_player);
        if (margin > 0) {
            return {k_pass_move, best_eval};
        }
    }

    best_move = moves[0];

    GamePhase phase = detect_phase(board);
    bool endgame = (phase == GamePhase::kEndgame);

    if (endgame && board.live_count <= 12) {
        // Deep endgame: exact solver
        negamax_endgame(board, -INF, INF, true);
        if (timed_out_ && best_move.is_pass()) {
            best_move = moves[0];
        }
        std::uint64_t hash = zobrist_.compute(board);
        int tt_score;
        Move tt_move;
        auto flag = tt_.probe(hash, 64, tt_score, tt_move);
        if (flag != TTEntry::EMPTY && !tt_move.is_pass()) {
            bool valid = false;
            for (const auto& mv : moves) {
                if (mv.r1 == tt_move.r1 && mv.c1 == tt_move.c1 &&
                    mv.r2 == tt_move.r2 && mv.c2 == tt_move.c2) {
                    valid = true;
                    break;
                }
            }
            if (valid) {
                best_move = tt_move;
                best_eval = tt_score;
            }
        }
        max_depth_reached_ = 64;
        return {best_move, best_eval, max_depth_reached_, tt_probes_, tt_hits_, node_count_};
    }

    // Iterative deepening with progressive widening
    int max_d = MAX_DEPTH;
    int last_eval = 0;

    for (int d = 1; d <= max_d && !timed_out_; ++d) {
        max_depth_reached_ = d;
        int alpha = last_eval - 100;
        int beta = last_eval + 100;

        // Using regular negamax — negamax_geo available for future optimization
        int score = negamax(board, d, alpha, beta, true);
        if (timed_out_) break;

        if (score <= alpha) {
            score = negamax(board, d, -INF, beta, true);
        } else if (score >= beta) {
            score = negamax(board, d, alpha, INF, true);
        }
        if (timed_out_) break;

        last_eval = score;
        best_eval = score;

        std::uint64_t hash = zobrist_.compute(board);
        int tt_score;
        Move tt_move;
        auto flag = tt_.probe(hash, d, tt_score, tt_move);
        if (tt_move != k_pass_move && !tt_move.is_pass() && flag != TTEntry::EMPTY) {
            bool valid = false;
            for (const auto& mv : moves) {
                if (mv.r1 == tt_move.r1 && mv.c1 == tt_move.c1 &&
                    mv.r2 == tt_move.r2 && mv.c2 == tt_move.c2) {
                    valid = true;
                    break;
                }
            }
            if (valid) best_move = tt_move;
        }
    }

    return {best_move, best_eval, max_depth_reached_, tt_probes_, tt_hits_, node_count_};
}

// ===== Simple search =====

SearchResult Search::simple_search(const Board& board, const SideConfig&) noexcept {
    auto moves = generate_legal_moves_optimized(board, table_);
    int player = board.current_player;

    SearchResult best{k_pass_move, -999999};

    for (const auto& mv : moves) {
        Board tmp = board;
        tmp.current_player = player;
        auto undo = tmp.apply_move(mv);
        int score = evaluate(tmp, player);
        tmp.unmake_move(undo);

        if (score > best.eval) {
            best.eval = score;
            best.move = mv;
        }
    }

    if (moves.empty()) {
        return {k_pass_move, evaluate(board, player)};
    }

    if (board.consecutive_passes >= 1) {
        int margin = board.score_from_perspective(player);
        if (margin > 0) {
            return {k_pass_move, evaluate(board, player)};
        }
    }

    return best;
}

// ===== Benchmark =====

SearchBenchmark Search::benchmark(const RectTable& table, const Zobrist& zobrist, int time_ms, int samples) noexcept {
    SearchBenchmark bm{};
    bm.samples = samples;

    for (int s = 0; s < samples; ++s) {
        Board board;
        int target_live = 60 + (s * 7) % 50;
        for (int i = 0; i < target_live; ++i) {
            int idx = (i * 37 + s * 13) % k_cells;
            int val = 1 + ((i + s) % 9);
            board.values[idx] = static_cast<std::int8_t>(val);
        }
        board.current_player = (s % 2 == 0) ? k_player_us : k_player_opp;
        board.recalc_live_mask();

        Search search(table, zobrist);
        auto start = std::chrono::steady_clock::now();
        auto result = search.iterative_deepening(board, time_ms, {});
        auto end = std::chrono::steady_clock::now();
        auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(end - start).count();

        bm.avg_depth += result.max_depth;
        bm.avg_nodes += result.nodes;
        bm.avg_ms += elapsed;
        if (result.tt_probes > 0)
            bm.avg_hit_rate += static_cast<double>(result.tt_hits) / result.tt_probes * 100.0;
    }

    bm.avg_depth /= samples;
    bm.avg_nodes /= samples;
    bm.avg_ms /= samples;
    bm.avg_hit_rate /= samples;
    return bm;
}

} // namespace cordyceps

namespace cordyceps {

Protocol::Protocol() {
    std::ios::sync_with_stdio(false);
    std::cin.tie(nullptr);
    
    table_ = new RectTable();
    if (table_->load("data.bin")) {
        zobrist_ = new Zobrist();
        search_ = new Search(*table_, *zobrist_);
    }
}

Protocol::~Protocol() {
    delete table_;
    delete search_;
    delete zobrist_;
}

void Protocol::run() {
    std::string line;
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;

        if (line.starts_with("READY")) {
            handle_ready(line);
        } else if (line.starts_with("INIT")) {
            handle_init(line);
        } else if (line.starts_with("TIME")) {
            handle_time(line);
        } else if (line.starts_with("OPP")) {
            handle_opp(line);
        } else if (line.starts_with("FINISH")) {
            break;
        }
    }
}

void Protocol::handle_ready(const std::string& line) {
    i_am_first_ = (line.find("FIRST") != std::string::npos);
    our_player_ = i_am_first_ ? k_player_us : k_player_opp;
    pass_tracker_.reset();
    std::cout << "OK\n" << std::flush;
}

void Protocol::handle_init(const std::string& line) {
    std::istringstream iss(line);
    std::string cmd;
    iss >> cmd;

    board_ = Board{};
    for (int r = 0; r < k_rows; ++r) {
        unsigned long long row_val;
        if (!(iss >> row_val)) break;
        for (int c = k_cols - 1; c >= 0; --c) {
            int digit = static_cast<int>(row_val % 10);
            row_val /= 10;
            board_.values[r * k_cols + c] = static_cast<std::int8_t>(digit);
        }
    }
    board_.recalc_live_mask();
    board_.my_mask = Bitboard::empty();
    board_.opp_mask = Bitboard::empty();
    board_.current_player = our_player_;
}

void Protocol::handle_opp(const std::string& line) {
    std::istringstream iss(line);
    std::string cmd;
    int r1, c1, r2, c2, ms;
    iss >> cmd >> r1 >> c1 >> r2 >> c2 >> ms;

    Move opp_move{static_cast<std::int8_t>(r1), static_cast<std::int8_t>(c1),
                  static_cast<std::int8_t>(r2), static_cast<std::int8_t>(c2)};

    if (opp_move.is_pass()) {
        // BTC may send duplicate OPP pass lines (logging bug).
        // Skip the duplicate to avoid corrupting board state (consecutive_passes,
        // current_player). Only apply the first real pass.
        if (pass_tracker_.last_pass_player != k_player_opp) {
            pass_tracker_.opp_has_passed = true;
            pass_tracker_.last_pass_player = k_player_opp;
            static_cast<void>(board_.apply_move(opp_move));
        }
    } else {
        pass_tracker_.last_pass_player = 0;
        pass_tracker_.opp_has_passed = false;
        static_cast<void>(board_.apply_move(opp_move));
    }
}

void Protocol::handle_time(const std::string& line) {
    std::istringstream iss(line);
    std::string cmd;
    int our_time, opp_time;
    iss >> cmd >> our_time >> opp_time;

    SideConfig config{};
    config.time_multiplier = i_am_first_ ? 1.0f : 1.5f;
    config.aggression = i_am_first_ ? 0.3f : 0.7f;
    config.steal_bonus = 1.0f;
    config.defense_bonus = i_am_first_ ? 2.0f : 1.0f;
    config.prefer_vertical = !i_am_first_;

    TimeManager tm;
    int margin = board_.score_from_perspective(our_player_);
    int search_time_ms = tm.get_budget(board_.live_count, config, our_time, margin);

    Move best;
    if (search_) {
        auto result = search_->iterative_deepening(board_, search_time_ms, config);
        best = result.move;
    } else {
        best = k_pass_move;
    }

    if (best.is_pass()) {
        if (pass_tracker_.last_pass_player != our_player_) {
            pass_tracker_.we_have_passed = true;
            pass_tracker_.last_pass_player = our_player_;
        }
    } else {
        pass_tracker_.reset();
    }

    static_cast<void>(board_.apply_move(best));
    write_move(best);
}

void Protocol::handle_finish() {
}

void Protocol::write_move(const Move& mv) const {
    if (mv.is_pass()) {
        std::cout << "-1 -1 -1 -1\n" << std::flush;
    } else {
        std::cout << static_cast<int>(mv.r1) << ' '
                  << static_cast<int>(mv.c1) << ' '
                  << static_cast<int>(mv.r2) << ' '
                  << static_cast<int>(mv.c2) << '\n' << std::flush;
    }
}

} // namespace cordyceps

int main() {
    cordyceps::Protocol protocol;
    protocol.run();
    return 0;
}
