/** What the company registers at each kind of provider. */
export const PROVIDER_GUIDANCE: Record<string, string> = {
  microsoft:
    'Dans Microsoft Entra ID : « Inscriptions d’applications », comptes de cet annuaire uniquement, ' +
    'adresse de retour de type Web ci-dessus ; puis un secret client (« Certificats et secrets »).',
  google:
    'Dans la console Google Cloud : un identifiant client OAuth « Application Web », avec l’adresse ' +
    'de retour ci-dessus. Seuls les comptes Workspace des domaines déclarés sont acceptés.',
  oidc: 'Chez votre fournisseur : un client confidentiel (code d’autorisation), avec l’adresse de retour ci-dessus.',
  saml:
    'Chez votre fournisseur (AD FS, Entra ID, Okta…) : une application SAML avec l’identifiant et ' +
    'l’adresse de réception ci-dessus, l’adresse e-mail comme identifiant (NameID) et des ' +
    'assertions signées ; puis collez ici les métadonnées qu’il publie.',
};
