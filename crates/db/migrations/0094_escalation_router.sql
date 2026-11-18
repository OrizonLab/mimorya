-- The router of the model escalation (ADR-207, step 2).
--
-- The company's rule for each kind of run: which model it starts with, the
-- main one, the escalation one, or the one a short classification of its
-- request by the main model names. Read only while the router is on.
ALTER TABLE tenant_model_escalation
  ADD COLUMN route_conversation text NOT NULL DEFAULT 'main'
    CONSTRAINT tenant_model_escalation_route_conversation
    CHECK (route_conversation IN ('main', 'escalation', 'classification')),
  ADD COLUMN route_mission text NOT NULL DEFAULT 'main'
    CONSTRAINT tenant_model_escalation_route_mission
    CHECK (route_mission IN ('main', 'escalation', 'classification')),
  ADD COLUMN route_scheduled_task text NOT NULL DEFAULT 'main'
    CONSTRAINT tenant_model_escalation_route_scheduled_task
    CHECK (route_scheduled_task IN ('main', 'escalation', 'classification')),
  ADD COLUMN route_hook text NOT NULL DEFAULT 'main'
    CONSTRAINT tenant_model_escalation_route_hook
    CHECK (route_hook IN ('main', 'escalation', 'classification'));

-- A skill's version may ask for the company's escalation model: a run that
-- loads it goes on with that model, while the router is on and within its
-- bounds. Written by the skills' administrators with the version, it never
-- changes with it; a version written without saying, an agent's proposal
-- included, keeps the active version's.
ALTER TABLE skill_versions
  ADD COLUMN model_escalation boolean NOT NULL DEFAULT false;
