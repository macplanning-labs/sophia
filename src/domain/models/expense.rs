/// domain/models/expense.rs — 経費申請エンティティ
///
/// PayrollSystem から移行。社員の経費を申請・承認管理する。
/// 1申請(ヘッダー: ExpenseRequest) は複数の明細(ExpenseRequestItem)を持つ
/// （交通費の複数チケット等、1回の申請に複数の支出をまとめられるようにするため）。

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// 経費カテゴリ
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpenseCategory {
    Transportation,
    Meal,
    Supply,
    Communication,
    Entertainment,
    Travel,
    Other,
}

impl ExpenseCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Transportation => "TRANSPORTATION",
            Self::Meal => "MEAL",
            Self::Supply => "SUPPLY",
            Self::Communication => "COMMUNICATION",
            Self::Entertainment => "ENTERTAINMENT",
            Self::Travel => "TRAVEL",
            Self::Other => "OTHER",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Transportation => "交通費",
            Self::Meal => "食費",
            Self::Supply => "消耗品",
            Self::Communication => "通信費",
            Self::Entertainment => "交際費",
            Self::Travel => "旅費",
            Self::Other => "その他",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "TRANSPORTATION" => Self::Transportation,
            "MEAL" => Self::Meal,
            "SUPPLY" => Self::Supply,
            "COMMUNICATION" => Self::Communication,
            "ENTERTAINMENT" => Self::Entertainment,
            "TRAVEL" => Self::Travel,
            _ => Self::Other,
        }
    }
}

/// 経費申請ステータス
///
/// 実際に稼働しているSPA(JSON API)側は DRAFT → PENDING → APPROVED/REJECTED を使う
/// （`frontend/src/lib/status.ts`の`expense`マップと一致させる）。
/// SUBMITTED/PAIDは旧SSRハンドラ由来で、SSR版`create`は未ルーティングのため実質到達しない。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExpenseStatus {
    Draft,
    Pending,
    Submitted,
    Approved,
    Rejected,
    Paid,
}

impl ExpenseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Pending => "PENDING",
            Self::Submitted => "SUBMITTED",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Paid => "PAID",
        }
    }

    pub fn display(&self) -> &'static str {
        match self {
            Self::Draft => "下書き",
            Self::Pending => "申請中",
            Self::Submitted => "申請中",
            Self::Approved => "承認済",
            Self::Rejected => "却下",
            Self::Paid => "精算済",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "DRAFT" => Self::Draft,
            "PENDING" => Self::Pending,
            "SUBMITTED" => Self::Submitted,
            "APPROVED" => Self::Approved,
            "REJECTED" => Self::Rejected,
            "PAID" => Self::Paid,
            _ => Self::Draft,
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Draft => "bg-secondary",
            Self::Pending | Self::Submitted => "bg-primary",
            Self::Approved => "bg-success",
            Self::Rejected => "bg-danger",
            Self::Paid => "bg-dark",
        }
    }
}

/// 経費申請ヘッダー
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExpenseRequest {
    pub id: i64,
    pub employee_id: i64,
    pub status: String,
    pub total_amount: i32,
    pub approved_by_id: Option<i64>,
    pub approved_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 経費申請明細（領収書画像本体は含まない一覧・詳細表示用）
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct ExpenseRequestItem {
    pub id: i64,
    pub expense_request_id: i64,
    pub expense_date: NaiveDate,
    pub category: String,
    pub description: String,
    pub amount: i32,
    /// 領収書が添付されているか（BYTEA本体は`GET /api/expenses/items/{id}/receipt`で別途取得）
    pub has_receipt: bool,
    /// 領収書の MIME（image/jpeg | image/png | application/pdf）。未添付時は None
    pub receipt_mime: Option<String>,
    pub display_order: i32,
}

/// 明細作成・更新フォーム（JSON API用）
#[derive(Debug, Deserialize)]
pub struct ExpenseRequestItemForm {
    pub expense_date: NaiveDate,
    pub category: String,
    #[serde(default)]
    pub description: String,
    pub amount: i32,
}

/// 経費申請作成フォーム（ヘッダー + 初期明細、JSON API用）
#[derive(Debug, Deserialize)]
pub struct ExpenseRequestCreateForm {
    pub employee_id: i64,
    #[serde(default)]
    pub items: Vec<ExpenseRequestItemForm>,
}

/// 旧SSRフォーム（未ルーティング・到達不能。互換のため型のみ残す）
#[derive(Debug, Deserialize)]
pub struct ExpenseRequestForm {
    pub employee_id: i64,
    pub expense_date: NaiveDate,
    pub category: String,
    pub amount: i32,
    pub description: Option<String>,
}

/// スマホアップロード用トークン
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct MobileUploadToken {
    pub token: uuid::Uuid,
    pub expense_request_item_id: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

impl MobileUploadToken {
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.used_at.is_none() && self.expires_at > now
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_token(expires_in_minutes: i64, used: bool) -> MobileUploadToken {
        let now = Utc::now();
        MobileUploadToken {
            token: uuid::Uuid::new_v4(),
            expense_request_item_id: 1,
            created_at: now,
            expires_at: now + Duration::minutes(expires_in_minutes),
            used_at: if used { Some(now) } else { None },
        }
    }

    #[test]
    fn fresh_unused_token_is_valid() {
        let token = make_token(10, false);
        assert!(token.is_valid(Utc::now()));
    }

    #[test]
    fn used_token_is_invalid_even_if_not_expired() {
        let token = make_token(10, true);
        assert!(!token.is_valid(Utc::now()));
    }

    #[test]
    fn expired_token_is_invalid_even_if_unused() {
        let token = make_token(-1, false);
        assert!(!token.is_valid(Utc::now()));
    }

    #[test]
    fn used_and_expired_token_is_invalid() {
        let token = make_token(-1, true);
        assert!(!token.is_valid(Utc::now()));
    }

    #[test]
    fn expense_status_pending_maps_to_shinsei_chuu_display() {
        // PENDINGはSPA(JSON API)側が実際に使うステータス。DRAFTにフォールバックされて
        // 「下書き」と誤表示されるバグを回帰させないためのテスト。
        let status = ExpenseStatus::from_str("PENDING");
        assert_eq!(status, ExpenseStatus::Pending);
        assert_eq!(status.display(), "申請中");
    }
}
