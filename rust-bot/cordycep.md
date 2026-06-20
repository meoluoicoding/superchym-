# CORDYCEPS ENGINE — Tài Liệu Toàn Diện

> **Ngôn ngữ:** C++20  
> **Trình biên dịch mục tiêu:** g++-14 (WSL Ubuntu 24.04)  
> **Luật chơi:** [GameRules.md](GameRules.md)  
> **Quy chế BTC:** [TONGHOP.md](TONGHOP.md), [Detail_BTC.md](Detail_BTC.md)  
> **Giới hạn hạ tầng:** 1 CPU Core, 1,024 MiB RAM, không network

---

## Mục Lục

1. [Tổng Quan Kiến Trúc](#1-tổng-quan-kiến-trúc)
2. [Luật Chơi Nấm](#2-luật-chơi-nấm)
3. [Cấu Trúc Dữ Liệu](#3-cấu-trúc-dữ-liệu)
4. [Thuật Toán Sinh Nước Đi](#4-thuật-toán-sinh-nước-đi)
5. [Hàm Đánh Giá (Evaluation)](#5-hàm-đánh-giá-evaluation)
6. [Thuật Toán Tìm Kiếm — Negamax Alpha-Beta](#6-thuật-toán-tìm-kiếm--negamax-alpha-beta)
7. [Iterative Deepening & Endgame Solver](#7-iterative-deepening--endgame-solver)
8. [Quản Lý Thời Gian](#8-quản-lý-thời-gian)
9. [Giao Thức I/O & Xử Lý Trận Đấu](#9-giao-thức-io--xử-lý-trận-đấu)
10. [Hệ Thống Test & Benchmark](#10-hệ-thống-test--benchmark)
11. [Hệ Thống Tuning Tự Động](#11-hệ-thống-tuning-tự-động)
12. [Quy Trình Build & Nộp Bài](#12-quy-trình-build--nộp-bài)
13. [Tổng Kết Hiệu Năng](#13-tổng-kết-hiệu-năng)

---

## 1. Tổng Quan Kiến Trúc

```
┌──────────────────────────────────────────────────────────────┐
│                       main.cpp                               │
│            (entry point: tạo Protocol, gọi run())             │
└─────────────────────┬────────────────────────────────────────┘
                      │
         ┌────────────▼────────────┐
         │      io/protocol        │  ← Giao tiếp BTC (READY/INIT/TIME/OPP/FINISH)
         │  - Protocol             │
         │  - PassTracker          │
         └──────┬──────────────────┘
                │
    ┌───────────┼───────────────────┐
    │           │                   │
    ▼           ▼                   ▼
┌─────────┐ ┌──────────┐    ┌──────────────┐
│ search  │ │ timeman  │    │   movegen    │
│Negamax  │ │ Phase    │    │ Brute-force  │
│α-β + TT │ │ Budget   │    │ Optimized    │
│+ LMR    │ │ Margin   │    │ (rect table) │
└────┬────┘ └──────────┘    └──────┬───────┘
     │                             │
     ▼                             ▼
┌─────────┐              ┌─────────────────┐
│  board  │              │   rect_table    │
│EvalCache│              │ 8415 pre-computed│
│UndoMove │              │ rects + bitmasks│
└────┬────┘              └────────┬────────┘
     │                            │
     ▼                            ▼
┌──────────┐             ┌─────────────────┐
│ zobrist  │             │    data.bin     │
│ 64-bit   │             │ (~920KB binary) │
│ hashing  │             └─────────────────┘
└────┬─────┘
     │
     ▼
┌──────────┐
│    tt    │
│ 256K     │
│ entries  │
└──────────┘
```

### Sơ đồ thư mục

```
Cordyceps-Final/
├── src/
│   ├── main.cpp                     # Entry point
│   ├── gen_geometry.cpp             # Pre-compute 8415 rects → data.bin (offline)
│   ├── common/
│   │   ├── types.hpp                # Constants, Move, GamePhase, SideConfig
│   │   ├── bitboard.hpp/.cpp        # 3×uint64_t bitboard (170 cells)
│   │   └── prefix_sum.hpp/.cpp      # 2D prefix sum O(1)
│   ├── engine/
│   │   ├── board.hpp/.cpp           # Board, EvalCache, UndoMove, evaluate()
│   │   ├── movegen.hpp/.cpp         # Brute-force + optimized move generation
│   │   ├── rect_table.hpp/.cpp      # RectInfo, RectTable, rect_id() O(1)
│   │   ├── search.hpp/.cpp          # Negamax α-β, iterative deepening
│   │   ├── zobrist.hpp/.cpp         # Zobrist hashing
│   │   ├── tt.hpp/.cpp              # TranspositionTable (2^18 entries)
│   │   └── timeman.hpp/.cpp         # Time management
│   └── io/
│       ├── protocol.hpp/.cpp        # BTC protocol, PassTracker
├── tests/unit/                      # 102 unit tests (GTest)
├── scripts/
│   ├── tournament.py                # Tournament runner
│   ├── merge.py                     # Flatten → single main.cpp
│   ├── analyze_time_pct.py          # Log analysis
│   └── ...
├── tools/
│   ├── tuner_cli.cpp                # Self-play simulator for tuning
│   └── tune_optuna.py               # Optuna hyperparameter optimizer
├── CMakeLists.txt
├── data.bin                         # Pre-computed rectangle table
└── testing_tool.py                  # BTC-provided CLI test tool
```

---

## 2. Luật Chơi Nấm

### Bàn Cờ

- **Kích thước:** 10 hàng × 17 cột = **170 ô**
- Mỗi ô chứa một cây nấm giá trị **1–9** (i.i.d. uniform). Ô trống = 0.
- Tổng giá trị toàn bàn: trung bình ~850.

### Nước Đi Hợp Lệ

Một nước đi là một hình chữ nhật `(r1, c1, r2, c2)` thỏa mãn:

1. **Tổng = 10:** Tổng giá trị nấm còn lại trong hình chữ nhật phải đúng bằng 10 (`k_target_sum=10`)
2. **Luật Nội Tiếp (Inscribed Rule):** Cả 4 cạnh của hình chữ nhật phải chạm vào ít nhất 1 cây nấm còn sống

### Nước Pass

- `(-1, -1, -1, -1)` — đại diện bởi `k_pass_move`
- Trận đấu kết thúc khi **cả hai người chơi pass liên tiếp** (`consecutive_passes >= 2`)

### Tính Điểm & Chiến Thắng

- **Điểm:** Tổng giá trị nấm đã thu thập
- **Cơ chế Cướp (Steal):** Khi chọn hình chữ nhật, các ô thuộc về đối thủ được chiếm lại
- **Thắng/Thua:** Người chơi sở hữu nhiều ô hơn khi kết thúc → thắng

---

## 3. Cấu Trúc Dữ Liệu

### 3.1 Bitboard (`src/common/bitboard.hpp`)

Mỗi bitboard gồm **3 × uint64_t** bao phủ 170 ô:

| Thành phần | Phạm vi ô    |
|------------|-------------|
| `lo`       | 0 – 63      |
| `mid`      | 64 – 127    |
| `hi`       | 128 – 169   |

**Các phép toán:** `set`, `clear`, `test`, `popcount` (dùng `std::popcount`), `XOR`, `AND`, `OR`, `is_empty`

**Ứng dụng:**
- `my_mask` — ô Cordyceps sở hữu
- `opp_mask` — ô đối thủ sở hữu
- `live_mask` — ô còn nấm sống
- Border masks trong `RectInfo` (top/bottom/left/right)

### 3.2 Move (4 bytes + padding)

```cpp
struct Move {
    int8_t r1, c1, r2, c2;   // Tọa độ hình chữ nhật
    int score_hint{0};        // Điểm sắp xếp tạm thời (root-level)
    bool is_pass() const;
};
constexpr Move k_pass_move{-1, -1, -1, -1};
```

### 3.3 Board (`src/engine/board.hpp`)

```cpp
struct Board {
    array<int8_t, 170>  values;    // Giá trị nấm 0-9
    array<int8_t, 170>  owners;    // 0=none, 1=us, -1=opp
    Bitboard my_mask, opp_mask, live_mask;
    int my_score, opp_score;
    int live_count;
    int current_player;            // 1=Cordyceps, -1=đối thủ
    int consecutive_passes;
    EvalCache eval_cache;          // 40 bytes, cập nhật incremental
};
```

### 3.4 EvalCache (40 bytes — Incremental)

Tất cả feature được cập nhật **tăng dần (incremental)** trong `apply_move()`, chỉ duyệt vùng ảnh hưởng của hình chữ nhật + các ô lân cận. Khi `unmake_move()`, khôi phục toàn bộ cache trong O(1).

| Feature             | Ý nghĩa                                    |
|---------------------|--------------------------------------------|
| `my_territory`      | Số ô Cordyceps sở hữu                      |
| `opp_territory`     | Số ô đối thủ sở hữu                        |
| `my_corners`        | Ô góc (4 góc) do Cordyceps sở hữu          |
| `opp_corners`       | Ô góc do đối thủ sở hữu                    |
| `my_edges`          | Ô biên do Cordyceps sở hữu                 |
| `opp_edges`         | Ô biên do đối thủ sở hữu                   |
| `live_adj_my`       | Ô nấm sống kề với lãnh thổ Cordyceps       |
| `live_adj_opp`      | Ô nấm sống kề với lãnh thổ đối thủ         |
| `connectivity_my`   | Cặp ô kề nhau cùng chủ Cordyceps           |
| `connectivity_opp`  | Cặp ô kề nhau cùng chủ đối thủ             |

### 3.5 UndoMove (~2.3KB)

Cho phép `make_move` / `unmake_move` với khôi phục đầy đủ:

```cpp
struct UndoMove {
    Move mv;
    int  changed_count;
    array<uint8_t, 170> changed_indices;  // Chỉ số ô bị thay đổi
    array<int8_t, 170>  old_values;       // Giá trị cũ của các ô
    Bitboard old_my_mask, old_opp_mask, old_live_mask;
    int old_live_count, old_my_score, old_opp_score;
    int old_consecutive_passes, old_current_player;
    EvalCache old_eval_cache;
};
```

### 3.6 RectTable — Bảng Hình Chữ Nhật Tiền Tính Toán

**RectInfo (80 bytes trên RAM):**
```cpp
struct RectInfo {
    int8_t r1, c1, r2, c2;         // 4 bytes
    uint16_t cell_count;           // Số ô trong rect
    uint16_t cell_offset;          // Offset vào cell_table_
    Bitboard top_mask;             // Các ô trên cạnh trên
    Bitboard bottom_mask;          // Các ô trên cạnh dưới
    Bitboard left_mask;            // Các ô trên cạnh trái
    Bitboard right_mask;           // Các ô trên cạnh phải
};
```

**Công thức `rect_id(r1,c1,r2,c2)` — O(1), không dùng hash table:**
```
row_pair = r1*10 - r1*(r1-1)/2 + (r2-r1)     // [0..54]
col_pair = c1*17 - c1*(c1-1)/2 + (c2-c1)     // [0..152]
rect_id  = row_pair * 153 + col_pair          // [0..8414]
```

**data.bin format:**
```
Magic: "CRDY" (0x43524459)      4 bytes
num_rects: 8415                  4 bytes
cell_table_size                  4 bytes
checksum                         4 bytes
8415 × PackedRect (104 bytes)  ~875 KB
cell_table_                     ~45 KB
──────────────────────────────────────
Total:                         ~920 KB (< 10 MiB limit)
```

### 3.7 TranspositionTable

| Tham số       | Giá trị            |
|---------------|-------------------|
| Dung lượng    | 2^18 = 256K entries |
| Kích thước/TTEntry | 32 bytes       |
| Tổng RAM      | ~8 MB              |
| Index         | `key & mask_`      |
| Replacement   | Depth-preferred (giữ entry sâu hơn) |

```cpp
struct TTEntry {
    uint64_t key;
    Move best_move;
    int score;
    int8_t depth;
    Flag flag;  // EMPTY=0, EXACT=1, ALPHA=2, BETA=3
};
```

### 3.8 Zobrist Hashing

- **MT19937-64** với seed `123456789`
- `z_value_[170][10]`: 170 ô × 9 giá trị nấm
- `z_owner_[170][2]`: ownership cho từng ô (US/OPP)
- `z_player_[2]`: người chơi hiện tại
- `z_passes_[170]`: số lần pass liên tiếp
- **Tổng:** 1870 key 64-bit ngẫu nhiên
- `compute()`: XOR tất cả các piece đang active — O(170)

### 3.9 2D Prefix Sum

- Pre-built từ `board.values`
- `sum(r1,c1,r2,c2)` = O(1) bằng công thức inclusion-exclusion
- Lưu dưới dạng `int[11][18]` (thêm padding 1 hàng/cột)

---

## 4. Thuật Toán Sinh Nước Đi

### 4.1 Brute-force (`generate_legal_moves`)

- 4 vòng lặp lồng: `(r1, r2, c1, c2)` → O(R²C²) = 55×153 = **8,415 hình chữ nhật**
- Tối ưu tổng cột: tích lũy `col_sums[c]` cho từng cặp hàng (r1,r2) để tránh tính lại tổng cột
- Kiểm tra inscribed: duyệt từng cạnh để tìm nấm sống
- Độ phức tạp: O(R²C² × max(R,C)) ≈ O(8415 × 17)

### 4.2 Optimized (`generate_legal_moves_optimized`) — Phiên bản chính

```
for each rect i in [0..8414]:
    ri = table.get_rect(i)
    
    // O(1) sum check: Prefix Sum
    if ps.sum(ri.r1, ri.c1, ri.r2, ri.c2) != 10 → skip
    
    // O(1) inscribed check: Bitmask AND với live_mask
    if (ri.top_mask & live).is_empty()    → skip
    if (ri.bottom_mask & live).is_empty() → skip
    if (ri.left_mask & live).is_empty()   → skip
    if (ri.right_mask & live).is_empty()  → skip
    
    moves.push_back({ri.r1, ri.c1, ri.r2, ri.c2})
```

- **Độ phức tạp:** O(8415) — mỗi rect được check trong O(1)
- **Hiệu năng:** ~2ms trên bàn đầy (benchmark xác nhận)
- Prefix sum được rebuild mỗi lần gọi (từ board hiện tại)

---

## 5. Hàm Đánh Giá (Evaluation)

### 5.1 Công thức cơ sở

```cpp
eval = score * 3          // Chênh lệch điểm thu thập (30%)
     + territory_diff * 3 // Chênh lệch ô sở hữu (30%)
     + corner_diff * 8    // Chênh lệch ô góc (80% — góc rất quan trọng)
     + edge_diff * 2      // Chênh lệch ô biên (20%)
     + live_adj_diff * 3  // Chênh lệch nấm kề lãnh thổ (30%)
     + connectivity * 0   // Vô hiệu hóa (bằng chứng: connectivity cao → dễ bị cướp)
```

Trong đó tất cả `diff` đều là `my - opp` tính từ `EvalCache`.

### 5.2 Side-Agnostic

```cpp
int evaluate(const Board& board, int player);
```

- Khi `player == k_player_opp`: đảo dấu tất cả diff
- Luôn trả về điểm từ góc nhìn của `player`

### 5.3 Runtime Weight Tuning

7 trọng số `thread_local` có thể override lúc runtime (dùng cho self-play tuning):
```
g_tune_w[0..6] = {score, territory, corners, edges, live_adj, recapture, vulnerability}
```
- Khi `g_tune_active = false` → dùng trọng số mặc định (zero overhead)
- Khi `g_tune_active = true` → dùng trọng số runtime

### 5.4 Root-Level Geometry Enhancement

Trước khi bắt đầu iterative deepening, mỗi nước đi gốc được đánh giá qua:

1. **Static eval** sau khi apply/unmake nước đi
2. **Mobility penalty:** `-opponent_legal_moves / 3` (càng hạn chế đối thủ càng tốt)
3. **Steal bonus:** `+opp_cells_in_rect * 2` (cướp ô đối thủ)
4. Kết quả lưu vào `Move::score_hint`, dùng để sắp xếp ban đầu

---

## 6. Thuật Toán Tìm Kiếm — Negamax Alpha-Beta

### 6.1 Core: `negamax(board, depth, alpha, beta, allow_pass)`

```
function negamax(board, depth, alpha, beta, allow_pass):
    # 1. Kiểm tra thời gian
    if timed_out: return 0
    if not time_check(): return 0
    
    # 2. Node lá / terminal
    if depth <= 0 or board.is_terminal():
        return evaluate(board, board.current_player)
    
    # 3. Transposition Table Probe
    hash = zobrist.compute(board)
    flag = tt.probe(hash, depth, score, tt_move)
    if flag == EXACT:   return score
    if flag == ALPHA and score <= alpha: return alpha
    if flag == BETA  and score >= beta:  return beta
    
    # 4. Futility Pruning (disabled)
    if is_futile(board, depth, alpha):
        return evaluate(board, board.current_player)
    
    # 5. Null-Move Pruning (depth ≥ 3)
    if allow_pass and depth >= 3 and not terminal and passes < 1:
        apply_move(PASS)
        score = -negamax(board, depth-3, -beta, -beta+1, allow_pass=false)
        unmake_move()
        if score >= beta: return beta  # cutoff
    
    # 6. Generate & sort moves
    moves = generate_legal_moves_optimized(board, table)
    moves.push(PASS)
    sort_moves(moves, depth, tt_move)
    
    # 7. Search loop
    best_score = -INF
    for i, mv in enumerate(moves):
        board.apply_move(mv)
        
        if searched >= 4 and depth >= 3 and mv != tt_move and not mv.is_pass():
            # Late Move Reduction (LMR)
            R = 1 + searched/4, capped at depth/2
            score = -negamax(board, depth-1-R, -alpha-1, -alpha, allow_pass=true)
            if score > alpha and score < beta:
                score = -negamax(board, depth-1, -beta, -alpha, allow_pass=true)  # re-search
        else:
            # Full search (first 4 moves + TT move)
            score = -negamax(board, depth-1, -beta, -alpha, allow_pass=true)
        
        board.unmake_move()
        
        if score > best_score: best_score = score; best_move = mv
        searched++
        if score > alpha: alpha = score
        if alpha >= beta:  # cutoff
            update_killers(mv)
            if full_search: history[mv] += depth²
            break
    
    # 8. History penalty cho nước đi đầu tiên nếu không tốt nhất
    if best_move != moves[0] and not best_move.is_pass():
        history[moves[0]] -= depth
    
    # 9. Store TT
    tt.store(hash, depth, flag, best_score, best_move)
    return best_score
```

### 6.2 Các Kỹ Thuật Cắt Tỉa & Tối Ưu

| Kỹ thuật                       | Mô tả                                                                 |
|-------------------------------|-----------------------------------------------------------------------|
| **Transposition Table**       | 256K entries, depth-preferred replacement. Cutoff EXACT/ALPHA/BETA    |
| **Null-Move Pruning**         | Depth ≥ 3, R=2. Giả định: pass → vị thế không quá tệ                 |
| **Late Move Reduction (LMR)** | Sau 4 nước đầu, tìm với depth giảm `R = 1 + searched/4`              |
| **Killer Moves**              | 2 killer/depth. Nước tạo cutoff không phải pass → priority 200/100   |
| **History Heuristic**         | `history_[10][17][10][17]` 4D array. Bonus = `depth²` khi cutoff      |
| **Aspiration Windows**        | Mỗi iteration bắt đầu với `[last_eval ± 100]`, re-search nếu vượt    |
| **Futility Pruning**          | Stub (disabled). Trả về false → không ảnh hưởng                       |
| **Move Ordering**             | TT move (10000) → Killers (200/100) → History (50+) → Shape/Side → Sum |

### 6.3 Move Ordering Chi Tiết

Thứ tự đánh giá nước đi:

```
1. TT best move            → score = 10000 (luôn search đầu tiên)
2. Killer1[depth]          → score = 200
3. Killer2[depth]          → score = 100
4. History heuristic       → score = 50 + history[mv] (nếu > 0)
5. Side-aware preference   → score = 25
   - FIRST: ưu tiên rect rộng ≥ cao (phòng thủ)
   - SECOND: ưu tiên rect cao > rộng (tấn công theo cột dọc)
6. Mushroom value sum      → score = tổng giá trị nấm trong rect (fallback)
```

---

## 7. Iterative Deepening & Endgame Solver

### 7.1 `iterative_deepening(board, time_ms, config)`

```python
def iterative_deepening(board, time_ms, config):
    # Khởi tạo
    start_time = now()
    time_limit = time_ms
    tt.clear()
    reset_killers()
    reset_history()
    
    # Root-level enhancement
    moves = generate_legal_moves_optimized(board)
    for mv in moves:
        apply(mv); mv.score_hint = evaluate(board) + mobility + steal_bonus; unmake(mv)
    moves.sort(by score_hint)
    
    # Pass nếu đang dẫn trước và consecutive_passes >= 1
    if consecutive_passes >= 1 and margin > 0:
        return PASS_MOVE
    
    # Endgame exact solver (live_count ≤ 12)
    if is_endgame and live_count <= 12:
        negamax_endgame(alpha=-INF, beta=INF)
        return tt_best_move
    
    # Iterative deepening
    for depth in 1..MAX_DEPTH:
        alpha = last_eval - 100  # aspiration window
        beta  = last_eval + 100
        
        score = negamax(board, depth, alpha, beta, allow_pass=true)
        if timed_out: break
        
        # Re-search nếu ngoài aspiration window
        if score <= alpha: score = negamax(board, depth, -INF, beta)
        elif score >= beta: score = negamax(board, depth, alpha, INF)
        if timed_out: break
        
        last_eval = score
        best_move = tt.probe(board_hash)  # Lấy best move từ TT
    
    return {best_move, best_eval, max_depth, tt_stats}
```

### 7.2 `negamax_endgame` — Exact Solver

- Kích hoạt khi `live_count ≤ 12`
- Không giới hạn depth (dùng depth=64 như constant)
- Không LMR
- Vẫn dùng TT và null-move pruning (R giảm)
- **Zero-window search:** PV move search full, các move còn lại search với window hẹp `[-alpha-1, -alpha]` rồi re-search nếu cần
- Trả về nước đi tối ưu chính xác cho bàn cờ nhỏ

---

## 8. Quản Lý Thời Gian

### 8.1 Phát Hiện Giai Đoạn

| Giai đoạn    | `live_count` | Budget % |
|-------------|-------------|----------|
| Opening      | > 32        | 6%       |
| Midgame      | 20 – 32     | 10%      |
| Late         | 13 – 19     | 12%      |
| Endgame      | ≤ 12        | 18%      |

### 8.2 Công Thức Budget

```
budget_ms = remaining_ms × (phase_pct / 100) × time_multiplier × margin_factor
```

**Trong đó:**

| Tham số           | FIRST (Đi trước) | SECOND (Đi sau) |
|-------------------|------------------|------------------|
| `time_multiplier` | 1.0              | 1.5              |
| `aggression`      | 0.3              | 0.7              |
| `steal_bonus`     | 1.0              | 1.0              |
| `defense_bonus`   | 2.0              | 1.0              |
| `prefer_vertical` | false (ngang)    | true (dọc)       |

**Margin Factor (điều chỉnh theo tỷ số):**

| Margin       | Factor  | Chiến lược           |
|-------------|---------|----------------------|
| > 40        | 0.60    | Đang thắng đậm → tiết kiệm |
| 20 – 40     | 0.70    | Thắng → tiết kiệm     |
| 5 – 20      | 0.85    | Thắng nhẹ             |
| -5 – 5      | 1.00    | Cân bằng              |
| -20 – -5    | 1.20    | Thua nhẹ → đầu tư     |
| -40 – -20   | 1.35    | Thua → đầu tư nhiều   |
| < -40       | 1.50    | Thua đậm → all-in     |

### 8.3 Giới Hạn & Emergency

- **Emergency:** `remaining_ms < 500` → budget cố định **15ms**
- **Cap:** 2000ms (2500ms cho endgame)
- **Hard limit:** không bao giờ > 90% thời gian còn lại

### 8.4 Ước Lượng Số Nước Đi Còn Lại

| live_count | Số nước ước tính |
|-----------|-----------------|
| > 60      | 22              |
| > 40      | 17              |
| > 25      | 12              |
| > 12      | 8               |
| ≤ 12      | 5               |

---

## 9. Giao Thức I/O & Xử Lý Trận Đấu

### 9.1 Protocol BTC

```
READY FIRST                          → "OK\n"
READY SECOND                         → "OK\n"
INIT <10 dòng, mỗi dòng 17 chữ số>   → (parse bàn cờ)
TIME <our_ms> <opp_ms>              → "<r1> <c1> <r2> <c2>\n"
OPP <r1> <c1> <r2> <c2> <ms>       → (cập nhật board)
FINISH                               → break
```

### 9.2 PassTracker — Xử Lý Bug BTC

**Vấn đề:** BTC in ra **2 dòng OPP pass trùng lặp** (logging bug, không phải 2 nước đi thật).

**Giải pháp `PassTracker`:**
- Theo dõi `last_pass_player` để deduplicate các pass liên tiếp từ cùng một người chơi
- Chỉ tính là game-over khi **hai người chơi khác nhau** pass liên tiếp
- Dòng OPP pass thứ 2 bị bỏ qua → không làm hỏng trạng thái board

### 9.3 Bug Phase 5C — `current_player = 0`

- `Board()` constructor set `current_player = 0`
- Protocol set `current_player` trong `handle_ready`, nhưng `handle_init` tạo Board mới với `current_player = 0`
- **Hậu quả:** Engine chơi như player 0 (không phải us cũng không phải opp), tất cả nước đi đều vào `opp_score`, engine nghĩ mình luôn thắng → pass sớm
- **Fix:** Thêm `board_.current_player = our_player_` trong `handle_init`
- **Impact:** Win rate tăng từ **38% → 82%** (FIRST 100%, SECOND 64%)

---

## 10. Hệ Thống Test & Benchmark

### 10.1 Tổng Quan

- **Framework:** GoogleTest (v1.17.0, FetchContent)
- **Tổng số test:** 102 tests / 18 file
- **Build + Run:** `cmake --build build --target cordyceps_tests && ctest --test-dir build`

### 10.2 Danh Sách Test Files

| File                          | Tests | Mô tả                                      |
|-------------------------------|-------|--------------------------------------------|
| `test_bitboard.cpp`           | 11    | set/clear/test, popcount, operators         |
| `test_board.cpp`              | 10    | apply/unmake, terminal, score perspective   |
| `test_movegen.cpp`            | 4     | Brute-force move generation                 |
| `test_movegen_optimized.cpp`  | 4     | Rect-table generation matches brute-force   |
| `test_prefixsum.cpp`          | 8     | Sum calculation, build                      |
| `test_recttable.cpp`          | 6     | Load data.bin, rect_id, cell lookup         |
| `test_eval.cpp`               | 8     | EvalCache incremental, perspective          |
| `test_eval_upgrade.cpp`       | 11    | Geometry-enhanced features                  |
| `test_search.cpp`             | 5     | Simple search, move selection               |
| `test_negamax.cpp`            | 5     | Depth match, time limit, pass logic         |
| `test_zobrist.cpp`            | 7     | Hash consistency, apply/unmake cycle        |
| `test_tt.cpp`                 | 8     | Probe, store, replacement, clear            |
| `test_benchmark.cpp`          | 4     | Search benchmark runs                       |
| `test_timeman.cpp`            | 7     | Phase detection, budget calculation         |
| `test_time_benchmark.cpp`     | 3     | Iterative deepening timing                  |
| `test_protocol_init.cpp`      | 4     | READY/INIT/OPP/TIME handlers               |
| `test_protocol_flow.cpp`      | 4     | Complete game flow simulation               |
| `test_turn2_crash.cpp`        | 2     | Regression: UndoMove overflow bug           |
| `test_tuner_weights.cpp`      | 4     | Runtime weight set/clear                    |

### 10.3 Tournament Runner (`scripts/tournament.py`)

- Cấu hình từ `tournament_config.ini`
- Chạy nhiều engine đối đầu, tự động swap side
- Sinh log format BTC với FIRST/SECOND tracking
- 28 games/run (7 boards × 2 sides × 2 games_per_board)

---

## 11. Hệ Thống Tuning Tự Động

### 11.1 `tuner_cli.cpp` — In-Process Game Simulator

- Mô phỏng self-play không cần I/O
- Nhận 7 trọng số eval từ command line
- Chạy trò chơi đến kết thúc, trả về kết quả (win/loss/draw)
- Designed để Optuna gọi với tham số khác nhau

### 11.2 `tune_optuna.py` — Hyperparameter Optimizer

- **Sampler:** TPE (Tree-structured Parzen Estimator)
- **7 tham số:** score_w, territory_w, corner_w, edge_w, adj_w, recapture_w, vulnerability_w
- **Resume:** SQLite storage
- **Multi-worker:** Chạy song song nhiều game evaluation
- Mỗi trial: chạy `tuner_cli` với bộ trọng số, đo win rate

---

## 12. Quy Trình Build & Nộp Bài

### 12.1 Build Cục Bộ (CMake)

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build --config Release --target cordyceps
```

3 targets:
- `cordyceps` — main executable
- `cordyceps_tests` — unit tests
- `tuner_cli` — game simulator for tuning

### 12.2 WSL Cross-Verification (Bắt buộc)

```bash
# Trong WSL Ubuntu
g++-14 -O3 -std=c++20 main.cpp -o main
python3 testing_tool.py main
```

### 12.3 Merge Single File (`scripts/merge.py`)

- Flatten tất cả `.hpp/.cpp` → 1 file `main.cpp` (< 1 MiB)
- Inline `#include "..."` recursively
- Strip header guards (`#ifndef`/`#define`/`#endif`, `#pragma once`)
- Deduplicate `#include <...>`
- **Output:** ~54 KiB

### 12.4 Giới Hạn Nộp BTC

| Loại file  | Giới hạn      |
|------------|---------------|
| main.cpp   | < 1 MiB (~54 KiB thực tế) |
| data.bin   | < 10 MiB (~920 KB thực tế) |

---

## 13. Tổng Kết Hiệu Năng

### 13.1 Kết Quả Đối Kháng (Post Phase 5C)

| Đối thủ                | Win Rate | Games |
|------------------------|----------|-------|
| vs agent-i-think-change | **79%**  | 14    |
| vs superchym            | **86%**  | 14    |
| **TỔNG**               | **82%**  | 28    |
|                         |          |       |
| As FIRST                | **100%** | 14    |
| As SECOND               | **64%**  | 14    |

### 13.2 Benchmark

| Metric                     | Giá trị            |
|----------------------------|-------------------|
| Search depth @200ms        | ~7.1              |
| Search depth @500ms        | ~8.2              |
| TT hit rate                | ~55%              |
| Move generation            | ~2ms (full board)  |
| Evaluation                 | O(1) (incremental) |
| Submission size            | ~54 KiB           |
| data.bin size              | ~920 KB           |

### 13.3 Tổng Hợp Kỹ Thuật

| Kỹ thuật                              | Độ phức tạp  | Mục đích                                    |
|---------------------------------------|-------------|---------------------------------------------|
| 2D Prefix Sum                         | Build O(N), Query O(1) | Tổng rect không cần duyệt lại |
| Pre-computed RectTable (8415 rects)   | O(1)/rect   | Sinh nước đi nhanh                          |
| Bitmask Border Check                  | O(1)        | Kiểm tra inscribed rule                     |
| Incremental EvalCache                 | O(affected) | Đánh giá không cần duyệt toàn bàn           |
| Transposition Table                   | O(1)        | Tránh search lại vị trí đã thấy             |
| Null-Move Pruning                     | -           | Cắt nhánh không cần thiết                   |
| Late Move Reduction                   | -           | Giảm depth cho nước đi xếp sau              |
| Killer + History Heuristic            | O(1)        | Sắp xếp nước đi hiệu quả                   |
| Aspiration Windows                    | -           | Thu hẹp window, giảm re-search              |
| Endgame Exact Solver                  | Full-depth  | Nước đi tối ưu cho bàn ≤ 12 ô sống          |
| Side-Aware Config (FIRST/SECOND)      | -           | Chiến lược khác biệt theo lượt              |
| Phase-Aware Time Management           | O(1)        | Phân bổ thời gian thông minh               |
| PassTracker (BTC bug workaround)      | O(1)        | Deduplicate OPP pass lines                  |
| Optuna Weight Tuning                  | -           | Tự động tối ưu trọng số eval               |

---

## Phụ Lục: Các Constant Quan Trọng

```cpp
namespace cordyceps {

constexpr int k_rows        = 10;     // Số hàng
constexpr int k_cols        = 17;     // Số cột
constexpr int k_cells       = 170;    // Tổng số ô
constexpr int k_target_sum  = 10;     // Tổng mục tiêu cho nước hợp lệ
constexpr int k_num_rects   = 8415;   // Số rect có thể trên bàn 10×17

constexpr int k_player_us   = 1;      // Cordyceps
constexpr int k_player_opp  = -1;     // Đối thủ
constexpr int k_no_owner    = 0;      // Ô trống

constexpr Move k_pass_move{-1, -1, -1, -1};

// Eval weights mặc định
// Score *3 | Territory *3 | Corners *8 | Edges *2 | LiveAdj *3 | Connectivity *0

// TT size: 2^18 = 256K entries (~8 MB)

// Time management:
// Opening:  6%  |  live > 32
// Midgame: 10%  |  20 ≤ live ≤ 32
// Late:    12%  |  13 ≤ live ≤ 19
// Endgame: 18%  |  live ≤ 12

// Side config:
// FIRST:  time_mult=1.0, aggression=0.3, defense=2.0, prefer_horizontal
// SECOND: time_mult=1.5, aggression=0.7, defense=1.0, prefer_vertical

} // namespace cordyceps
```
