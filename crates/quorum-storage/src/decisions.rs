//! Decision records and their append-only history.

use crate::{PgConnection, Result};
use quorum_domain::decision::DecisionContent;
use sqlx::FromRow;
use time::{Date, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct DecisionRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub number: i32,
    pub title: String,
    pub problem_statement: String,
    pub context: String,
    pub alternatives: String,
    pub evaluation_criteria: String,
    pub evidence: String,
    pub outcome: String,
    pub consequences: String,
    pub risks: String,
    pub decision_maker_id: Option<Uuid>,
    pub decision_maker_name: Option<String>,
    pub review_date: Option<Date>,
    pub status: String,
    pub superseded_by: Option<Uuid>,
    pub version: i32,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}

impl DecisionRow {
    pub fn content(&self) -> DecisionContent {
        DecisionContent {
            title: self.title.clone(),
            problem_statement: self.problem_statement.clone(),
            context: self.context.clone(),
            alternatives: self.alternatives.clone(),
            evaluation_criteria: self.evaluation_criteria.clone(),
            evidence: self.evidence.clone(),
            outcome: self.outcome.clone(),
            consequences: self.consequences.clone(),
            risks: self.risks.clone(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct DecisionVersionRow {
    pub version: i32,
    pub status: String,
    pub snapshot: serde_json::Value,
    pub note: String,
    pub changed_by_name: String,
    pub changed_at: OffsetDateTime,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    project: Uuid,
    content: &DecisionContent,
    decision_maker: Option<Uuid>,
    review_date: Option<Date>,
    created_by: Uuid,
) -> Result<i32> {
    // Serialize numbering per project via the project row lock the caller holds.
    let number: i32 = sqlx::query_scalar(
        "SELECT coalesce(max(number), 0) + 1 FROM decisions WHERE project_id = $1",
    )
    .bind(project)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO decisions (id, organization_id, project_id, number, title, problem_statement, context, alternatives,
                                evaluation_criteria, evidence, outcome, consequences, risks, decision_maker_id, review_date, created_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(id)
    .bind(org)
    .bind(project)
    .bind(number)
    .bind(&content.title)
    .bind(&content.problem_statement)
    .bind(&content.context)
    .bind(&content.alternatives)
    .bind(&content.evaluation_criteria)
    .bind(&content.evidence)
    .bind(&content.outcome)
    .bind(&content.consequences)
    .bind(&content.risks)
    .bind(decision_maker)
    .bind(review_date)
    .bind(created_by)
    .execute(&mut *conn)
    .await?;
    snapshot(conn, org, id, "Created", created_by).await?;
    Ok(number)
}

/// Appends the current state of the record to `decision_versions`.
pub async fn snapshot(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    note: &str,
    changed_by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO decision_versions (organization_id, decision_id, version, status, snapshot, note, changed_by)
         SELECT d.organization_id, d.id, d.version, d.status,
                jsonb_build_object(
                    'title', d.title, 'problem_statement', d.problem_statement, 'context', d.context,
                    'alternatives', d.alternatives, 'evaluation_criteria', d.evaluation_criteria,
                    'evidence', d.evidence, 'outcome', d.outcome, 'consequences', d.consequences,
                    'risks', d.risks, 'decision_maker_id', d.decision_maker_id, 'review_date', d.review_date,
                    'status', d.status, 'superseded_by', d.superseded_by),
                $3, $4
         FROM decisions d WHERE d.organization_id = $1 AND d.id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(note)
    .bind(changed_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<DecisionRow>> {
    let query = if lock {
        "SELECT d.*, u.display_name AS decision_maker_name FROM decisions d
         LEFT JOIN users u ON u.id = d.decision_maker_id
         WHERE d.organization_id = $1 AND d.id = $2 FOR UPDATE OF d"
    } else {
        "SELECT d.*, u.display_name AS decision_maker_name FROM decisions d
         LEFT JOIN users u ON u.id = d.decision_maker_id
         WHERE d.organization_id = $1 AND d.id = $2"
    };
    Ok(sqlx::query_as(query)
        .bind(org)
        .bind(id)
        .fetch_optional(conn)
        .await?)
}

pub async fn list(
    conn: &mut PgConnection,
    org: Uuid,
    project: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<DecisionRow>> {
    Ok(sqlx::query_as(
        "SELECT d.*, u.display_name AS decision_maker_name FROM decisions d
         LEFT JOIN users u ON u.id = d.decision_maker_id
         WHERE d.organization_id = $1 AND d.project_id = $2
         ORDER BY d.number DESC LIMIT $3 OFFSET $4",
    )
    .bind(org)
    .bind(project)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

/// Overwrites draft content and increments the version. The database
/// trigger refuses this for non-draft records.
pub async fn update_content(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    content: &DecisionContent,
    decision_maker: Option<Uuid>,
    review_date: Option<Date>,
) -> Result<()> {
    sqlx::query(
        "UPDATE decisions SET title = $3, problem_statement = $4, context = $5, alternatives = $6, evaluation_criteria = $7,
                evidence = $8, outcome = $9, consequences = $10, risks = $11, decision_maker_id = $12, review_date = $13,
                version = version + 1, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(&content.title)
    .bind(&content.problem_statement)
    .bind(&content.context)
    .bind(&content.alternatives)
    .bind(&content.evaluation_criteria)
    .bind(&content.evidence)
    .bind(&content.outcome)
    .bind(&content.consequences)
    .bind(&content.risks)
    .bind(decision_maker)
    .bind(review_date)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_status(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    status: &str,
    superseded_by: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "UPDATE decisions SET status = $3, superseded_by = $4, version = version + 1, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(status)
    .bind(superseded_by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn list_versions(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
) -> Result<Vec<DecisionVersionRow>> {
    Ok(sqlx::query_as(
        "SELECT v.version, v.status, v.snapshot, v.note, coalesce(u.display_name, 'Former member') AS changed_by_name, v.changed_at
         FROM decision_versions v LEFT JOIN users u ON u.id = v.changed_by
         WHERE v.organization_id = $1 AND v.decision_id = $2 ORDER BY v.version DESC",
    )
    .bind(org)
    .bind(id)
    .fetch_all(conn)
    .await?)
}
