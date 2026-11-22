//! The help of `mimorya-admin`, in French: each command and its options.

pub const USAGE: &str = "\
Usage : mimorya-admin <commande> [options]

Commandes :
  migrate          applique les migrations de la base (DATABASE_URL, rôle propriétaire),
                   sans aucune donnée
  create-company   crée une entreprise et son premier administrateur, invité par un lien
                     --name <nom>  --slug <sigle>  --admin-email <adresse>  --admin-name <nom>
  invite           nouveau lien d’invitation d’un compte qui n’a pas encore choisi
                   son mot de passe
                     --email <adresse>
  suspend-company  suspend une entreprise : plus personne ne s’y connecte, ses sessions et
                   ses jetons sont refusés, ses runs s’arrêtent à leur prochaine étape ;
                   la commande ne supprime rien, mais les échéances des validations et la
                   rétention courent toujours
                     --slug <sigle>
  reactivate-company
                   réactive une entreprise suspendue, moins ce que ces échéances ont
                   retiré entre-temps
                     --slug <sigle>
  release-company  retire chez leurs fournisseurs ce que la plateforme y a déclaré pour
                   une entreprise suspendue : révoque les autorisations OAuth de ses
                   serveurs MCP, retire ses bots Telegram et leur webhook, révoque
                   chez Google les accords des comptes Google reliés par ses
                   personnes (réactivée, elles les relient de nouveau) ; avant
                   delete-company, dans le service api (MIMORYA_MASTER_KEY)
                     --slug <sigle>
  delete-company   supprime une entreprise suspendue et tout ce qu’elle garde, son journal
                   d’audit compris, sans retour (DATABASE_URL, rôle propriétaire) ; sans
                   --confirm, montre ce qu’elle garde et ne supprime rien
                     --slug <sigle>  [--confirm <sigle>]
  verify-audit     vérifie le journal d’audit scellé de chaque entreprise (MIMORYA_MASTER_KEY) ;
                   échoue si une entrée scellée a été modifiée ou supprimée, ou si une
                   chaîne ne porte plus un sceau relevé dans les journaux (« Audit log
                   sealed », « Audit chain found ») : seul ce sceau montre des entrées
                   retirées de la fin d’une chaîne, ou des sceaux posés à nouveau
                     [--anchor <entreprise>:<place>:<sceau>]…   (autant que voulu ;
                     l’entreprise par son identifiant, tel que journalisé, ou son sigle)
  model-supply     formule de fourniture du modèle d’une entreprise (ADR-208) : formule,
                   plafond, coefficient, consommation du mois sur la clé de l’exploitant
                   (coût réel et montant refacturé), versements du crédit et solde,
                   parts qui alertent les administrateurs
                     --slug <sigle>
  set-model-supply choisit la formule : client-key (la clé de l’entreprise), capped (la
                   clé de l’exploitant dans un plafond mensuel, 100 USD sans --cap, sans
                   plafond seulement avec --cap unlimited), prepaid (la clé de
                   l’exploitant contre un crédit prépayé)
                     --slug <sigle>  --formula <formule>  [--cap <USD>|unlimited]
  set-markup       coefficient de refacturation des appels sur la clé de l’exploitant,
                   de 0 à 100 % ; le coût réel n’en est jamais changé
                     --slug <sigle>  --percent <taux>
  add-credit       ajoute un versement au crédit prépayé, avec sa référence (numéro de
                   facture) ; une ligne n’est jamais modifiée : une correction est un
                   versement de plus, négatif s’il le faut
                     --slug <sigle>  --amount <USD>  --reference <facture>
  set-supply-alerts parts qui alertent les administrateurs de l’entreprise par e-mail
                   (ADR-211) : du plafond atteintes (de 1 à 100 %, 80 et 95 par défaut),
                   du dernier versement restantes (de 0 à 99 %, 20 et 5 par défaut) ;
                   au plus cinq de chaque, « aucun » pour n’en garder aucune
                     --slug <sigle>  [--cap <parts>]  [--credit <parts>]   (l’une au moins,
                     par exemple --cap 80,95,100)
  check-install    dit ce qui est prêt et ce qui manque : base et migrations, clé maître,
                   domaine et certificat, relais SMTP, archive des WAL, workers, marque,
                   modèle de la plateforme ; échoue si un point empêche de servir. Dans
                   le service api : docker compose … exec api mimorya-admin check-install
                     [--mail-to <adresse>]   envoie un e-mail d’essai par le relais
                     [--call-model]          appelle vraiment le modèle de la plateforme
                                             (quelques jetons, facturés)
  help             affiche cette aide

create-company, invite, suspend-company, reactivate-company, release-company,
verify-audit et check-install passent par DATABASE_APP_URL ; les liens mènent à WEB_ORIGIN.
delete-company, model-supply, set-model-supply, set-markup, add-credit et
set-supply-alerts passent par DATABASE_URL (rôle propriétaire) : chaque changement est
tracé dans le journal d’audit de l’entreprise.";
