# pkt2ai

Giải mã file Cisco Packet Tracer (`.pkt`/`.pka`) sang XML và rút gọn thành JSON/Markdown để đưa cho AI đọc.
Một file Python duy nhất, chỉ dùng thư viện chuẩn (Python 3.8+), không cần cài thêm gì.

```bash
python pkt2ai.py decode lab.pkt -o lab.xml          # .pkt -> XML
python pkt2ai.py encode lab.xml -o lab_moi.pkt      # XML -> .pkt
python pkt2ai.py summary lab.pkt                    # JSON gọn ra màn hình
python pkt2ai.py summary lab.pkt --format md -o lab.md   # Markdown cho AI
python pkt2ai.py tree lab.xml --depth 4             # xem cấu trúc thẻ (debug)
```

`summary` nhận cả `.pkt` lẫn `.xml`. Kết quả gồm: thiết bị (loại, model, IP/gateway của PC,
VLAN, interface + IP trong running-config, khối `router ospf/rip/eigrp`, DHCP pool, ACL, line vty)
và danh sách liên kết `A:cổng <-> B:cổng`. File XML ~1 MB rút còn ~7–14 KB.

Tùy chọn của `summary`:
- Mặc định che `password / secret / community / key-string` thành `<redacted>`. Dùng `--keep-secrets` để giữ nguyên.
- `--full-config` kèm toàn bộ running-config nguyên văn.

## Cách hoạt động

`.pkt` → đảo byte + XOR → Twofish-EAX (key `0x89`×16, nonce `0x10`×16, tag 16 byte ở cuối)
→ XOR → 4 byte kích thước gốc (big-endian) + zlib → XML. Twofish, CMAC, CTR, EAX đều tự cài đặt.

## Đã kiểm tra

- Twofish khớp test vector chuẩn (key 0, plaintext 0 → `9F589F5C…E8C35A`) và khớp 200/200 khối ngẫu nhiên với bản tham chiếu `securedypkt`.
- Giải mã được 11 file `.pkt` thật (PT 7.1.1), mọi link đều khớp tên thiết bị; round-trip decode → encode → decode cho XML giống hệt.

## Chưa kiểm tra: Packet Tracer 9.x

Chưa có file `.pkt` do PT 9.x tạo ra để thử. Nếu giải mã báo `EAX tag khong khop` thì key/định dạng đã đổi.
Khi đó chạy với `--no-verify`, và xem các byte đầu của file để dò. Nếu giải mã được nhưng `summary` báo
"không tìm thấy NETWORK/DEVICES" thì tên thẻ XML đã đổi: chạy `tree` rồi sửa đường dẫn trong hàm `extract()`.

Chỉ dùng cho file của chính bạn hoặc file bạn được phép sử dụng.
