-- Executed once by the official postgres image when the data directory is
-- created (docker-compose). The superuser created by POSTGRES_USER owns the
-- schema and runs migrations; the application connects as mimorya_app, which
-- is subject to row-level security.
-- Development credentials only: override them for any shared environment.
CREATE ROLE mimorya_app LOGIN PASSWORD 'mimorya_app' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
CREATE EXTENSION IF NOT EXISTS citext;
CREATE EXTENSION IF NOT EXISTS vector;
