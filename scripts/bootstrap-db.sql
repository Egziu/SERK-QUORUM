-- Local development database bootstrap. Run once as a PostgreSQL superuser:
--   psql -v ON_ERROR_STOP=1 -f scripts/bootstrap-db.sql
-- Creates:
--   quorum_owner : owns the schema, runs migrations (CREATEDB is only needed
--                  so the integration tests can create throwaway databases).
--   quorum_api   : the login role the API uses; member of quorum_app (created
--                  by the first migration), owns nothing, cannot bypass RLS.
-- The passwords below are for local development only. Never reuse them.
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'quorum_owner') THEN
        CREATE ROLE quorum_owner LOGIN PASSWORD 'quorum_owner_dev' CREATEDB CREATEROLE;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'quorum_app') THEN
        CREATE ROLE quorum_app NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'quorum_api') THEN
        CREATE ROLE quorum_api LOGIN PASSWORD 'quorum_api_dev' NOSUPERUSER NOBYPASSRLS IN ROLE quorum_app;
    END IF;
END
$$;
SELECT 'CREATE DATABASE quorum_dev OWNER quorum_owner'
WHERE NOT EXISTS (SELECT 1 FROM pg_database WHERE datname = 'quorum_dev')\gexec
-- quorum_owner needs ADMIN on quorum_app to grant it to throwaway test login roles.
GRANT quorum_app TO quorum_owner WITH ADMIN OPTION;
