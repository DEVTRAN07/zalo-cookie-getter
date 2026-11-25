//! CLI lấy cookie + imei Zalo bằng QR cho bot Rust.
//!
//! Cách dùng:
//!   zalo-cookie-getter [--out-dir <thư mục>] [--user-agent <UA>] [--qr <file>] [--retries <n>] [--no-verify]
//!
//! Mặc định ghi `account.json` + `cookie.json` + `qr.png` vào thư mục hiện tại.

use std::path::PathBuf;
use std::time::Duration;
use zalo_cookie_getter::{
    login_with_qr_once, verify_login, DEFAULT_MAX_ATTEMPTS, DEFAULT_USER_AGENT,
};

fn print_usage() {
    println!("zalo-cookie-getter — lấy cookie + imei Zalo bằng QR cho bot Rust");
    println!();
    println!("Cách dùng:");
    println!("  zalo-cookie-getter [--out-dir <thư mục>] [--user-agent <UA>]");
    println!("                     [--qr <file>] [--retries <n>] [--no-verify]");
    println!();
    println!("Tùy chọn:");
    println!("  --out-dir <thư mục>   Nơi ghi account.json + cookie.json + qr.png (mặc định: .)");
    println!("  --user-agent <UA>     User-Agent dùng để login (mặc định: macOS Firefox)");
    println!("  --qr <file>           Đường dẫn file qr.png (mặc định: <out-dir>/qr.png)");
    println!("  --retries <n>         Số lần thử quét QR tối đa (mặc định: 8)");
    println!("  --no-verify           Bỏ qua bước login lại bằng cookie để xác minh");
    println!("  --help                Hiện hướng dẫn này");
}

#[tokio::main]
async fn main() {
    let mut out_dir = PathBuf::from(".");
    let mut user_agent = DEFAULT_USER_AGENT.to_string();
    let mut qr_path: Option<PathBuf> = None;
    let mut retries: u32 = DEFAULT_MAX_ATTEMPTS;
    let mut no_verify = false;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                print_usage();
                return;
            }
            "--out-dir" => {
                out_dir = args.next().map(PathBuf::from).unwrap_or(out_dir);
            }
            "--user-agent" => {
                user_agent = args.next().unwrap_or(user_agent);
            }
            "--qr" => {
                qr_path = args.next().map(PathBuf::from);
            }
            "--retries" => {
                retries = args
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_MAX_ATTEMPTS);
            }
            "--no-verify" => no_verify = true,
            other => {
                eprintln!("[lỗi] Tùy chọn không rõ: {other}");
                print_usage();
                std::process::exit(2);
            }
        }
    }
    let qr_path = qr_path.unwrap_or_else(|| out_dir.join("qr.png"));
    let account_path = out_dir.join("account.json");

    println!("=== zalo-cookie-getter — lấy cookie + imei Zalo cho bot Rust ===");
    println!("[*] Thư mục lưu file: {}", out_dir.display());
    println!("[*] User-Agent: {user_agent}");
    println!("[*] QR hết hạn sau 100 giây — quét nhanh, hết hạn sẽ tự tạo mã mới.");
    println!("[!] CẢNH BÁO: account.json + cookie.json là chìa khóa đăng nhập.");
    println!("    Đừng gửi cho ai, đừng commit lên GitHub (đã chặn trong .gitignore).");
    println!();

    let mut attempt = 1;
    let outcome = loop {
        println!("[*] Lần thử {attempt}/{retries}: đang tạo mã QR...");
        match login_with_qr_once(&user_agent, &out_dir, &qr_path).await {
            Ok(outcome) => break outcome,
            Err(e) => {
                eprintln!("[lỗi] Lần {attempt} thất bại: {e}");
                if attempt >= retries {
                    eprintln!("[lỗi] Đã thử {retries} lần vẫn không xong. Kiểm tra mạng rồi chạy lại.");
                    std::process::exit(1);
                }
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    };

    println!();
    println!("=== ĐĂNG NHẬP THÀNH CÔNG ===");
    if !outcome.display_name.is_empty() {
        println!("Tài khoản: {}", outcome.display_name);
    }
    println!("UID:        {}", outcome.uid);
    println!("imei:       {}", outcome.imei);
    println!("userAgent:  {}", outcome.user_agent);
    println!("account.json: {}", outcome.account_path.display());
    println!("cookie.json:  {}", outcome.cookie_path.display());
    println!("qr.png:       {}", outcome.qr_path.display());

    if !no_verify {
        println!();
        println!("[*] Đang xác minh: login lại bằng cookie vừa lưu...");
        match verify_login(&account_path).await {
            Ok(uid) => {
                println!("[ok] Xác minh OK — login bằng cookie thành công, UID {uid}.");
                println!("[ok] File dùng được cho bot Rust.");
            }
            Err(e) => {
                eprintln!("[cảnh báo] Xác minh thất bại: {e}");
                eprintln!("       File đã lưu nhưng chưa chắc dùng được — thử chạy lại tool.");
                std::process::exit(1);
            }
        }
    }

    println!();
    println!("Bước tiếp theo: copy 2 file account.json + cookie.json vào thư mục");
    println!("chạy bot (cạnh config.yml), rồi chạy bot — bot sẽ login bằng cookie,");
    println!("không cần quét QR nữa.");
}
