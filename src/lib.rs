//! Lấy cookie + imei Zalo bằng QR, ghi đúng định dạng bot Rust dùng:
//! `account.json` + `cookie.json`.
//!
//! Luồng: QR login (zalo-rs) → sinh imei từ userAgent → ghi 2 file →
//! login lại bằng cookie để xác minh file dùng được.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zalo_rs::api::login_qr::{login_qr, LoginQrEvent, LoginQrOptions};
use zalo_rs::context::{create_context, Credentials, Options, ZaloCookie};
#[allow(deprecated)]
use zalo_rs::crypto::generate_zalo_uuid;
use zalo_rs::http::init_http;
use zalo_rs::ZaloClient;

/// User-Agent mặc định (macOS Firefox) — giống ví dụ trong config.yml của bot.
/// Đổi userAgent là PHẢI xóa account.json + cookie.json rồi quét QR lại,
/// vì imei được sinh TỪ userAgent này.
pub const DEFAULT_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0";

/// Số lần thử quét QR tối đa (khớp bot JS cũ: tối đa 8 lần).
pub const DEFAULT_MAX_ATTEMPTS: u32 = 8;

/// Kết quả sau khi QR login thành công và đã ghi file.
#[derive(Debug, Clone)]
pub struct LoginOutcome {
    pub uid: String,
    pub display_name: String,
    pub imei: String,
    pub user_agent: String,
    pub account_path: PathBuf,
    pub cookie_path: PathBuf,
    pub qr_path: PathBuf,
}

/// Sinh imei từ userAgent (định dạng `{uuid-v4}-{md5(userAgent)}`).
/// Upstream đánh deprecated vì ưu tiên IMEI thật từ trình duyệt, nhưng sau
/// QR login không có IMEI thật — đây là cách duy nhất (giống bot).
#[allow(deprecated)]
pub fn imei_from_user_agent(user_agent: &str) -> String {
    generate_zalo_uuid(user_agent)
}

/// Nội dung file `account.json` (định dạng bot JS cũ, bot Rust đọc y hệt).
pub fn build_account_json(imei: &str, user_agent: &str) -> Value {
    json!({
        "imei": imei,
        "userAgent": user_agent,
        "cookie": "cookie.json",
    })
}

