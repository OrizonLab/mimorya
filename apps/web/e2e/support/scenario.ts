/**
 * The request the scripted model answers with the reference scenario
 * (`crates/api/examples/e2e_server/scenario.rs`): one scenario is queued per
 * test that sends it, in the order the files run, after the one the run the
 * server leaves interrupted plays again at startup (`interrupted-run.spec.ts`).
 */
export const REFERENCE_REQUEST =
  'Fais le point sur les interventions de la semaine et planifie le suivi de celles qui ne sont pas terminées.';
