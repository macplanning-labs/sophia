/// domain/models/mail_pipeline.rs — メール自動取込パイプライン共通型
///
/// 4フェーズ(監視・分類 / データソース取得 / 解析・登録 / 保存・通知)の
/// 受け渡しは t_received_email.status で行う（状態遷移ベースの疎結合設計）。
///
/// 状態遷移:
///   NEW ─Phase2→ FETCHED ─Phase3→ IMPORTED ─Phase4→ (drive_file_id設定で終端)
///    │              │                 │
///    └FETCH_FAILED  └PARSE_FAILED     └DRIVE_FAILED（DB登録済みなので次回Phase4で再試行のみ）
///   IGNORED / SKIPPED は終端（対象外 or 重複）
///
/// retry_count が MAX_RETRY 以上になった *_FAILED 行は needs_manual_review=TRUE となり、
/// Phase4がADMINへ集約アラートメールを送る。

use std::fmt;

/// リトライ上限（これを超えたら needs_manual_review に倒す）
pub const MAX_RETRY: i32 = 3;

/// t_received_email.status の取りうる値
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatus {
    New,
    Ignored,
    Fetched,
    FetchFailed,
    Imported,
    ParseFailed,
    DriveFailed,
    Skipped,
}

impl PipelineStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PipelineStatus::New => "NEW",
            PipelineStatus::Ignored => "IGNORED",
            PipelineStatus::Fetched => "FETCHED",
            PipelineStatus::FetchFailed => "FETCH_FAILED",
            PipelineStatus::Imported => "IMPORTED",
            PipelineStatus::ParseFailed => "PARSE_FAILED",
            PipelineStatus::DriveFailed => "DRIVE_FAILED",
            PipelineStatus::Skipped => "SKIPPED",
        }
    }
}

impl fmt::Display for PipelineStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// t_received_email.source_type の取りうる値
///
/// Phase1がメールを分類する際に付与し、Phase2がどの取得経路を使うかの分岐に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceType {
    /// m_client.edi_system_type が設定済みのクライアント宛て
    /// → Phase2はメール本文を解析せず、EDI-OASIS APIを年月ポーリングする
    EdiApi,
    /// 添付ファイル付き（PDF/Excel等）
    Attachment,
    /// 件名フィルタ非該当かつ送信元照合不可
    Ignored,
    /// 未分類（初期値・想定外パターン）
    Unknown,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceType::EdiApi => "EDI_API",
            SourceType::Attachment => "ATTACHMENT",
            SourceType::Ignored => "IGNORED",
            SourceType::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// 各フェーズの処理結果として返すエラー種別
///
/// - Transient: 一時的エラー（ネットワーク断・セッション切れ等）。retry_countを進めて次回再試行する。
/// - Permanent: 恒久的エラー（フォーマット不正・必須マスタ未登録等）。即座に needs_manual_review とする。
#[derive(Debug, Clone)]
pub enum PhaseError {
    Transient(String),
    Permanent(String),
}

impl PhaseError {
    pub fn message(&self) -> &str {
        match self {
            PhaseError::Transient(m) | PhaseError::Permanent(m) => m,
        }
    }

    pub fn is_permanent(&self) -> bool {
        matches!(self, PhaseError::Permanent(_))
    }
}

impl fmt::Display for PhaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PhaseError::Transient(m) => write!(f, "{m}"),
            PhaseError::Permanent(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for PhaseError {}

/// 指定フェーズ名を付与したエラーメッセージを組み立てる（error_message列に保存する形式）
///
/// 例: "[Phase2] OASIS認証エラー: セッション切れ"
pub fn tag_phase_error(phase: &str, err: &PhaseError) -> String {
    format!("[{phase}] {}", err.message())
}