/// Ghi `cookie.json` + `account.json` vào `out_dir`.
/// Trả về `(đường dẫn account.json, đường dẫn cookie.json)`.
pub fn save_account_files(
    out_dir: &Path,
    imei: &str,
    user_agent: &str,
    cookies: &[ZaloCookie],
) -> Result<(PathBuf, PathBuf), String> {
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let cookie_path = out_dir.join("cookie.json");
    let account_path = out_dir.join("account.json");
    let cookie_json =
        serde_json::to_string_pretty(cookies).map_err(|e| e.to_string())?;
    std::fs::write(&cookie_path, cookie_json).map_err(|e| e.to_string())?;
    let account_json = build_account_json(imei, user_agent);
    std::fs::write(
        &account_path,
        serde_json::to_string_pretty(&account_json).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok((account_path, cookie_path))
}

/// Đọc lại `account.json` (kèm file cookie nó trỏ tới) — cùng logic bot dùng
/// để login bằng cookie. Trả về `(imei, user_agent, cookies)`.
pub fn load_account(path: &Path) -> Result<(String, String, Vec<ZaloCookie>), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let parsed: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    let imei = parsed
        .get("imei")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let user_agent = parsed
        .get("userAgent")
        .or_else(|| parsed.get("user_agent"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if imei.is_empty() || user_agent.is_empty() {
        return Err("account.json thiếu imei hoặc userAgent".to_string());
    }
    let cookies: Vec<ZaloCookie> = match parsed.get("cookie") {
        Some(Value::String(file)) => {
            let cookie_path = if Path::new(file).is_absolute() {
                PathBuf::from(file)
            } else {
                path.parent()
                    .map(|p| p.join(file))
                    .unwrap_or_else(|| PathBuf::from(file))
            };
            let cookie_raw = std::fs::read_to_string(&cookie_path)
                .map_err(|e| format!("đọc {}: {e}", cookie_path.display()))?;
            let cookie_val: Value =
                serde_json::from_str(&cookie_raw).map_err(|e| e.to_string())?;
            serde_json::from_value(cookie_val)
                .map_err(|e| format!("cookie parse failed: {e}"))?
        }
        Some(Value::Array(_)) => serde_json::from_value(
            parsed.get("cookie").unwrap().clone(),
        )
        .map_err(|e| format!("cookie parse failed: {e}"))?,
        _ => match parsed.get("cookies") {
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|e| format!("cookie parse failed: {e}"))?,
            None => return Err("account.json thiếu cookie".to_string()),
        },
    };
    if cookies.is_empty() {
        return Err("cookie rỗng".to_string());
    }
    Ok((imei, user_agent, cookies))
}

/// Login bằng cookie từ `account.json`, trả về uid. Dùng để xác minh file
/// vừa ghi có dùng được không (không cần quét QR lại).
pub async fn verify_login(account_path: &Path) -> Result<String, String> {
    let (imei, user_agent, cookies) = load_account(account_path)?;
    let client = ZaloClient::new(Options {
        self_listen: true,
        ..Default::default()
    });
    let api = client
        .login(Credentials {
            imei,
            cookie: cookies,
            user_agent,
            language: Some("vi".to_string()),
        })
        .await
        .map_err(|e| e.to_string())?;
    Ok(api.uid().to_string())
}

/// Giải mã ảnh QR base64 (có hoặc không có tiền tố `data:image/png;base64,`).
pub fn qr_png_bytes_from_base64(image_b64: &str) -> Result<Vec<u8>, String> {
    let raw = image_b64
        .strip_prefix("data:image/png;base64,")
        .unwrap_or(image_b64);
    B64.decode(raw).map_err(|e| format!("base64 QR lỗi: {e}"))
}

/// Vẽ ảnh QR PNG thành chữ trong terminal (mỗi ký tự = 2 pixel dọc).
/// Trả về chuỗi nhiều dòng, có viền trắng (quiet zone) quanh mã.
pub fn render_qr_terminal(png_bytes: &[u8]) -> Result<String, String> {
    let img = image::load_from_memory(png_bytes).map_err(|e| e.to_string())?;
    let gray = img.to_luma8();
    let (w, h) = gray.dimensions();
    if w == 0 || h == 0 {
        return Err("ảnh QR rỗng".to_string());
    }
    // Cột ký tự tối đa 44 (mỗi ký tự = 1 pixel ngang, 2 pixel dọc).
    let cols = 44u32.min(w);
    let rows = ((cols as f64 * h as f64 / w as f64) / 2.0).ceil() as u32;
    let rows = rows.max(1);
    let px_w = (w as f64 / cols as f64).max(1.0);
    let px_h = (h as f64 / (rows as f64 * 2.0)).max(1.0);
    let dark = |x: u32, y: u32| -> bool {
        let sx = ((x as f64 * px_w) as u32).min(w - 1);
        let sy = ((y as f64 * px_h) as u32).min(h - 1);
        gray.get_pixel(sx, sy).0[0] < 128
    };
    let mut out = String::new();
    let margin = "  ";
    out.push_str(margin);
    out.push_str(&" ".repeat((cols + 4) as usize));
    out.push('\n');
    for r in 0..rows {
        out.push_str(margin);
        out.push_str("  ");
        for c in 0..cols {
            let top = dark(c, r * 2);
            let bottom = dark(c, r * 2 + 1);
            out.push(match (top, bottom) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            });
        }
        out.push_str("  \n");
    }
    out.push_str(margin);
    out.push_str(&" ".repeat((cols + 4) as usize));
    out.push('\n');
    Ok(out)
}

