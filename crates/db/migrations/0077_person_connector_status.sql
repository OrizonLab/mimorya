-- Servers each person authorizes (ADR-174) were marked in error by runs
-- before each person's own session existed, saying that no session opened
-- yet with the people's authorizations. Runs no longer write the status of
-- such a server: a person's session says as much of their authorization as
-- of the server. That error, no longer true, is cleared.
UPDATE connector_instances
   SET status = 'untested', last_error = NULL
 WHERE status = 'error'
   AND last_error LIKE 'Autorisation OAuth donnée par chaque personne pour elle-même : aucune session ne s’ouvre encore%';
