# zalo-cookie-getter

Lấy **cookie + imei Zalo** bằng QR, ghi đúng định dạng bot Rust dùng:
`account.json` + `cookie.json`. Dành riêng cho bot Zalo viết bằng Rust
(dùng thư viện [zalo-rs](https://github.com/DEVTRAN07/Zalo-rs)).

> ⚠️ **CẢNH BÁO BẢO MẬT**
> `account.json` + `cookie.json` là chìa khóa đăng nhập tài khoản Zalo của bạn.
> **Đừng gửi cho ai, đừng commit 2 file này lên GitHub.**
> Repo này đã chặn sẵn trong `.gitignore` (`account.json`, `cookie.json`, `qr.png`).
> Đổi `userAgent` là phải xóa 2 file rồi quét QR lại, vì imei được sinh từ userAgent.

## Cách chạy (macOS)

Cần cài Rust trước (chưa có thì chạy `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`).

```sh
git clone https://github.com/DEVTRAN07/zalo-cookie-getter.git
cd zalo-cookie-getter
cargo run --release
```

Muốn lưu file vào thư mục khác:

```sh
cargo run --release -- --out-dir ~/bot-zalo
```

Các tùy chọn khác:

```sh
cargo run --release -- --help
# --out-dir <thư mục>   Nơi ghi account.json + cookie.json + qr.png (mặc định: .)
# --user-agent <UA>     User-Agent dùng để login (mặc định: macOS Firefox)
# --qr <file>           Đường dẫn file qr.png (mặc định: <out-dir>/qr.png)
# --retries <n>         Số lần thử quét QR tối đa (mặc định: 8)
# --no-verify           Bỏ qua bước login lại bằng cookie để xác minh
```

## Quy trình

1. Tool tạo mã QR và hiện ngay trong terminal (đồng thời lưu `qr.png`).
   Mã hết hạn sau 100 giây — quét nhanh, hết hạn tool tự tạo mã mới (tối đa 8 lần).
2. Mở Zalo trên điện thoại → quét mã → xác nhận đăng nhập.
3. Tool sinh `imei` từ userAgent, ghi `account.json` + `cookie.json`.
4. Tool tự login lại bằng cookie vừa lưu để xác minh file dùng được, rồi in
   ra `imei`, `userAgent`, `UID` và đường dẫn 2 file.

## File tạo ra

| File | Nội dung |
| --- | --- |
| `account.json` | `{"imei": "...", "userAgent": "...", "cookie": "cookie.json"}` |
| `cookie.json` | Mảng cookie `[{name, value, domain, path, expiration_date, host_only, http_only, secure, ...}]` |
| `qr.png` | Ảnh mã QR lần đăng nhập (chỉ để quét, không cần giữ) |

## Dùng với bot Rust

Copy 2 file `account.json` + `cookie.json` vào thư mục chạy bot (cạnh
`config.yml`), rồi chạy bot — bot sẽ login bằng cookie, không cần quét QR nữa.
Nếu cookie hết hạn, chạy lại tool này để lấy bộ mới.

## Phát triển

```sh
cargo check
cargo test
cargo run --release
```

Tool dùng `zalo-rs` ở pin `d2ba2818` (xem `Cargo.toml`).
`imei` được sinh bằng `zalo_rs::crypto::generate_zalo_uuid(user_agent)`
(định dạng `{uuid-v4}-{md5(userAgent)}`).