/// QR login một lần: hiện QR (lib tự lưu file `qr_path`), chờ quét,
/// rồi sinh imei + ghi 2 file. Hết hạn/bị từ chối thì trả Err để caller retry.
pub async fn login_with_qr_once(
    user_agent: &str,
    out_dir: &Path,
    qr_path: &Path,
) -> Result<LoginOutcome, String> {
    let mut ctx = create_context(30, 685);
    let store = Arc::new(reqwest_cookie_store::CookieStoreMutex::new(
        cookie_store::CookieStore::new(None),
    ));
    init_http(&ctx, store).await.map_err(|e| e.to_string())?;
    let qr_path_str = qr_path.to_string_lossy().to_string();
    let qr_path_for_cb = qr_path.to_path_buf();
    let callback = Box::new(move |event: LoginQrEvent| match event {
        LoginQrEvent::QrGenerated { code, image, .. } => {
            println!("[qr] Mã QR đã tạo (code {code})");
            match qr_png_bytes_from_base64(&image).and_then(|b| render_qr_terminal(&b)) {
                Ok(art) => {
                    println!("{art}");
                    println!(
                        "[qr] Mở Zalo trên điện thoại → quét mã trên → xác nhận đăng nhập."
                    );
                    println!(
                        "[qr] Ảnh QR cũng đã lưu tại {} (mở file này quét cũng được).",
                        qr_path_for_cb.display()
                    );
                }
                Err(_) => {
                    println!(
                        "[qr] Không vẽ được mã trong terminal — mở file {} để quét.",
                        qr_path_for_cb.display()
                    );
                }
            }
        }
        LoginQrEvent::QrScanned { display_name, .. } => {
            println!("[qr] Đã quét ({display_name}) — chờ xác nhận trên điện thoại...");
        }
        LoginQrEvent::QrDeclined { .. } => {
            println!("[qr] Mã QR bị từ chối.");
        }
        LoginQrEvent::QrExpired => {
            println!("[qr] Mã QR hết hạn (100 giây).");
        }
        LoginQrEvent::GotLoginInfo => {
            println!("[qr] Đã lấy thông tin đăng nhập...");
        }
        LoginQrEvent::Error(msg) => {
            println!("[qr] Lỗi: {msg}");
        }
    });
    let result = login_qr(
        &mut ctx,
        LoginQrOptions {
            user_agent: user_agent.to_string(),
            qr_path: Some(qr_path_str),
        },
        Some(callback),
    )
    .await
    .map_err(|e| e.to_string())?;
    let display_name = result
        .user_info
        .get("displayName")
        .or_else(|| result.user_info.get("display_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let imei = imei_from_user_agent(user_agent);
    let (account_path, cookie_path) =
        save_account_files(out_dir, &imei, user_agent, &result.cookies)?;
    println!(
        "[ok] Đã lưu cookie vào {} và {}",
        cookie_path.display(),
        account_path.display()
    );
    // Login lại bằng cookie vừa lấy để có uid (giống bot làm sau QR login).
    let uid = verify_login(&account_path).await?;
    Ok(LoginOutcome {
        uid,
        display_name,
        imei,
        user_agent: user_agent.to_string(),
        account_path,
        cookie_path,
        qr_path: qr_path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAAAAABX3VL4AAAAC0lEQVR4nGNgAAMAAAcAAbKGrPQAAAAASUVORK5CYII=";

    #[test]
    fn account_json_round_trip() {
        let v = build_account_json("imei-123", "TestUA/1.0");
        assert_eq!(v["imei"], "imei-123");
        assert_eq!(v["userAgent"], "TestUA/1.0");
        assert_eq!(v["cookie"], "cookie.json");
        let s = serde_json::to_string_pretty(&v).unwrap();
        let back: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }

    #[test]
    fn cookie_json_round_trip() {
        let cookies = vec![ZaloCookie {
            name: "zpw_sek".to_string(),
            value: "abc123".to_string(),
            domain: Some(".zalo.me".to_string()),
            path: Some("/".to_string()),
            expiration_date: Some(1790000000.0),
            host_only: Some(false),
            http_only: Some(false),
            secure: Some(true),
            same_site: None,
            session: None,
            store_id: None,
            key: None,
        }];
        let s = serde_json::to_string_pretty(&cookies).unwrap();
        let back: Vec<ZaloCookie> = serde_json::from_str(&s).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name, "zpw_sek");
        assert_eq!(back[0].value, "abc123");
        assert_eq!(back[0].domain.as_deref(), Some(".zalo.me"));
    }

    #[test]
    fn imei_format_is_uuid_plus_md5() {
        let imei = imei_from_user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15)");
        let (uuid_part, md5_part) = imei.split_once('-').unwrap();
        // uuid v4: 8-4-4-4-12 hex, rồi "-" + md5 hex 32 ký tự.
        assert_eq!(uuid_part.len(), 8);
        assert_eq!(md5_part.len(), 32 + 1 + 4 + 1 + 4 + 1 + 4 + 1 + 12);
        assert!(md5_part.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        // Cùng userAgent cho cùng hậu tố md5, uuid khác nhau mỗi lần.
        let imei2 = imei_from_user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15)");
        assert_ne!(imei, imei2);
        assert_eq!(imei.split('-').last(), imei2.split('-').last());
    }

    #[test]
    fn qr_base64_decode_and_render() {
        let with_prefix = format!("data:image/png;base64,{TINY_PNG_B64}");
        let bytes = qr_png_bytes_from_base64(&with_prefix).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let bytes2 = qr_png_bytes_from_base64(TINY_PNG_B64).unwrap();
        assert_eq!(bytes, bytes2);
        let art = render_qr_terminal(&bytes).unwrap();
        assert!(!art.trim().is_empty());
        assert!(render_qr_terminal(b"not a png").is_err());
        assert!(qr_png_bytes_from_base64("!!!").is_err());
    }

    #[test]
    fn save_and_load_files_round_trip() {
        let dir = std::env::temp_dir()
            .join(format!("zalo-cookie-getter-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cookies = vec![ZaloCookie {
            name: "zpw_sek".to_string(),
            value: "v1".to_string(),
            domain: Some(".zalo.me".to_string()),
            path: Some("/".to_string()),
            expiration_date: None,
            host_only: None,
            http_only: None,
            secure: None,
            same_site: None,
            session: None,
            store_id: None,
            key: None,
        }];
        let (acc, ck) =
            save_account_files(&dir, "imei-x", "UA/1.0", &cookies).unwrap();
        assert!(acc.exists() && ck.exists());
        let (imei, ua, back) = load_account(&acc).unwrap();
        assert_eq!(imei, "imei-x");
        assert_eq!(ua, "UA/1.0");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].value, "v1");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_account_rejects_bad_files() {
        let dir = std::env::temp_dir()
            .join(format!("zalo-cookie-getter-test-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("account.json");
        std::fs::write(&p, r#"{"imei": "", "userAgent": "UA", "cookie": "cookie.json"}"#)
            .unwrap();
        assert!(load_account(&p).is_err());
        std::fs::write(&p, r#"{"imei": "i", "userAgent": "UA", "cookie": []}"#).unwrap();
        assert!(load_account(&p).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
