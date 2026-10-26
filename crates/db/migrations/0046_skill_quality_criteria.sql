-- Quality criteria of a skill's version (ADR-106): what any answer that
-- follows it must meet. Written with the version, they never change with
-- it; a run that loads the version has its answer reviewed on them. An
-- agent's proposal carries none.
ALTER TABLE skill_versions
  ADD COLUMN quality_criteria text[] NOT NULL DEFAULT '{}'
  CONSTRAINT skill_versions_quality_criteria_count CHECK (cardinality(quality_criteria) <= 8);
