//! presentation/handlers/assignments.rs — アサインの作成と利益試算(UI刷新 2-6・2-7 / DEMO-000140・000139)
//!
//! - `POST /api/v1/assignments/preview` — 保存せずに、利益・目安との差・警告を返す(赤字でも作成は妨げない)
//! - `POST /api/v1/assignments`        — 受注契約(常に)+発注契約(パートナー要員のときだけ)を1回で作る
//!
//! どちらも管理者専用(`admin_routes`)。契約の作成は案件ウィザードと同じ関数を使う(挙動を揃える)。

use axum::{extract::State, http::StatusCode, response::{IntoResponse, Response}, Json};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use sqlx::PgPool;

use crate::domain::services::assignment::{evaluate, project_target_status, validate_period, validate_terms, TermsInput};
use crate::infrastructure::repositories::assignment_repo::{self, AssignmentError};
use crate::infrastructure::repositories::project_repo::{
    EngineerRef, WizardClientContractInput, WizardPartnerContractInput, WorkLocationRef,
};
use crate::presentation::api_response::ApiError;

fn error_response(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(ApiError { success: false, error: message.into() })).into_response()
}

fn default_settlement_type() -> String {
    "上下割".to_string()
}

#[derive(Debug, Deserialize)]
pub struct NewEngineerBody {
    pub name: String,
    #[serde(default)]
    pub name_kana: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ClientContractBody {
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default = "default_settlement_type")]
    pub settlement_type: String,
    #[serde(flatten)]
    pub terms: TermsInput,
    #[serde(default)]
    pub remarks: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PartnerContractBody {
    /// 提案元パートナー(発注契約ごとに持つ)
    pub partner_id: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
    #[serde(default = "default_settlement_type")]
    pub settlement_type: String,
    #[serde(flatten)]
    pub terms: TermsInput,
    #[serde(default)]
    pub work_location_name: Option<String>,
    #[serde(default)]
    pub remarks: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AssignmentRequest {
    pub project_id: String,
    /// "EMPLOYEE"(自社社員)| "PARTNER"(パートナー要員)
    pub staff_type: String,
    /// 既存の要員。`new_engineer` のどちらか一方
    #[serde(default)]
    pub engineer_id: Option<i64>,
    #[serde(default)]
    pub new_engineer: Option<NewEngineerBody>,
    pub client_contract: ClientContractBody,
    /// パートナー要員のときだけ必須。自社社員には渡さない
    #[serde(default)]
    pub partner_contract: Option<PartnerContractBody>,
}

/// 入力の検証(DBを使わない部分)。エラー文は画面にそのまま出せる日本語
pub fn validate_request(req: &AssignmentRequest) -> Result<(), String> {
    if req.project_id.trim().is_empty() {
        return Err("案件を指定してください".into());
    }
    match req.staff_type.as_str() {
        "EMPLOYEE" => {
            if req.partner_contract.is_some() {
                return Err("自社社員のアサインには、提案元パートナー(発注契約)を指定できません".into());
            }
        }
        "PARTNER" => match &req.partner_contract {
            None => return Err("パートナー要員のアサインには、提案元パートナー(発注契約)が必要です".into()),
            Some(pc) if pc.partner_id.trim().is_empty() => {
                return Err("提案元パートナーを指定してください".into());
            }
            Some(_) => {}
        },
        other => return Err(format!("要員の区分が不正です: {other}(EMPLOYEE か PARTNER)")),
    }
    match (&req.engineer_id, &req.new_engineer) {
        (Some(_), Some(_)) => return Err("既存の要員と新規の要員は、どちらか一方だけ指定してください".into()),
        (None, None) => return Err("要員を指定してください".into()),
        (Some(id), None) if *id <= 0 => return Err("要員の指定が不正です".into()),
        (None, Some(n)) if n.name.trim().is_empty() => return Err("要員の氏名を入力してください".into()),
        _ => {}
    }
    let cc = &req.client_contract;
    validate_period(cc.start_date, cc.end_date, "受注契約")?;
    validate_terms(&cc.terms, "受注")?;
    if let Some(pc) = &req.partner_contract {
        validate_period(pc.start_date, pc.end_date, "発注契約")?;
        validate_terms(&pc.terms, "発注")?;
    }
    Ok(())
}

fn client_input(req: &AssignmentRequest, engineer: EngineerRef) -> WizardClientContractInput {
    let cc = &req.client_contract;
    WizardClientContractInput {
        engineer,
        start_date: cc.start_date,
        end_date: cc.end_date,
        settlement_type: cc.settlement_type.clone(),
        lower_limit_hours: cc.terms.lower_limit_hours,
        upper_limit_hours: cc.terms.upper_limit_hours,
        fixed_hours: cc.terms.fixed_hours,
        base_rate: cc.terms.base_rate,
        deduction_rate: cc.terms.deduction_rate,
        overtime_rate: cc.terms.overtime_rate,
        effort: cc.terms.effort,
        mid_month_rule: None,
        billing_timing: None,
        payment_terms: None,
        report_deadline_days_before: None,
        currency: None,
        remarks: cc.remarks.clone(),
    }
}

fn partner_input(pc: &PartnerContractBody, engineer: EngineerRef) -> WizardPartnerContractInput {
    let location = pc.work_location_name.clone().unwrap_or_default();
    WizardPartnerContractInput {
        partner_id: pc.partner_id.clone(),
        engineer,
        // 作業場所は名前でそのまま保存する(マスタには追加しない)。この値は使われない
        work_location: WorkLocationRef::New { name: location },
        start_date: pc.start_date,
        end_date: pc.end_date,
        settlement_type: pc.settlement_type.clone(),
        lower_limit_hours: pc.terms.lower_limit_hours,
        upper_limit_hours: pc.terms.upper_limit_hours,
        fixed_hours: pc.terms.fixed_hours,
        base_rate: pc.terms.base_rate,
        deduction_rate: pc.terms.deduction_rate,
        overtime_rate: pc.terms.overtime_rate,
        effort: pc.terms.effort,
        mid_month_rule: None,
        kou_responsible: None,
        kou_contact: None,
        otsu_responsible: None,
        otsu_contact: None,
        work_responsible: None,
        deliverable_text: None,
        payment_condition: None,
        contract_items: None,
        remarks: pc.remarks.clone(),
        order_create_deadline_day: None,
        order_approve_deadline_days_before: None,
        report_upload_deadline_days_before: None,
        invoice_create_deadline_day: None,
        invoice_approve_deadline_day: None,
    }
}

/// アサイン作成
pub async fn api_create(State(pool): State<PgPool>, Json(req): Json<AssignmentRequest>) -> Response {
    if let Err(m) = validate_request(&req) {
        return error_response(StatusCode::BAD_REQUEST, m);
    }

    // 既存の要員は、区分(自社社員/パートナー要員)がアサインの区分と一致していること
    let engineer = match (&req.engineer_id, &req.new_engineer) {
        (Some(id), _) => {
            match assignment_repo::engineer_affiliation(&pool, *id).await {
                Ok(Some(aff)) if aff == req.staff_type => {}
                Ok(Some(aff)) => {
                    return error_response(
                        StatusCode::BAD_REQUEST,
                        format!("要員の区分({aff})が、アサインの区分({})と一致しません", req.staff_type),
                    );
                }
                Ok(None) => return error_response(StatusCode::NOT_FOUND, "要員が見つかりません"),
                Err(e) => {
                    tracing::error!("assignment: engineer lookup failed: {e:?}");
                    return error_response(StatusCode::INTERNAL_SERVER_ERROR, "アサインを作成できませんでした");
                }
            }
            EngineerRef::Existing { id: *id }
        }
        (None, Some(n)) => EngineerRef::New {
            name: n.name.trim().to_string(),
            name_kana: n.name_kana.clone(),
            affiliation_type: req.staff_type.clone(),
            // 要員の所属会社は、パートナー要員なら提案元パートナー。自社社員は無し
            partner_id: req.partner_contract.as_ref().map(|p| p.partner_id.clone()),
            email: n.email.clone(),
        },
        (None, None) => return error_response(StatusCode::BAD_REQUEST, "要員を指定してください"),
    };

    let cc = client_input(&req, clone_engineer(&engineer));
    let pc = req.partner_contract.as_ref().map(|p| partner_input(p, clone_engineer(&engineer)));
    let location = req
        .partner_contract
        .as_ref()
        .and_then(|p| p.work_location_name.clone())
        .unwrap_or_default();

    match assignment_repo::create_assignment(&pool, &req.project_id, &engineer, &cc, pc.as_ref(), &location).await {
        Ok(c) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "success": true,
                "engineer_id": c.engineer_id,
                "client_contract_id": c.client_contract_id,
                "partner_contract_id": c.partner_contract_id,
            })),
        )
            .into_response(),
        Err(AssignmentError::NotFound(m)) => error_response(StatusCode::NOT_FOUND, m),
        Err(AssignmentError::Duplicate(m)) => error_response(StatusCode::CONFLICT, m),
        Err(AssignmentError::Other(e)) => {
            tracing::error!("assignment create failed: {e:?}");
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "アサインを作成できませんでした。入力を確認して、もう一度お試しください")
        }
    }
}

