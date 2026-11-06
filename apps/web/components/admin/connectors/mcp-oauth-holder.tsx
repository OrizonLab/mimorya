import { Badge } from '@/components/ui';
import type { McpOAuth } from '@/lib/api';
import { NO_REVOCATION, REVOCATION_ASKED } from './revocation-words';

/** How many people authorized the server for themselves: never who. */
function counted(count: number): string {
  if (count === 0) return 'Aucune personne ne l’a encore autorisé pour elle-même.';
  if (count === 1) return '1 personne l’a autorisé pour elle-même.';
  return `${count} personnes l’ont autorisé pour elles-mêmes.`;
}

/**
 * A server each person authorizes for themself (ADR-174), as its card says
 * it: how many did, whether each is verified to use their own account
 * (ADR-175), and what it means for runs and for its test.
 */
export function PersonAuthorizedState({ oauth }: { oauth: McpOAuth }) {
  return (
    <>
      <p className="flex flex-wrap items-center gap-2">
        <Badge tone="neutral">Chaque personne l’autorise</Badge>
        {oauth.verifiesIdentity && <Badge tone="ok">Compte vérifié</Badge>}
        <span>{counted(oauth.personAuthorizations)}</span>
      </p>
      {oauth.verifiesIdentity && (
        <p className="text-xs text-muted">
          Son serveur d’autorisation prouve que chacun l’autorise avec son propre compte : une
          autorisation donnée avec le compte d’une autre adresse est refusée.
        </p>
      )}
      <p className="text-xs text-muted">
        Chaque run utilise l’autorisation de la personne qu’il sert, jamais celle d’une autre ni
        celle de l’entreprise. Pour tester ce serveur, autorisez-le d’abord pour vous dans « Mon
        compte ».
      </p>
    </>
  );
}

/**
 * The change of who authorizes the server, the company or each person
 * (ADR-174): what the button says, and what is asked before it is done,
 * every authorization given then being taken, and revoked where the
 * authorization server allows it.
 */
export function holderChange(oauth: McpOAuth): {
  perPerson: boolean;
  label: string;
  question: string;
} {
  if (oauth.perPerson) {
    const revoked = oauth.revocable
      ? 'la plateforme demande au serveur d’autorisation de révoquer leurs jetons'
      : 'aucun point de révocation n’étant connu pour ce serveur d’autorisation, chacune doit aussi retirer son accord chez lui';
    const taken =
      oauth.personAuthorizations === 0
        ? 'Aucune autorisation de personne n’est à retirer.'
        : `Les autorisations des personnes (${oauth.personAuthorizations}) sont retirées ; ${revoked}.`;
    const verification = oauth.verifiesIdentity
      ? ' La vérification du compte de chacun est retirée.'
      : '';
    return {
      perPerson: false,
      label: 'Faire autoriser par l’entreprise',
      question: `Faire autoriser ce serveur par l’entreprise, une fois pour tous les runs ? ${taken}${verification} Un administrateur l’autorise ensuite ici.`,
    };
  }
  const revoked = oauth.revocable ? REVOCATION_ASKED : NO_REVOCATION;
  const taken = oauth.authorization
    ? `L’autorisation de l’entreprise est retirée ; ${revoked}.`
    : 'Aucune autorisation de l’entreprise n’est à retirer.';
  return {
    perPerson: true,
    label: 'Faire autoriser par chaque personne',
    question: `Faire autoriser ce serveur par chaque personne, pour elle-même ? ${taken} Chacune l’autorise ensuite dans « Mon compte ».`,
  };
}
