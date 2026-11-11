/**
 * The request the scripted model answers with the reference scenario
 * (`crates/api/examples/e2e_server/scenario.rs`), each run from its own
 * history, whichever tests ran before it: its follow-up planned at 13 h, or
 * at another hour once a run of the same server start had its planning
 * applied there (ADR-199).
 */
export const REFERENCE_REQUEST =
  'Fais le point sur les interventions de la semaine et planifie le suivi de celles qui ne sont pas terminées.';
