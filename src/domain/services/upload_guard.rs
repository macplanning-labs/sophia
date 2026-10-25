/// domain/services/upload_guard.rs — 稼働報告アップロードの受け入れ検査(解析の前に行う)
///
/// 認証なしのトークンリンクからも、ファイルが届く。細工されたファイルで解析処理に負荷をかけられないよう、
/// 解析(Excel/PDFのパース)に渡す前に、①空でない ②大きさ ③種類(拡張子と先頭バイトの一致)
/// ④Excel(ZIP)の中身が膨らみすぎない(ZIP爆弾)ことを確かめる。DEMO-000167 項目2。
///
/// ## 検査しないこと(明記)
/// - ウイルス検査は行わない(スキャナが無い)。ファイルはディスクに保存せず、メモリで解析するだけ。
/// - ZIP内の宣言サイズ(中央ディレクトリ)を信じて検査する。宣言を偽る細工は、これだけでは防げない。
///   解析側(calamine/zip)が実際に展開する量の上限は別の対策になる。

/// アップロード1件の最大サイズ(バイト)。実際の勤務表は数百KB程度。
pub const MAX_UPLOAD_BYTES: usize = 5 * 1024 * 1024;
/// Excel(ZIP)の展開後サイズの合計の上限(宣言値)
pub const MAX_UNCOMPRESSED_BYTES: u64 = 100 * 1024 * 1024;
/// Excel(ZIP)内のファイル数の上限(通常の.xlsxは数十個)
pub const MAX_ZIP_ENTRIES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadKind {
    Pdf,
    Xlsx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadRejection {
    Empty,
    TooLarge,
    /// 拡張子が対象外、または拡張子と中身が一致しない、中身が対象外
    UnsupportedType,
    /// Excelファイルの構造が壊れている、または展開後が大きすぎる・ファイルが多すぎる
    SuspiciousArchive,
}

impl UploadRejection {
    /// 画面に出す文言(内部の詳細は出さない)
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "ファイルが空です。",
            Self::TooLarge => "ファイルが大きすぎます(5MBまで)。",
            Self::UnsupportedType => "対応していないファイルです。Excel(.xlsx)またはPDFをアップロードしてください。",
            Self::SuspiciousArchive => "ファイルの内容を確認できませんでした。Excelを開いて保存し直してから、もう一度お試しください。",
        }
    }
}

