# pkt2ai

`pkt2ai` là công cụ Rust để giải mã file Cisco Packet Tracer (`.pkt`) thành XML, rút gọn topology thành JSON/Markdown, truy vấn cấu hình và xem mạng bằng đồ thị TUI tương tác. File `pkt2ai.py` được giữ lại làm bản tham chiếu hành vi.

Định dạng Packet Tracer được xử lý hoàn toàn cục bộ: lớp obfuscation → Twofish-128/EAX → qCompress/zlib. Tag EAX luôn được kiểm tra mặc định; mật khẩu, secret, community và key-string luôn được che mặc định.

## Cài đặt

Với Nix:

```bash
nix develop
cargo build --release
# hoặc chạy trực tiếp
nix run -- summary lab.pkt
```

Với Rust stable:

```bash
cargo install --path .
```

TUI được bật trong default features. Chỉ build CLI lõi, không kéo ratatui/crossterm:

```bash
cargo build --release --no-default-features
```

## CLI

```bash
pkt2ai decode lab.pkt -o lab.xml
pkt2ai encode lab.xml -o lab-new.pkt
pkt2ai summary lab.pkt
pkt2ai summary lab.xml --format md --full-config -o lab.md
pkt2ai tree lab.xml --depth 4

pkt2ai list lab.pkt --type Router
pkt2ai show lab.pkt R1
pkt2ai links lab.pkt --device R1 --json
pkt2ai grep lab.pkt 'router|network' -i --device R1

pkt2ai export lab.pkt --format dot -o lab.dot
pkt2ai export lab.pkt --format mermaid -o lab.mmd
pkt2ai export lab.pkt --format graphml -o lab.graphml
pkt2ai export lab.pkt --format svg -o lab.svg
pkt2ai export lab.pkt --format obsidian -o lab-vault
```

`decode`, `encode`, `summary` và `tree` chấp nhận `-` cho stdin/stdout. Export không ghi đè nếu thiếu `--force`. Tên thiết bị trong `show`, `links` và `grep` được tìm theo thứ tự: khớp chính xác, khớp không phân biệt hoa thường, rồi chuỗi con duy nhất.

Mã thoát: `0` thành công, `1` lỗi dữ liệu/I/O, `2` sai tham số, `3` grep không có kết quả. Broken pipe (ví dụ `pkt2ai summary lab.pkt | head`) được coi là thoát thành công.

## TUI

```bash
pkt2ai tui lab.pkt
pkt2ai tui                 # mở file picker .pkt/.xml/.json
```

TUI nhận dạng theo nội dung thay vì phần mở rộng. JSON là output của `summary`; nguồn JSON không thể xuất ngược XML/.pkt hay bật hiện secret vì không còn dữ liệu gốc.

Phím:

- `?` trợ giúp, `q` thoát, `o` mở file, `e` xuất, `/` tìm, `f` lọc loại.
- `Tab` chọn node kế tiếp; `j`/`k` chọn liên kết trong panel và `Enter` nhảy sang đầu kia.
- `l` bật nhãn, `r` chạy lại layout, `g` xoay vòng Packet Tracer → Tầng → Force.
- `0` fit-to-view, `+`/`-` zoom theo phần trăm so với fit, mũi tên pan, `[`/`]` đổi độ sâu local graph.
- `p` bật/tắt panel chi tiết (trong hộp xuất, `p` vẫn dùng để sửa đường dẫn).
- `s` bật/tắt che secret, `i` bật/tắt node cô lập.
- Chuột: click chọn, kéo để pan, cuộn để zoom. Mọi thao tác đều có phương án bàn phím.

Đồ thị dùng Braille 2×4 dots, tự fit theo đúng tỉ lệ lưới chấm, giữ đa cạnh và làm mờ node ngoài vùng chọn. Layout Tầng dùng BFS từ router/core; Force layout chạy trên worker thread với seed tái lập. Palette chỉ dùng màu ANSI cơ bản nên hoạt động ở terminal 16/256 màu; không cần truecolor.

Hộp xuất TUI hỗ trợ JSON, Markdown, XML, `.pkt`, Obsidian vault, DOT, Mermaid, GraphML và SVG. Nhấn `p` để sửa đường dẫn, `Tab` để đổi giữa toàn graph/phần đang hiện. File có sẵn luôn yêu cầu xác nhận.

## Phát triển

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Cấu trúc chính:

- `src/crypto.rs`, `src/ptformat.rs`: Twofish/CMAC/CTR/EAX và container Packet Tracer.
- `src/extract.rs`, `src/cli.rs`: mô hình serde, parser XML, Markdown, truy vấn và CLI.
- `src/graph.rs`, `src/layout.rs`, `src/export.rs`: logic không phụ thuộc terminal.
- `src/tui/`: ratatui canvas, app, file picker và export dialog.
- `tests/`: vector crypto, đối chiếu Python, CLI/BrokenPipe, graph/export và TestBackend TUI.

Các quyết định kỹ thuật, chiều phụ thuộc module và thuật toán đồ thị được mô tả chi tiết trong [ARCHITECTURE.md](ARCHITECTURE.md).

## Giới hạn

- Chưa có chế độ tô màu theo VLAN/subnet; graph tô màu theo loại thiết bị.
- File picker lọc theo đuôi để danh sách gọn, nhưng sau khi chọn vẫn nhận dạng nội dung.
- Snapshot headless nằm ở `tests/snapshots/tui_80x24.txt`; hiển thị thực tế còn phụ thuộc font Braille của terminal.

Chỉ dùng cho file của chính bạn hoặc file bạn được phép sử dụng.
