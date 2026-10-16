# Kiến trúc pkt2ai

Tài liệu này giải thích các quyết định kỹ thuật của `pkt2ai`, luồng dữ liệu và cách các phần của chương trình phối hợp với nhau. Mục tiêu chính là giữ CLI nhỏ, có thể chạy độc lập, đồng thời cung cấp TUI đồ thị mà không làm logic nghiệp vụ phụ thuộc terminal.

## Mục tiêu thiết kế

- Giải mã và mã hóa lại định dạng Packet Tracer một cách tái lập, kiểm tra authentication tag trước khi trả XML.
- Rút XML lớn thành một mô hình topology ổn định để CLI, exporter và TUI dùng chung.
- Build được bản CLI thuần với `--no-default-features`; người không dùng TUI không phải kéo `ratatui` hoặc `crossterm`.
- Không khóa logic đồ thị vào giao diện. Graph, layout và export đều có thể unit test mà không cần terminal.
- Hoạt động trong terminal nhỏ, terminal 16/256 màu và khi không có chuột.
- Giữ hành vi tương thích với `pkt2ai.py`, là bản tham chiếu cho định dạng và quy tắc trích xuất.

## Vì sao chọn Rust và các thư viện hiện tại

### Rust 2024

Giải mã `.pkt` cần nhiều phép toán byte, số nguyên wrapping và kiểm tra biên. Rust cho phép biểu diễn rõ các trường hợp này mà không đánh đổi an toàn bộ nhớ. Một binary duy nhất cũng thuận tiện cho người dùng Linux/Nix hơn việc phải quản lý virtual environment.

### `clap`, `serde`, `serde_json`

- `clap` định nghĩa cây lệnh và mã hóa phần lớn kiểm tra tham số ngay tại biên CLI.
- `serde` làm `Info` trở thành hợp đồng dữ liệu chung giữa extractor, JSON, exporter và TUI.
- Thứ tự field nằm trong struct thay vì phụ thuộc map động, giúp output dễ đối chiếu với bản Python.

### `roxmltree` và `flate2`

`roxmltree` phù hợp vì extractor chỉ cần đọc cây XML, không chỉnh sửa DOM. Nó không giữ một mô hình XML có thể mutate nên chi phí và API nhỏ hơn. `flate2` xử lý phần zlib của qCompress và không ảnh hưởng đến cấu trúc XML sau giải nén.

### Twofish/EAX cài trong crate

Packet Tracer dùng tổ hợp cố định Twofish-128 + EAX với nonce/tag 16 byte. Phần crypto được cài cục bộ để biểu diễn đúng biến thể wire format này và được khóa bằng vector Twofish, CMAC/EAX và vector toàn pipeline. Đây là mã tương thích định dạng, không phải API crypto tổng quát; mọi thay đổi phải tiếp tục qua các vector trong test.

### `ratatui` và `crossterm` là feature tùy chọn

`ratatui` cung cấp mô hình render thuần vào `Buffer`, nhờ vậy cùng một code có thể chạy trên terminal thật và `TestBackend`. `crossterm` đảm nhiệm raw mode, alternate screen, chuột và sự kiện bàn phím đa nền tảng. Cả hai chỉ được bật bởi feature `tui`; core không import chúng.

### Nix

`flake.nix` cung cấp cả package tái lập và dev shell có cargo, rustc, clippy, rustfmt, rust-analyzer. `Cargo.lock` là đầu vào của `buildRustPackage`, vì đây là application chứ không phải library cần để downstream tự chọn phiên bản dependency.

## Ranh giới module

```text
                ┌──────────────┐
.pkt/.xml ─────▶│ ptformat     │───── XML bytes
                │ + crypto     │
                └──────┬───────┘
                       ▼
                ┌──────────────┐
                │ extract      │───── Info (serde)
                └───┬──────┬───┘
                    │      │
              ┌─────▼─┐  ┌─▼────────────┐
              │ CLI   │  │ Graph/Layout │
              └───────┘  └──┬────────┬──┘
                             │        │
                        ┌────▼───┐ ┌──▼──────┐
                        │ export │ │ TUI     │
                        └────────┘ └─────────┘
```

