-- Evaluation cases of a skill (ADR-103): requests the skills'
-- administrators write, each with the criteria a good answer meets, on
-- which a version is tried before it is made active (lot 4b runs them).
-- They replace `skill_evals`, reserved by the initial schema and never
-- written. A case goes with its skill; its author leaving, it stays the
-- company's.
DROP TABLE skill_evals;

CREATE TABLE skill_eval_cases (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
  skill_id uuid NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  name text NOT NULL CHECK (char_length(name) BETWEEN 2 AND 80),
  request text NOT NULL CHECK (char_length(request) BETWEEN 1 AND 4000),
  criteria text[] NOT NULL
    CHECK (cardinality(criteria) BETWEEN 1 AND 8 AND array_position(criteria, NULL) IS NULL),
  created_by_user_id uuid REFERENCES users(id) ON DELETE SET NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE (skill_id, name)
);

ALTER TABLE skill_eval_cases ENABLE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation ON skill_eval_cases AS PERMISSIVE FOR ALL TO public
  USING (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid)
  WITH CHECK (tenant_id = nullif(current_setting('app.tenant_id', true), '')::uuid);

GRANT SELECT, INSERT, UPDATE, DELETE ON skill_eval_cases TO mimorya_app;