fn clone_engineer(e: &EngineerRef) -> EngineerRef {
    match e {
        EngineerRef::Existing { id } => EngineerRef::Existing { id: *id },
        EngineerRef::New { name, name_kana, affiliation_type, partner_id, email } => EngineerRef::New {
            name: name.clone(),
            name_kana: name_kana.clone(),
            affiliation_type: affiliation_type.clone(),
            partner_id: partner_id.clone(),
            email: email.clone(),
        },
    }
}

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub project_id: String,
    /// 既存の要員(前回単価との比較に使う)。新規の要員なら省略
    #[serde(default)]
    pub engineer_id: Option<i64>,
    pub client_contract: TermsInput,
    /// パートナー要員のときだけ。自社社員は省略(原価が分からないため粗利は出ない)
    #[serde(default)]
    pub partner_contract: Option<TermsInput>,
}

/// 利益の試算(保存しない)
pub async fn api_preview(State(pool): State<PgPool>, Json(req): Json<PreviewRequest>) -> Response {
    if let Err(m) = validate_terms(&req.client_contract, "受注") {
        return error_response(StatusCode::BAD_REQUEST, m);
    }
    if let Some(p) = &req.partner_contract {
        if let Err(m) = validate_terms(p, "発注") {
            return error_response(StatusCode::BAD_REQUEST, m);
        }
    }

    let ctx = match assignment_repo::project_profit_context(&pool, &req.project_id).await {
        Ok(Some(c)) => c,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, format!("案件 {} が見つかりません", req.project_id)),
        Err(e) => {
            tracing::error!("assignment preview: context failed: {e:?}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "利益を試算できませんでした");
        }
    };
    let targets = assignment_repo::margin_targets(&pool).await.unwrap_or((20, 8));
    let prev = match req.engineer_id {
        Some(id) => assignment_repo::previous_monthly_price(&pool, id).await.ok().flatten(),
        None => None,
    };

    let preview = evaluate(
        &req.client_contract,
        req.partner_contract.as_ref(),
        ctx.commercial_flow.as_deref(),
        targets,
        prev,
        (ctx.revenue, ctx.cost),
    );
    Json(preview).into_response()
}

