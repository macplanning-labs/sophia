/// infrastructure/repositories/employee_repo.rs — 社員(m_employee) CRUD

use anyhow::Result;
use sqlx::PgPool;

use crate::domain::models::payroll::{Employee, EmployeeForm};

// ── 共通CRUD（マクロ生成）──

impl_list_all!(list_all, Employee, "m_employee", "employee_id");
impl_find_by_id!(find_by_id, Employee, "m_employee", "id");

// ── 業務固有 ──

/// 社員登録。作成された社員のidを返す
pub async fn insert(pool: &PgPool, form: &EmployeeForm) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO m_employee (
            employee_id, last_name, first_name, last_name_kana, first_name_kana,
            employment_type, birth_date, hire_date, email,
            base_salary, position_allowance, housing_allowance, commuting_allowance,
            standard_monthly_hours, standard_remuneration, dependents_count,
            is_active
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, true)
        RETURNING id
        "#
    )
    .bind(&form.employee_id)
    .bind(&form.last_name)
    .bind(&form.first_name)
    .bind(form.last_name_kana.as_deref().unwrap_or(""))
    .bind(form.first_name_kana.as_deref().unwrap_or(""))
    .bind(form.employment_type.as_deref().unwrap_or("REGULAR"))
    .bind(form.birth_date)
    .bind(form.hire_date)
    .bind(form.email.as_deref().unwrap_or(""))
    .bind(form.base_salary.unwrap_or(0))
    .bind(form.position_allowance.unwrap_or(0))
    .bind(form.housing_allowance.unwrap_or(0))
    .bind(form.commuting_allowance.unwrap_or(0))
    .bind(form.standard_monthly_hours.unwrap_or(rust_decimal::Decimal::from(160)))
    .bind(form.standard_remuneration.unwrap_or(0))
    .bind(form.dependents_count.unwrap_or(0))
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// 社員更新
pub async fn update(pool: &PgPool, id: i64, form: &EmployeeForm) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE m_employee SET
            employee_id = $1, last_name = $2, first_name = $3,
            last_name_kana = $4, first_name_kana = $5,
            employment_type = $6, birth_date = $7, hire_date = $8, email = $9,
            base_salary = $10, position_allowance = $11, housing_allowance = $12,
            commuting_allowance = $13, standard_monthly_hours = $14,
            standard_remuneration = $15, dependents_count = $16,
            updated_at = NOW()
        WHERE id = $17
        "#
    )
    .bind(&form.employee_id)
    .bind(&form.last_name)
    .bind(&form.first_name)
    .bind(form.last_name_kana.as_deref().unwrap_or(""))
    .bind(form.first_name_kana.as_deref().unwrap_or(""))
    .bind(form.employment_type.as_deref().unwrap_or("REGULAR"))
    .bind(form.birth_date)
    .bind(form.hire_date)
    .bind(form.email.as_deref().unwrap_or(""))
    .bind(form.base_salary.unwrap_or(0))
    .bind(form.position_allowance.unwrap_or(0))
    .bind(form.housing_allowance.unwrap_or(0))
    .bind(form.commuting_allowance.unwrap_or(0))
    .bind(form.standard_monthly_hours.unwrap_or(rust_decimal::Decimal::from(160)))
    .bind(form.standard_remuneration.unwrap_or(0))
    .bind(form.dependents_count.unwrap_or(0))
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 社員を論理削除する（is_active = false）
pub async fn deactivate(pool: &PgPool, id: i64) -> Result<()> {
    sqlx::query("UPDATE m_employee SET is_active = false, updated_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