/// 受け入れてよいファイルか。ファイル名が空(欠落)のときは、先頭バイトだけで種類を決める。
pub fn check_upload(bytes: &[u8], filename: &str) -> Result<UploadKind, UploadRejection> {
    if bytes.is_empty() {
        return Err(UploadRejection::Empty);
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(UploadRejection::TooLarge);
    }

    let kind = if bytes.starts_with(b"%PDF") {
        UploadKind::Pdf
    } else if bytes.starts_with(b"PK\x03\x04") {
        UploadKind::Xlsx
    } else {
        return Err(UploadRejection::UnsupportedType);
    };

    // 拡張子があれば、中身と一致していること(.pdf なのにZIP、.xlsx なのにPDF、は受け付けない)
    let lower = filename.trim().to_ascii_lowercase();
    if !lower.is_empty() {
        let ext_ok = match kind {
            UploadKind::Pdf => lower.ends_with(".pdf"),
            UploadKind::Xlsx => lower.ends_with(".xlsx") || lower.ends_with(".xlsm"),
        };
        if !ext_ok {
            return Err(UploadRejection::UnsupportedType);
        }
    }

    if kind == UploadKind::Xlsx {
        check_zip_directory(bytes)?;
    }
    Ok(kind)
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

/// ZIPの中央ディレクトリを読み、ファイル数と展開後サイズの合計(宣言値)を検査する。読めなければ拒否する。
fn check_zip_directory(bytes: &[u8]) -> Result<(), UploadRejection> {
    let bad = UploadRejection::SuspiciousArchive;

    // 末尾から「中央ディレクトリ終端(EOCD, 0x06054b50)」を探す(コメント最大65535バイト + 22)
    let tail_start = bytes.len().saturating_sub(65_535 + 22);
    let eocd = (tail_start..bytes.len().saturating_sub(21))
        .rev()
        .find(|&i| bytes[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))
        .ok_or(bad)?;

    let total_entries = u16_at(bytes, eocd + 10).ok_or(bad)? as usize;
    let cd_offset = u32_at(bytes, eocd + 16).ok_or(bad)? as usize;
    // ZIP64(件数や位置が上限値)は勤務表では使わない
    if total_entries == 0xFFFF || cd_offset == 0xFFFF_FFFF {
        return Err(bad);
    }
    if total_entries == 0 || total_entries > MAX_ZIP_ENTRIES {
        return Err(bad);
    }

    let mut pos = cd_offset;
    let mut total: u64 = 0;
    for _ in 0..total_entries {
        if bytes.get(pos..pos + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err(bad);
        }
        let uncompressed = u32_at(bytes, pos + 24).ok_or(bad)?;
        if uncompressed == 0xFFFF_FFFF {
            return Err(bad); // ZIP64
        }
        total += uncompressed as u64;
        if total > MAX_UNCOMPRESSED_BYTES {
            return Err(bad);
        }
        let name_len = u16_at(bytes, pos + 28).ok_or(bad)? as usize;
        let extra_len = u16_at(bytes, pos + 30).ok_or(bad)? as usize;
        let comment_len = u16_at(bytes, pos + 32).ok_or(bad)? as usize;
        pos = pos + 46 + name_len + extra_len + comment_len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の最小のZIP(中央ディレクトリだけ。中身の解凍はしない検査なので、これで足りる)。
    /// entries: 各ファイルの「宣言された展開後サイズ」
    fn fake_zip(entries: &[u32]) -> Vec<u8> {
        let mut out = b"PK\x03\x04".to_vec();
        out.extend_from_slice(&[0u8; 26]); // ローカルヘッダの残り(検査対象外)
        let cd_offset = out.len() as u32;
        for (i, &size) in entries.iter().enumerate() {
            let name = format!("f{i}.xml");
            let mut e = vec![0x50, 0x4b, 0x01, 0x02];
            e.extend_from_slice(&[0u8; 20]); // +4..+24
            e.extend_from_slice(&size.to_le_bytes()); // +24 uncompressed
            e.extend_from_slice(&(name.len() as u16).to_le_bytes()); // +28
            e.extend_from_slice(&[0u8; 2]); // extra
            e.extend_from_slice(&[0u8; 2]); // comment
            e.extend_from_slice(&[0u8; 12]); // +34..+46
            e.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&e);
        }
        let cd_size = (out.len() as u32) - cd_offset;
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&[0u8; 2]);
        out
    }

    #[test]
    fn 実際のxlsxファイルは通る() {
        // 偽物のZIPだけでなく、実際の(Excel互換ライブラリが作った)xlsxが、誤って拒否されないこと
        let real = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/templates/勤務表_yyyy年mm月_氏名フルネーム.xlsx"));
        assert_eq!(check_upload(real, "勤務表_2026-10.xlsx"), Ok(UploadKind::Xlsx));
        assert_eq!(check_upload(real, ""), Ok(UploadKind::Xlsx));
        // 同じ中身を .pdf と偽れば拒否
        assert_eq!(check_upload(real, "勤務表.pdf"), Err(UploadRejection::UnsupportedType));
    }

    #[test]
    fn 空は拒否() {
        assert_eq!(check_upload(&[], "a.xlsx"), Err(UploadRejection::Empty));
    }

    #[test]
    fn 大きすぎるファイルは拒否() {
        let mut big = b"%PDF-1.4".to_vec();
        big.resize(MAX_UPLOAD_BYTES + 1, 0);
        assert_eq!(check_upload(&big, "a.pdf"), Err(UploadRejection::TooLarge));
    }

    #[test]
    fn 上限ちょうどは通る() {
        let mut ok = b"%PDF-1.4".to_vec();
        ok.resize(MAX_UPLOAD_BYTES, 0);
        assert_eq!(check_upload(&ok, "a.pdf"), Ok(UploadKind::Pdf));
    }

    #[test]
    fn 正常なpdfとxlsxは通る() {
        assert_eq!(check_upload(b"%PDF-1.7 ...", "勤務表.pdf"), Ok(UploadKind::Pdf));
        assert_eq!(check_upload(&fake_zip(&[1000, 20000]), "勤務表.xlsx"), Ok(UploadKind::Xlsx));
        assert_eq!(check_upload(&fake_zip(&[10]), "A.XLSM"), Ok(UploadKind::Xlsx));
    }

    #[test]
    fn ファイル名が空でも中身で判定できる() {
        assert_eq!(check_upload(b"%PDF-1.7 ...", ""), Ok(UploadKind::Pdf));
        assert_eq!(check_upload(&fake_zip(&[10]), ""), Ok(UploadKind::Xlsx));
    }

    #[test]
    fn 対象外の中身は拒否() {
        assert_eq!(check_upload(b"MZ\x90\x00 exe", "a.xlsx"), Err(UploadRejection::UnsupportedType));
        assert_eq!(check_upload(b"<html>", "a.pdf"), Err(UploadRejection::UnsupportedType));
        // 旧Excel(.xls)のOLE形式
        assert_eq!(check_upload(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1], "a.xls"), Err(UploadRejection::UnsupportedType));
    }

    #[test]
    fn 拡張子と中身が違えば拒否() {
        assert_eq!(check_upload(&fake_zip(&[10]), "a.pdf"), Err(UploadRejection::UnsupportedType));
        assert_eq!(check_upload(b"%PDF-1.7", "a.xlsx"), Err(UploadRejection::UnsupportedType));
        assert_eq!(check_upload(b"%PDF-1.7", "a.exe"), Err(UploadRejection::UnsupportedType));
    }

    #[test]
    fn 展開後が大きすぎるzipは拒否() {
        // 1ファイル 90MB ×2 = 180MB(宣言)
        assert_eq!(
            check_upload(&fake_zip(&[90 * 1024 * 1024, 90 * 1024 * 1024]), "a.xlsx"),
            Err(UploadRejection::SuspiciousArchive)
        );
    }

    #[test]
    fn ファイル数が多すぎるzipは拒否() {
        let many = vec![1u32; MAX_ZIP_ENTRIES + 1];
        assert_eq!(check_upload(&fake_zip(&many), "a.xlsx"), Err(UploadRejection::SuspiciousArchive));
        let ok = vec![1u32; MAX_ZIP_ENTRIES];
        assert_eq!(check_upload(&fake_zip(&ok), "a.xlsx"), Ok(UploadKind::Xlsx));
    }

    #[test]
    fn 壊れたzipは拒否() {
        // ZIPの先頭だけで、終端が無い
        assert_eq!(check_upload(b"PK\x03\x04 garbage", "a.xlsx"), Err(UploadRejection::SuspiciousArchive));
        // 終端はあるが、中央ディレクトリの位置が範囲外
        let mut z = fake_zip(&[10]);
        let n = z.len();
        z[n - 6..n - 2].copy_from_slice(&0x00FF_0000u32.to_le_bytes());
        assert_eq!(check_upload(&z, "a.xlsx"), Err(UploadRejection::SuspiciousArchive));
    }

    #[test]
    fn zip64は拒否() {
        assert_eq!(
            check_upload(&fake_zip(&[0xFFFF_FFFF]), "a.xlsx"),
            Err(UploadRejection::SuspiciousArchive)
        );
    }
}
