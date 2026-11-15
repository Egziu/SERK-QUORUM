//! Tenant isolation. A cross-tenant disclosure is a release-blocking defect,
//! so isolation is verified at two layers:
//!   1. the HTTP API (application authorization), and
//!   2. the database itself, connected as the application role, proving that
//!      row-level security holds even if application code forgets a filter.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use common::*;
use serde_json::json;
use sqlx::Row;

struct Tenants {
    a: Client,
    org_a: String,
    project_a: String,
    doc_a: String,
    decision_a: String,
    b: Client,
    org_b: String,
    project_b: String,
}

async fn two_tenants(app: &axum::Router) -> Tenants {
    let (mut a, org_a) = signup(app, "alice", "Tenant A").await;
    let project_a = create_project(&mut a, &org_a, "Confidential A").await;
    let doc_a = create_document(&mut a, &org_a, &project_a, "Secret plan A").await;
    let decision_a = create_decision(&mut a, &org_a, &project_a, "Secret decision A").await["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let (mut b, org_b) = signup(app, "bob", "Tenant B").await;
    let project_b = create_project(&mut b, &org_b, "Project B").await;
    Tenants {
        a,
        org_a,
        project_a,
        doc_a,
        decision_a,
        b,
        org_b,
        project_b,
    }
}

#[tokio::test]
async fn api_never_reveals_another_tenants_data() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let mut t = two_tenants(&app).await;
    let (oa, pa, da, ca) = (&t.org_a, &t.project_a, &t.doc_a, &t.decision_a);

    // Direct access to tenant A paths: 404 (existence not disclosed), never 200 or 403.
    for path in [
        format!("/api/v1/orgs/{oa}"),
        format!("/api/v1/orgs/{oa}/members"),
        format!("/api/v1/orgs/{oa}/projects"),
        format!("/api/v1/orgs/{oa}/projects/{pa}"),
        format!("/api/v1/orgs/{oa}/projects/{pa}/documents"),
        format!("/api/v1/orgs/{oa}/projects/{pa}/decisions"),
        format!("/api/v1/orgs/{oa}/documents/{da}"),
        format!("/api/v1/orgs/{oa}/documents/{da}/versions"),
        format!("/api/v1/orgs/{oa}/documents/{da}/versions/1"),
        format!("/api/v1/orgs/{oa}/documents/{da}/export.md"),
        format!("/api/v1/orgs/{oa}/decisions/{ca}"),
        format!("/api/v1/orgs/{oa}/decisions/{ca}/versions"),
        format!("/api/v1/orgs/{oa}/audit-events"),
        format!("/api/v1/orgs/{oa}/provider-runs"),
        format!("/api/v1/orgs/{oa}/providers"),
    ] {
        let r = t.b.get(&path).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "GET {path}: {}", r.text);
        assert!(
            !r.text.contains("Secret") && !r.text.contains("Confidential"),
            "{path} leaked content"
        );
    }

    // Confused-deputy: tenant A's resource ids under tenant B's own org path.
    let ob = &t.org_b;
    for path in [
        format!("/api/v1/orgs/{ob}/projects/{pa}"),
        format!("/api/v1/orgs/{ob}/projects/{pa}/documents"),
        format!("/api/v1/orgs/{ob}/documents/{da}"),
        format!("/api/v1/orgs/{ob}/documents/{da}/export.md"),
        format!("/api/v1/orgs/{ob}/decisions/{ca}"),
    ] {
        let r = t.b.get(&path).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "GET {path}: {}", r.text);
    }

    // Writes into tenant A are refused and change nothing.
    let writes = [
        t.b.post(&format!("/api/v1/orgs/{oa}/projects"), json!({ "name": "Injected" })).await,
        t.b.patch(&format!("/api/v1/orgs/{oa}/projects/{pa}"), json!({ "name": "Hijacked" })).await,
        t.b.post(
            &format!("/api/v1/orgs/{ob}/projects/{pa}/documents"),
            json!({ "title": "Planted", "category": "general" }),
        )
        .await,
        t.b.post(
            &format!("/api/v1/orgs/{ob}/documents/{da}/versions"),
            json!({ "base_version": 1, "title": "Overwrite", "body": "x" }),
        )
        .await,
        t.b.post(&format!("/api/v1/orgs/{ob}/documents/{da}/approve"), json!({ "version": 1 })).await,
        t.b.post(
            &format!("/api/v1/orgs/{ob}/decisions/{ca}/transition"),
            json!({ "to": "under_review", "base_version": 1 }),
        )
        .await,
        t.b.post(
            &format!("/api/v1/orgs/{oa}/invitations"),
            json!({ "email": "bob@example.test", "role": "owner" }),
        )
        .await,
        t.b.post(
            &format!("/api/v1/orgs/{ob}/provider-runs"),
            json!({ "provider": "openai", "task": "planning", "prompt": "x", "mode": "fixture", "project_id": pa }),
        )
        .await,
    ];
    for r in writes {
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{}", r.text);
    }

    // Tenant B cannot make tenant A's decision supersede into B.
    let d_b = create_decision(&mut t.b, &t.org_b, &t.project_b, "B decision").await;
    let r =
        t.b.post(
            &format!(
                "/api/v1/orgs/{ob}/decisions/{}/transition",
                d_b["id"].as_str().unwrap()
            ),
            json!({ "to": "superseded", "base_version": 1, "confirm": true, "superseded_by": ca }),
        )
        .await;
    assert_ne!(r.status, StatusCode::OK);

    // Tenant A's data is untouched, and B's listing shows only B.
    let r = t.a.get(&format!("/api/v1/orgs/{oa}/projects/{pa}")).await;
    assert_eq!(r.body["name"], "Confidential A");
    assert_eq!(r.body["document_count"], 1);
    let r = t.b.get(&format!("/api/v1/orgs/{ob}/projects")).await;
    let names: Vec<_> = r.body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].clone())
        .collect();
    assert_eq!(names, vec![json!("Project B")]);
    let r = t.b.get("/api/v1/auth/session").await;
    assert_eq!(r.body["memberships"].as_array().unwrap().len(), 1);

    // Tenant B's audit trail contains no tenant A events.
    let r =
        t.b.get(&format!("/api/v1/orgs/{ob}/audit-events?limit=100"))
            .await;
    assert!(!r.text.contains(oa.as_str()));
    db.drop().await;
}

