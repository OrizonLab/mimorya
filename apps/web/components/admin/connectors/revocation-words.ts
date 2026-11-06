/**
 * What becomes of the tokens the platform lets go of (ADR-172), as the
 * confirmations say it: revoked at the authorization server when it names a
 * revocation endpoint, otherwise the agreement to withdraw there by hand.
 */
export const REVOCATION_ASKED =
  'la plateforme demande au serveur d’autorisation de révoquer ses jetons';

export const NO_REVOCATION =
  'aucun point de révocation n’étant connu pour ce serveur d’autorisation, retirez aussi l’accord chez lui';
