-- A connector is "untested" until a start succeeds or fails: after its
-- creation, a change of address or token, or its re-activation. It used to
-- read "ok" without the server ever being reached (ADR-025).
ALTER TABLE connector_instances DROP CONSTRAINT connector_instances_status_check;

ALTER TABLE connector_instances ADD CONSTRAINT connector_instances_status_check
  CHECK (status IN ('untested', 'ok', 'error', 'disabled'));

ALTER TABLE connector_instances ALTER COLUMN status SET DEFAULT 'untested';