- `crypto.rs`: primitive Twofish-128, CMAC, CTR và EAX đúng biến thể Packet Tracer.
- `ptformat.rs`: obfuscation ngoài/trong, qCompress/zlib, nhận dạng XML và lỗi `PktError`.
- `extract.rs`: XML → `Info`, che secret, truy vấn thiết bị/liên kết và Markdown.
- `cli.rs`: ánh xạ lệnh thành API core, quản lý stdin/stdout, mã thoát và BrokenPipe.
- `graph.rs`: index tên node, adjacency, đa cạnh, local graph và tập node đang hiện.
- `layout.rs`: Packet Tracer coordinates, layout Tầng, Force và fit-to-view thuần.
- `export.rs`: DOT, Mermaid, GraphML, SVG và Obsidian; không phụ thuộc terminal.
- `tui/`: state machine, canvas, file picker và export dialog.

Chiều phụ thuộc luôn đi từ giao diện vào core. `layout.rs` và `export.rs` không biết `ratatui`; điều này ngăn việc thay backend terminal làm thay đổi kết quả graph/export.

## Luồng giải mã và trích xuất

1. `load_xml` nhận byte và nhận dạng XML theo nội dung; nếu không phải XML thì gọi `pkt_to_xml`.
2. `pkt_to_xml` gỡ obfuscation ngoài, xác thực EAX, gỡ obfuscation trong rồi giải nén qCompress.
3. `extract` duyệt đúng các đường dẫn Packet Tracer và tạo `Info`.
4. Các consumer chỉ làm việc với `Info`; không consumer nào tự parse lại XML.

Authentication tag được kiểm tra trước khi giải nén để dữ liệu hỏng không đi sâu hơn vào pipeline. Cảnh báo kích thước qCompress được giữ tách khỏi lỗi authentication vì Packet Tracer thực tế có thể chứa metadata không hoàn toàn đồng nhất.

## Mô hình đồ thị và node không khớp

Tên thiết bị là khóa node. Một cạnh giữ đồng thời `Link` gốc và hai index tùy chọn. Nếu đầu link là số hoặc không khớp tên, cạnh vẫn tồn tại để export/hiển thị cảnh báo nhưng renderer bỏ qua phần không thể nối. Vì vậy file lỗi một phần không làm toàn bộ graph panic.

Đa cạnh được đánh số `parallel_index` và `parallel_count`. Renderer/SVG dùng offset vuông góc nhỏ để nhiều dây giữa cùng hai thiết bị không chồng hoàn toàn.

## Ba chiến lược layout

### Packet Tracer

Giữ tọa độ `X,Y` gốc. Node thiếu tọa độ được đặt xác định trên một vòng fallback bằng seed cố định. Đây là mặc định khi có ít nhất một thiết bị có `xy`, vì nó giữ gần nhất ý đồ bố trí của tác giả lab.

### Tầng

Chọn gốc theo thứ tự Router → MultiLayerSwitch → node bậc cao nhất. BFS gán tầng; trong mỗi tầng, vài vòng sắp barycenter theo hàng cha làm giảm giao cắt. Thành phần không tới được gốc nằm ở hàng cuối. Khoảng cách dọc lớn hơn khoảng cách ngang vì sau đó tọa độ được chiếu lên lưới Braille 2×4.

### Force

Fruchterman–Reingold dùng RNG SplitMix64 tự cài với seed cố định, nên cùng graph cho cùng kết quả và không cần dependency `rand`. O(n²) phù hợp quy mô mục tiêu khoảng 500 node. TUI chạy layout này trên worker thread và nhận kết quả qua `std::sync::mpsc`, tránh chặn vòng lặp input/render.

## Fit-to-view và hệ tọa độ Braille

Một ô terminal chứa 2×4 chấm Braille, vì vậy không thể coi ô ký tự là pixel vuông. `fit_to_view` chuyển kích thước canvas sang số chấm `(width×2, height×4)`, tính scale chung để bbox dùng 90% chiều giới hạn, rồi trả `FitView` gồm tâm và half-size trong world space.

Zoom là tỷ lệ so với fit, không phải hệ số world tùy ý:

