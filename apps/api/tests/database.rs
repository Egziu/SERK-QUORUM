//! Database-level guarantees: migrations, least-privilege grants,
//! append-only history and immutable approved decisions.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use common::*;
use serde_json::json;

#[tokio::test]
async fn migrations_are_recorded_and_idempotent() {
    let db = TestDb::create().await;
    // Re-running is a no-op; sqlx verifies stored checksums against the files.
    quorum_storage::MIGRATOR
        .run(&db.owner_pool)
        .await
        .expect("second run");
    let applied: Vec<(i64, bool)> =
        sqlx::query_as("SELECT version, success FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&db.owner_pool)
            .await
            .unwrap();
    assert_eq!(applied.len(), quorum_storage::MIGRATOR.iter().count());
    assert!(applied.iter().all(|(_, ok)| *ok));
    db.drop().await;
}

#[tokio::test]
async fn application_role_cannot_rewrite_history_or_audit() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut owner, &org, "P").await;
    create_document(&mut owner, &org, &project, "Doc").await;
    create_decision(&mut owner, &org, &project, "Dec").await;

    for sql in [
        "UPDATE audit_events SET outcome = 'success'",
        "DELETE FROM audit_events",
        "UPDATE document_versions SET body = 'tampered'",
        "DELETE FROM document_versions",
        "UPDATE decision_versions SET note = 'tampered'",
        "DELETE FROM decision_versions",
        "DELETE FROM provider_requests",
        "TRUNCATE projects",
        "DROP TABLE projects",
        "ALTER TABLE projects DISABLE ROW LEVEL SECURITY",
    ] {
        let mut tx = db.app_pool.begin().await.unwrap();
        sqlx::query("SELECT set_config('app.org_id', $1, true)")
            .bind(&org)
            .execute(&mut *tx)
            .await
            .unwrap();
        let err = sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
            .execute(&mut *tx)
            .await
            .expect_err(sql);
        let msg = err.to_string();
        assert!(
            msg.contains("permission denied") || msg.contains("must be owner"),
            "{sql}: {msg}"
        );
        tx.rollback().await.unwrap();
    }
    let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events")
        .fetch_one(&db.owner_pool)
        .await
        .unwrap();
    assert!(audit > 0);
    db.drop().await;
}

#[tokio::test]
async fn approved_decisions_are_immutable_even_to_direct_sql() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut owner, org) = signup(&app, "owner", "Org").await;
    let project = create_project(&mut owner, &org, "P").await;
    let d = create_decision(&mut owner, &org, &project, "Original").await;
    let id = d["id"].as_str().unwrap();
    let path = format!("/api/v1/orgs/{org}/decisions/{id}/transition");
    owner
        .post(&path, json!({ "to": "under_review", "base_version": 1 }))
        .await;
    let r = owner
        .post(
            &path,
            json!({ "to": "approved", "base_version": 2, "confirm": true }),
        )
        .await;
    assert!(r.status.is_success(), "{}", r.text);

    // Even the schema owner's direct UPDATE is refused by the trigger.
    let err = sqlx::query("UPDATE decisions SET outcome = 'rewritten' WHERE id = $1::uuid")
        .bind(id)
        .execute(&db.owner_pool)
        .await
        .expect_err("trigger must refuse");
    assert!(err.to_string().contains("immutable"), "{err}");

    // Cross-tenant superseded_by is rejected by the composite foreign key.
    let (mut other, other_org) = signup(&app, "other", "Other").await;
    let other_project = create_project(&mut other, &other_org, "Q").await;
    let foreign = create_decision(&mut other, &other_org, &other_project, "Foreign").await;
    let err = sqlx::query(
        "UPDATE decisions SET status = 'superseded', superseded_by = $2::uuid WHERE id = $1::uuid",
    )
    .bind(id)
    .bind(foreign["id"].as_str().unwrap())
    .execute(&db.owner_pool)
    .await
    .expect_err("foreign key must refuse");
    assert!(err.to_string().contains("foreign key"), "{err}");
    db.drop().await;
}

#[tokio::test]
async fn secrets_are_never_stored_in_plaintext() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (c, _) = signup(&app, "owner", "Org").await;
    let cookie_secret = c
        .cookie
        .as_ref()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_owned();

    let hash: String = sqlx::query_scalar("SELECT password_hash FROM users")
        .fetch_one(&db.owner_pool)
        .await
        .unwrap();
    assert!(hash.starts_with("$argon2id$"));
    assert!(!hash.contains(PASSWORD));
    let digests: Vec<Vec<u8>> = sqlx::query_scalar("SELECT token_digest FROM sessions")
        .fetch_all(&db.owner_pool)
        .await
        .unwrap();
    assert!(digests.iter().all(|d| d.len() == 32));
    assert!(
        digests
            .iter()
            .all(|d| !String::from_utf8_lossy(d).contains(&cookie_secret))
    );
    let audit_text: String =
        sqlx::query_scalar("SELECT coalesce(string_agg(details::text, ' '), '') FROM audit_events")
            .fetch_one(&db.owner_pool)
            .await
            .unwrap();
    assert!(!audit_text.contains(PASSWORD) && !audit_text.contains(&cookie_secret));
    db.drop().await;
}