/// 案件カード用の一覧(有効な案件・テスト案件を除く)。月額の売上・原価・粗利・粗利率と、目安との比較
pub async fn api_overview(State(pool): State<PgPool>) -> Response {
    let rows = match assignment_repo::project_overview(&pool).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("assignment overview failed: {e:?}");
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, "案件の一覧を取得できませんでした");
        }
    };
    let targets = assignment_repo::margin_targets(&pool).await.unwrap_or((20, 8));
    let projects: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            let (margin, target_pct, diff_pct, status) =
                project_target_status(r.commercial_flow.as_deref(), r.revenue, r.cost, targets);
            serde_json::json!({
                "project_id": r.project_id,
                "project_name": r.project_name,
                "client_id": r.client_id,
                "client_name": r.client_name,
                "commercial_flow": r.commercial_flow,
                "assignment_count": r.assignment_count,
                "internal_count": r.internal_count,
                "revenue": r.revenue,
                "cost": r.cost,
                "gross_profit": r.revenue - r.cost,
                "margin_pct": margin,
                "target_pct": target_pct,
                "diff_pct": diff_pct,
                "status": status,
            })
        })
        .collect();
    Json(serde_json::json!({ "projects": projects, "targets": { "direct": targets.0, "subcontract": targets.1 } })).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms() -> TermsInput {
        TermsInput {
            base_rate: 700_000,
            effort: Decimal::ONE,
            lower_limit_hours: Decimal::from(140),
            upper_limit_hours: Decimal::from(180),
            fixed_hours: None,
            deduction_rate: 0,
            overtime_rate: 0,
        }
    }

    fn d(m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, m, day).unwrap()
    }

    fn base(staff: &str, with_partner: bool) -> AssignmentRequest {
        AssignmentRequest {
            project_id: "PRJ00000001".into(),
            staff_type: staff.into(),
            engineer_id: Some(1),
            new_engineer: None,
            client_contract: ClientContractBody {
                start_date: d(11, 1),
                end_date: d(12, 31),
                settlement_type: "上下割".into(),
                terms: terms(),
                remarks: None,
            },
            partner_contract: with_partner.then(|| PartnerContractBody {
                partner_id: "P0000000001".into(),
                start_date: d(11, 1),
                end_date: d(12, 31),
                settlement_type: "上下割".into(),
                terms: terms(),
                work_location_name: None,
                remarks: None,
            }),
        }
    }

    #[test]
    fn valid_requests_pass() {
        assert!(validate_request(&base("EMPLOYEE", false)).is_ok());
        assert!(validate_request(&base("PARTNER", true)).is_ok());
    }

    #[test]
    fn employee_must_not_have_a_partner_contract() {
        assert!(validate_request(&base("EMPLOYEE", true)).unwrap_err().contains("自社社員"));
    }

    #[test]
    fn partner_staff_needs_a_proposing_partner() {
        assert!(validate_request(&base("PARTNER", false)).unwrap_err().contains("提案元パートナー"));
        let mut r = base("PARTNER", true);
        r.partner_contract.as_mut().unwrap().partner_id = "  ".into();
        assert!(validate_request(&r).is_err());
    }

    #[test]
    fn staff_type_and_engineer_are_checked() {
        assert!(validate_request(&base("OTHER", false)).is_err());
        let mut r = base("EMPLOYEE", false);
        r.engineer_id = None;
        assert!(validate_request(&r).unwrap_err().contains("要員を指定"));
        r.engineer_id = Some(1);
        r.new_engineer = Some(NewEngineerBody { name: "山田".into(), name_kana: None, email: None });
        assert!(validate_request(&r).unwrap_err().contains("どちらか一方"));
        r.engineer_id = None;
        r.new_engineer = Some(NewEngineerBody { name: "  ".into(), name_kana: None, email: None });
        assert!(validate_request(&r).unwrap_err().contains("氏名"));
    }

    #[test]
    fn dates_and_terms_are_checked() {
        let mut r = base("EMPLOYEE", false);
        r.client_contract.end_date = d(10, 31);
        assert!(validate_request(&r).unwrap_err().contains("終了日"));
        let mut r = base("PARTNER", true);
        r.partner_contract.as_mut().unwrap().terms.base_rate = 0;
        assert!(validate_request(&r).unwrap_err().contains("発注"));
        let mut r = base("EMPLOYEE", false);
        r.client_contract.terms.lower_limit_hours = Decimal::from(200);
        assert!(validate_request(&r).unwrap_err().contains("精算幅"));
    }

    #[test]
    fn request_json_with_flattened_terms_deserializes() {
        let j = serde_json::json!({
            "project_id": "PRJ00000001", "staff_type": "PARTNER", "engineer_id": 7,
            "client_contract": {"start_date":"2026-11-01","end_date":"2027-03-31","base_rate":700000,
                                 "lower_limit_hours":140,"upper_limit_hours":180},
            "partner_contract": {"partner_id":"P0000000001","start_date":"2026-11-01","end_date":"2027-03-31",
                                  "base_rate":640000,"lower_limit_hours":140,"upper_limit_hours":180}
        });
        let r: AssignmentRequest = serde_json::from_value(j).expect("flatten + 既定値で読める");
        assert_eq!(r.client_contract.terms.base_rate, 700_000);
        assert_eq!(r.client_contract.terms.effort, Decimal::ONE, "工数の既定は1人月");
        assert_eq!(r.client_contract.settlement_type, "上下割");
        assert!(validate_request(&r).is_ok());
    }
}