#[tokio::test]
async fn project_names_are_unique_only_within_a_tenant() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let (mut a, org_a) = signup(&app, "alice", "A").await;
    let (mut b, org_b) = signup(&app, "bob", "B").await;
    create_project(&mut a, &org_a, "Atlas").await;
    create_project(&mut b, &org_b, "Atlas").await;
    db.drop().await;
}

/// Row-level security, checked directly as the application role.
#[tokio::test]
async fn row_level_security_enforces_isolation_in_the_database() {
    let db = TestDb::create().await;
    let app = app(&db, test_config(&[]));
    let t = two_tenants(&app).await;

    // The application role is not the owner and cannot bypass RLS.
    let row = sqlx::query(
        "SELECT current_user::text AS who, r.rolsuper, r.rolbypassrls,
                (SELECT tableowner FROM pg_tables WHERE tablename = 'projects') AS owner
         FROM pg_roles r WHERE r.rolname = current_user",
    )
    .fetch_one(&db.app_pool)
    .await
    .unwrap();
    let who: String = row.get("who");
    let owner: String = row.get("owner");
    assert_ne!(who, owner, "API must not connect as the table owner");
    assert!(!row.get::<bool, _>("rolsuper"));
    assert!(!row.get::<bool, _>("rolbypassrls"));

    // Without tenant context: nothing is visible, even with no WHERE clause.
    for table in [
        "projects",
        "documents",
        "document_versions",
        "decisions",
        "audit_events",
        "invitations",
    ] {
        let sql = match table {
            "projects" => "SELECT count(*) FROM projects",
            "documents" => "SELECT count(*) FROM documents",
            "document_versions" => "SELECT count(*) FROM document_versions",
            "decisions" => "SELECT count(*) FROM decisions",
            "audit_events" => "SELECT count(*) FROM audit_events",
            _ => "SELECT count(*) FROM invitations",
        };
        let n: i64 = sqlx::query_scalar(sql)
            .fetch_one(&db.app_pool)
            .await
            .unwrap();
        assert_eq!(n, 0, "{table} visible without tenant context");
    }
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&db.app_pool)
        .await
        .unwrap();
    assert_eq!(users, 0, "user directory is not readable without context");

    // With tenant B context, an unfiltered query returns only B's rows.
    let org_b: uuid::Uuid = t.org_b.parse().unwrap();
    let org_a: uuid::Uuid = t.org_a.parse().unwrap();
    let mut tx = db.app_pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(org_b.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let orgs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT DISTINCT organization_id FROM projects")
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    assert_eq!(orgs, vec![org_b]);
    let secret: i64 =
        sqlx::query_scalar("SELECT count(*) FROM document_versions WHERE title LIKE 'Secret%'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(secret, 0);

    // Writing a row for tenant A while in tenant B's context is rejected.
    let err = sqlx::query(
        "INSERT INTO projects (id, organization_id, name, created_by)
         SELECT $1, $2, 'Planted', created_by FROM projects LIMIT 1",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(org_a)
    .execute(&mut *tx)
    .await
    .expect_err("RLS WITH CHECK must reject");
    assert!(err.to_string().contains("row-level security"), "{err}");
    tx.rollback().await.unwrap();

    // Updates cannot reach another tenant's rows either.
    let mut tx = db.app_pool.begin().await.unwrap();
    sqlx::query("SELECT set_config('app.org_id', $1, true)")
        .bind(org_b.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let hit = sqlx::query("UPDATE projects SET name = 'Hijacked' WHERE organization_id = $1")
        .bind(org_a)
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
    assert_eq!(hit, 0);
    tx.commit().await.unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM projects WHERE organization_id = $1")
        .bind(org_a)
        .fetch_one(&db.owner_pool)
        .await
        .unwrap();
    assert_eq!(name, "Confidential A");
    db.drop().await;
}

/// Tenant context is transaction-local and cannot leak through a pooled
/// connection to the next request.
#[tokio::test]
async fn pooled_connections_do_not_leak_tenant_context() {
    let db = TestDb::create_with_pool_size(1).await;
    let app = app(&db, test_config(&[]));
    let t = two_tenants(&app).await;

    // Use the single pooled connection inside tenant A's context.
    let mut tx = quorum_storage::begin(
        &db.app_pool,
        None,
        Some(quorum_domain::ids::OrganizationId::from_uuid(
            t.org_a.parse().unwrap(),
        )),
    )
    .await
    .unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1);
    tx.commit().await.unwrap();

    // Same physical connection (pool size 1), no context: nothing visible.
    let mut conn = db.app_pool.acquire().await.unwrap();
    let setting: Option<String> =
        sqlx::query_scalar("SELECT nullif(current_setting('app.org_id', true), '')")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
    assert_eq!(setting, None, "tenant context survived the transaction");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&mut *conn)
        .await
        .unwrap();
    assert_eq!(n, 0);
    drop(conn);

    // Rolled-back transactions do not leak either.
    let tx = quorum_storage::begin(
        &db.app_pool,
        None,
        Some(quorum_domain::ids::OrganizationId::from_uuid(
            t.org_a.parse().unwrap(),
        )),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM projects")
        .fetch_one(&db.app_pool)
        .await
        .unwrap();
    assert_eq!(n, 0);

    // Interleaved API requests from both tenants over the one connection.
    let mut a = t.a.clone();
    let mut b = t.b.clone();
    for _ in 0..5 {
        let ra = a.get(&format!("/api/v1/orgs/{}/projects", t.org_a)).await;
        let rb = b.get(&format!("/api/v1/orgs/{}/projects", t.org_b)).await;
        assert_eq!(ra.body["items"][0]["name"], "Confidential A");
        assert_eq!(rb.body["items"][0]["name"], "Project B");
        assert_eq!(rb.body["items"].as_array().unwrap().len(), 1);
    }
    db.drop().await;
}
