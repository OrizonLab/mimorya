-- What the agent proposes of the company's skills (ADR-100): a version
-- written with the person it serves, whose text they validated, recorded
-- as a candidate for the skills' administrators, who make it active or
-- reject it. `proposed` keeps where a version came from once decided; its
-- run and its author are named as any version's are, and forgotten with
-- them.
ALTER TABLE skill_versions ADD COLUMN proposed boolean NOT NULL DEFAULT false;

-- The proposals awaiting the administrators, counted for each company.
CREATE INDEX skill_versions_pending_proposals_idx ON skill_versions (tenant_id)
  WHERE proposed AND status = 'candidate';

-- The platform's roles of the companies already there gain what the new
-- ones are created with: every member proposes skills, a knowledge manager
-- too.
UPDATE roles SET permissions = permissions || ARRAY['skill:propose']
  WHERE is_system AND key IN ('member', 'knowledge_manager')
    AND NOT 'skill:propose' = ANY (permissions);
