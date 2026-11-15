//! Council persistence: sessions, the responses under review, annotations and
//! human criterion ratings.

use crate::{PgConnection, Result};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct CouncilRow {
    pub id: Uuid,
    pub project_id: Uuid,
    pub title: String,
    pub question: String,
    pub status: String,
    pub decision_summary: String,
    pub decision_id: Option<Uuid>,
    pub version: i32,
    pub created_by: Uuid,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub response_count: i64,
}

pub async fn insert(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    project: Uuid,
    title: &str,
    question: &str,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO councils (id, organization_id, project_id, title, question, created_by) VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(org)
    .bind(project)
    .bind(title)
    .bind(question)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn get(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    lock: bool,
) -> Result<Option<CouncilRow>> {
    let sql = if lock {
        "SELECT c.id, c.project_id, c.title, c.question, c.status, c.decision_summary, c.decision_id, c.version,
                c.created_by, c.created_at, c.updated_at,
                (SELECT count(*) FROM council_responses x WHERE x.council_id = c.id AND x.removed_at IS NULL) AS response_count
         FROM councils c WHERE c.organization_id = $1 AND c.id = $2 FOR UPDATE OF c"
    } else {
        "SELECT c.id, c.project_id, c.title, c.question, c.status, c.decision_summary, c.decision_id, c.version,
                c.created_by, c.created_at, c.updated_at,
                (SELECT count(*) FROM council_responses x WHERE x.council_id = c.id AND x.removed_at IS NULL) AS response_count
         FROM councils c WHERE c.organization_id = $1 AND c.id = $2"
    };
    Ok(sqlx::query_as(sql)
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
) -> Result<Vec<CouncilRow>> {
    Ok(sqlx::query_as(
        "SELECT c.id, c.project_id, c.title, c.question, c.status, c.decision_summary, c.decision_id, c.version,
                c.created_by, c.created_at, c.updated_at,
                (SELECT count(*) FROM council_responses x WHERE x.council_id = c.id AND x.removed_at IS NULL) AS response_count
         FROM councils c WHERE c.organization_id = $1 AND c.project_id = $2
         ORDER BY c.updated_at DESC, c.id LIMIT $3 OFFSET $4",
    )
    .bind(org)
    .bind(project)
    .bind(limit)
    .bind(offset)
    .fetch_all(conn)
    .await?)
}

pub async fn update_details(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    title: &str,
    question: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE councils SET title = $3, question = $4, version = version + 1, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(title)
    .bind(question)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn set_status(conn: &mut PgConnection, org: Uuid, id: Uuid, status: &str) -> Result<()> {
    sqlx::query(
        "UPDATE councils SET status = $3, version = version + 1, updated_at = now() WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(status)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn record_decision(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    summary: &str,
    decision: Uuid,
) -> Result<()> {
    sqlx::query(
        "UPDATE councils SET status = 'decision_recorded', decision_summary = $3, decision_id = $4,
                version = version + 1, updated_at = now()
         WHERE organization_id = $1 AND id = $2",
    )
    .bind(org)
    .bind(id)
    .bind(summary)
    .bind(decision)
    .execute(conn)
    .await?;
    Ok(())
}

/// Bumps the version so concurrent editors notice a change to responses or reviews.
pub async fn touch(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<()> {
    sqlx::query("UPDATE councils SET version = version + 1, updated_at = now() WHERE organization_id = $1 AND id = $2")
        .bind(org)
        .bind(id)
        .execute(conn)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------- members

#[derive(Debug, Clone, FromRow)]
pub struct MemberRow {
    pub response_id: Uuid,
    pub position: i16,
    pub added_at: OffsetDateTime,
    pub removed_at: Option<OffsetDateTime>,
    pub replaced_by: Option<Uuid>,
}

pub async fn members(conn: &mut PgConnection, org: Uuid, council: Uuid) -> Result<Vec<MemberRow>> {
    Ok(sqlx::query_as(
        "SELECT response_id, position, added_at, removed_at, replaced_by FROM council_responses
         WHERE organization_id = $1 AND council_id = $2 ORDER BY position, added_at",
    )
    .bind(org)
    .bind(council)
    .fetch_all(conn)
    .await?)
}

pub async fn active_at(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
    position: i16,
) -> Result<Option<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT response_id FROM council_responses
         WHERE organization_id = $1 AND council_id = $2 AND position = $3 AND removed_at IS NULL",
    )
    .bind(org)
    .bind(council)
    .bind(position)
    .fetch_optional(conn)
    .await?)
}

pub async fn add_member(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
    response: Uuid,
    position: i16,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO council_responses (organization_id, council_id, response_id, position, added_by) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(org)
    .bind(council)
    .bind(response)
    .bind(position)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

/// Retires the active response in a position, recording what replaced it (if anything).
pub async fn retire(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
    response: Uuid,
    replaced_by: Option<Uuid>,
) -> Result<()> {
    sqlx::query(
        "UPDATE council_responses SET removed_at = now(), replaced_by = $4
         WHERE organization_id = $1 AND council_id = $2 AND response_id = $3 AND removed_at IS NULL",
    )
    .bind(org)
    .bind(council)
    .bind(response)
    .bind(replaced_by)
    .execute(conn)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------- annotations & ratings

#[derive(Debug, Clone, FromRow)]
pub struct AnnotationRow {
    pub id: Uuid,
    pub response_id: Option<Uuid>,
    pub kind: String,
    pub quote: String,
    pub body: String,
    pub author_id: Uuid,
    pub author_name: String,
    pub created_at: OffsetDateTime,
}

#[allow(clippy::too_many_arguments)]
pub async fn add_annotation(
    conn: &mut PgConnection,
    id: Uuid,
    org: Uuid,
    council: Uuid,
    response: Option<Uuid>,
    kind: &str,
    quote: &str,
    body: &str,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO council_annotations (id, organization_id, council_id, response_id, kind, quote, body, author_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(org)
    .bind(council)
    .bind(response)
    .bind(kind)
    .bind(quote)
    .bind(body)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn annotations(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
) -> Result<Vec<AnnotationRow>> {
    Ok(sqlx::query_as(
        "SELECT a.id, a.response_id, a.kind, a.quote, a.body, a.author_id,
                coalesce(u.display_name, 'Former member') AS author_name, a.created_at
         FROM council_annotations a LEFT JOIN users u ON u.id = a.author_id
         WHERE a.organization_id = $1 AND a.council_id = $2 ORDER BY a.created_at, a.id",
    )
    .bind(org)
    .bind(council)
    .fetch_all(conn)
    .await?)
}

/// Deletes an annotation written by `author`. Returns false if none matched.
pub async fn delete_annotation(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
    id: Uuid,
    author: Uuid,
) -> Result<bool> {
    Ok(sqlx::query(
        "DELETE FROM council_annotations WHERE organization_id = $1 AND council_id = $2 AND id = $3 AND author_id = $4",
    )
    .bind(org)
    .bind(council)
    .bind(id)
    .bind(author)
    .execute(conn)
    .await?
    .rows_affected()
        == 1)
}

#[derive(Debug, Clone, FromRow)]
pub struct RatingRow {
    pub response_id: Uuid,
    pub criterion: String,
    pub score: i16,
    pub reason: String,
    pub rated_by: Uuid,
    pub rated_by_name: String,
    pub updated_at: OffsetDateTime,
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_rating(
    conn: &mut PgConnection,
    org: Uuid,
    council: Uuid,
    response: Uuid,
    criterion: &str,
    score: i16,
    reason: &str,
    by: Uuid,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO council_ratings (organization_id, council_id, response_id, criterion, score, reason, rated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (council_id, response_id, criterion, rated_by)
         DO UPDATE SET score = EXCLUDED.score, reason = EXCLUDED.reason, updated_at = now()",
    )
    .bind(org)
    .bind(council)
    .bind(response)
    .bind(criterion)
    .bind(score)
    .bind(reason)
    .bind(by)
    .execute(conn)
    .await?;
    Ok(())
}

pub async fn ratings(conn: &mut PgConnection, org: Uuid, council: Uuid) -> Result<Vec<RatingRow>> {
    Ok(sqlx::query_as(
        "SELECT r.response_id, r.criterion, r.score, r.reason, r.rated_by,
                coalesce(u.display_name, 'Former member') AS rated_by_name, r.updated_at
         FROM council_ratings r LEFT JOIN users u ON u.id = r.rated_by
         WHERE r.organization_id = $1 AND r.council_id = $2 ORDER BY r.response_id, r.criterion, r.rated_by",
    )
    .bind(org)
    .bind(council)
    .fetch_all(conn)
    .await?)
}
