# The production stack's database (ADR-034, ADR-206): PostgreSQL 16 with
# pgvector, as before, and age, with which the archive command
# (infra/postgres/archive-wal.sh, mounted by compose.production.yml)
# encrypts each WAL segment for the backups' recipients.
#   docker build -f infra/docker/db.Dockerfile -t mimorya-db .

FROM pgvector/pgvector:pg16
RUN apt-get update \
    && apt-get install -y --no-install-recommends age \
    && rm -rf /var/lib/apt/lists/*
