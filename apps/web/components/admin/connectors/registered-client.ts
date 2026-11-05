/**
 * What becomes of the client the platform registered at an MCP server's
 * authorization server when the platform lets it go (ADR-173): it stays
 * there, to be withdrawn there, the platform managing no registered client.
 * Said from this one file wherever the platform lets it go, and in the
 * conditional below where it only would.
 */
export const REGISTERED_CLIENT_STAYS =
  'Le client que la plateforme avait enregistré reste chez le serveur d’autorisation : retirez-le chez lui.';

/**
 * The same, said before a change that would let it go (a new address):
 * the client still serves until then, and is not to be withdrawn yet.
 */
export const REGISTERED_CLIENT_WOULD_STAY =
  'Le client que la plateforme avait enregistré resterait alors chez le serveur d’autorisation, à retirer chez lui.';