- `1.0` được hiển thị là `100% (fit)`;
- pan là offset world-space quanh tâm fit;
- đổi tập node hiện hoặc kích thước canvas làm vô hiệu fit và đưa zoom về 100%.

Hàm fit không phụ thuộc TUI và được test với graph một node, hai node, rất dài và rất cao.

## Render, màu và nhãn

Canvas vẽ theo ba lớp: cạnh, node Braille, rồi nhãn text. Mọi node dùng cùng hình thức cụm chấm; loại thiết bị chỉ khác màu và chú thích. Palette dùng màu ANSI đặt tên thay vì RGB truecolor để có fallback tự nhiên trên terminal hạn chế màu.

Nhãn được lập kế hoạch trong tọa độ ô trước khi render:

1. node đang chọn;
2. hàng xóm trực tiếp;
3. hub có bậc từ 3;
4. node khác khi zoom đủ lớn.

Thuật toán greedy thử phải, trái, trên, dưới node và loại nhãn ưu tiên thấp khi không còn chỗ. Node chọn/hàng xóm dùng phép tìm ô trống gần nhất nên không bị mất. Việc đổi ngược từ ô sang world coordinate dùng đúng công thức mapping `(width-1, height-1)` của `ratatui::Canvas` để tránh hai hàng nhãn bị làm tròn vào cùng một hàng.

## State machine của TUI

`TopologyApp` giữ dữ liệu, graph, layout, viewport và mode tương tác. `Mode` tách Normal, Help, Search, Filter, Picker, Export, Confirm và Message; mỗi phím chỉ được xử lý trong mode hiện tại nên phím `p` có thể là bật panel ở Normal nhưng là sửa path trong Export.

Mỗi frame thực hiện:

1. nhận kết quả worker nếu có;
2. sửa selection nếu node đã bị filter;
3. tính vùng canvas/panel;
4. fit lại nếu layout, tập visible hoặc kích thước thay đổi;
5. render canvas, panel, status và overlay.

Terminal session dùng guard cleanup để khôi phục raw mode, alternate screen, cursor và mouse capture khi thoát bình thường hoặc unwind.

## Export và an toàn ghi file

Exporter nhận `Info`, `Graph` và layout thay vì đọc state terminal. CLI yêu cầu `--force` trước khi ghi đè; TUI chuyển sang mode Confirm. Obsidian exporter chuẩn hóa tên file, phát hiện collision và tạo wikilink hai chiều.

Secret được che mặc định ngay ở extractor. TUI chỉ có thể đổi trạng thái che khi còn XML gốc; JSON summary không chứa đủ dữ liệu để khôi phục secret hay tạo lại `.pkt`.

## Chiến lược kiểm thử

- Vector unit cho Twofish/EAX và toàn pipeline `.pkt`.
- Round-trip nhiều kích thước và dữ liệu bị sửa.
- Đối chiếu với `pkt2ai.py` khi có Python.
- Integration test CLI, exit code và BrokenPipe.
- Graph rỗng, một node, đa cạnh, link không khớp và local graph.
- Parse lại GraphML/SVG bằng XML parser.
- Layout tái lập, hữu hạn, không trùng tọa độ; layout Tầng kiểm tra tầng của cạnh cây.
- `TestBackend` render 80×24, 120×40 và 160×50; test màu, cạnh, nhãn và panel không cần terminal thật.

Snapshot text phục vụ kiểm tra cấu trúc buffer. Nó không thay thế đánh giá font Braille, độ sáng màu và kích thước cell trên terminal thật.

## Giới hạn và hướng mở rộng

- Force layout vẫn là O(n²); graph lớn hơn đáng kể 500 node nên dùng Barnes–Hut hoặc spatial grid.
- Chưa có tô màu VLAN/subnet; có thể thêm một `ColorMode` độc lập mà không đổi `Graph`.
- Độ rộng Unicode hiện được giới hạn chủ yếu vào tên ASCII/Latin thông dụng; hỗ trợ đầy đủ grapheme width sẽ cần dependency mới và phải được cân nhắc riêng.
- File picker lọc extension để dễ dùng, nhưng loader vẫn nhận dạng nội dung sau khi chọn.
