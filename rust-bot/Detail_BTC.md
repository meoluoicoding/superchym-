Để chuẩn bị môi trường phát triển (Environment Setup) khớp 100% với hệ thống chấm thi của Ban tổ chức **NYPC 2026 Master Track**, giúp loại bỏ hoàn toàn rủi ro lỗi biên dịch hay sai lệch hiệu năng, đây là bảng tổng hợp chi tiết toàn bộ thông số kỹ thuật từ trang cấu hình của ban tổ chức [NYPC Practice Session](https://contest.nypc.co.kr/problems/1).

---

## 1. Cấu hình Máy chủ Chấm thi (Hardware Spec)

Tất cả các Agent khi nộp lên đấu trường Arena sẽ chạy trên hạ tầng đám mây của Amazon Web Services (AWS) với cấu hình:

* **Instance Type:** AWS **c7a.2xlarge** (Dòng chip chuyên dụng tối ưu tính toán thế hệ mới của AMD).
* **Bộ xử lý (CPU):** Giới hạn nghiêm ngặt **đúng 1 CPU Core** cho mỗi Agent (Kiến trúc AMD EPYC thế hệ thứ 4).
* *Lưu ý:* Code C++ của bạn không cần viết đa luồng (`std::thread`), vì hệ thống chỉ cấp 1 nhân, viết đa luồng sẽ làm tăng chi phí quản lý luồng khiến bot chạy chậm đi.


* **Bộ nhớ (RAM Limit):** Tối đa **1,024 MiB (1 GB)**.
* Nếu thuật toán MCTS sinh node rác quá mức hoặc tra bảng dữ liệu vượt quá 1GB, hệ thống sẽ kích hoạt cơ chế OOM-Killer để giết tiến trình và báo lỗi `ABORT`.


* **Kết nối mạng:** **Ngắt hoàn toàn Internet** trong lúc trận đấu diễn ra.

---

## 2. Môi trường & Trình biên dịch C++ của Ban tổ chức (Compiler Spec)

Vì bạn đã chọn hướng đi **C++ thuần**, đây là các thông số bắt buộc phải thiết lập ở máy nhà:

* **Hệ điều hành Server:** **Ubuntu 24.04 LTS (Noble Numbat)** hoặc dòng RedHat tương đương.
* **Trình biên dịch mặc định:** `g++ 14.2.0` (Hỗ trợ hoàn chỉnh chuẩn **C++20** và phần lớn **C++23**).
* **Trình biên dịch thử nghiệm (Experimental):** Bản cấu hình cao hơn có sẵn `gcc 15` (Hỗ trợ **C++26**).
* **Thư viện bổ trợ cho phép:** Bộ thư viện **Boost C++ Libraries phiên bản 1.90.0** được cài đặt sẵn trên server. Bạn có thể tận dụng `boost::unordered_map` hoặc các cấu trúc tối ưu của Boost để tăng tốc.

---

## 3. Các Giới hạn về File nộp bài (Submission Limits)

Hệ thống Arena của Nexon có hai nút upload riêng biệt với quy định dung lượng nghiêm ngặt (được kiểm tra qua file cấu hình `tools/check_limits.py` trong dự án của bạn):

* **File Mã nguồn (`Source code`):** Chỉ được nộp **duy nhất 1 file** (ví dụ: `main.cpp`). Dung lượng file phải  nhỏ hơn 1 MiB.
* **File Dữ liệu (`data.bin`):** Được nộp kèm tối đa **1 file nhị phân** tĩnh. Dung lượng phải **nhỏ hơn 10 MiB**.

---

## 4. Công cụ Kiểm thử & Giả lập tại máy nhà (Testing Tools)

Ban tổ chức cung cấp 2 bộ công cụ để bạn giả lập trận đấu đối kháng (Bot-vs-Bot) cục bộ:

### Bộ công cụ dòng lệnh (CLI Tool)

* **Tên file:** `testing_tool.py` (Chạy trên nền Python 3.12).
* **Cơ chế hoạt động:** Đọc file cấu hình `setting.ini` để nạp ma trận nấm ban đầu và gọi 2 file thực thi của 2 bot đấu với nhau.
* **Định dạng file cấu hình mẫu (`setting.ini`):**
```ini
INPUT=./input.txt
LOG=./log.txt
EXEC1=./Main_Yeti.exe
EXEC2=./Main_PinkBean.exe

```



### Bộ công cụ giao diện (GUI Tool)

* **Tên công cụ:** `testing-tool-mushroom`
* **Hỗ trợ các nền tảng:** Windows 64-bit (x86-64), MacOS Universal, Ubuntu (file `.deb`), và RedHat (file `.rpm`). Công cụ này giúp bạn xem trực quan các bước đi hái nấm và cướp đất hiển thị trên màn hình.

---

## 🛠️ Khuyến nghị thiết lập Máy cá nhân của bạn ngay hôm nay:

Để biến máy tính của bạn thành một "phòng thí nghiệm" chuẩn xác, hãy thực hiện cài đặt theo các bước sau:

1. **Kích hoạt WSL2 trên Windows:** Cài đặt bản phân phối **Ubuntu 24.04 LTS** từ Microsoft Store.
2. **Cài đặt GCC 14 trong WSL2:** Mở Terminal Linux lên và chạy lệnh:
```bash
sudo apt update
sudo apt install build-essential g++-14 libboost-all-dev

```


3. **Lệnh Biên dịch Phòng thí nghiệm:** Luôn build code bằng lệnh ép chuẩn giống hệt BTC:
```bash
g++-14 -O3 -std=c++20 main.cpp -o main

```


*(Cờ `-O3` là bắt buộc để trình biên dịch tối ưu hóa vòng lặp MCTS, giúp tốc độ chạy ở nhà phản ánh chính xác tốc độ trên server BTC).*

