# Recette de la bêta

Les essais à faire soi-même sur l'installation de bêta (`docs/INSTALLATION.md`), avec une
vraie clé d'Anthropic, avant d'ouvrir la plateforme à son équipe puis aux entreprises
pilotes. Chaque essai dit ce qu'il faut faire et ce qui doit se passer ; notez la date et
le résultat dans la dernière colonne de votre copie.

Les commandes se lancent depuis `/opt/mimorya`, avec
`alias dc='docker compose --env-file .env.production -f compose.production.yml'`.

Avant de commencer : `dc exec api mimorya-admin check-install --mail-to vous@example.com --call-model`
ne dit aucun `ÉCHEC`, et l'e-mail d'essai est arrivé.

## 1. Comptes

| #   | Essai                                                                                                                                             | Résultat attendu                                                                                                                                                                              |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.1 | **Invitation par e-mail.** « Administration > Utilisateurs », « Créer le compte » pour une adresse que vous lisez.                                | Un e-mail d'invitation arrive, au nom de la plateforme (ou de votre marque), avec un lien valable sept jours. Le lien ouvre le choix du mot de passe, puis une session dans l'entreprise.     |
| 1.2 | Rouvrir le même lien d'invitation.                                                                                                                | Refusé : un lien ne sert qu'une fois.                                                                                                                                                         |
| 1.3 | **Mot de passe oublié.** Se déconnecter, « Mot de passe oublié ? » sur la page de connexion, saisir l'adresse.                                    | Un e-mail de réinitialisation arrive ; le lien permet de choisir un nouveau mot de passe ; l'ancien ne connecte plus. Une adresse inconnue reçoit la même réponse à l'écran, et aucun e-mail. |
| 1.4 | **Second facteur.** « Mon compte > Second facteur », « Activer le second facteur » ; scanner le QR code avec une application (Aegis, 1Password…). | L'application affiche le code sous le nom du produit. Le premier code active le second facteur et montre dix codes de secours, une seule fois. Les autres sessions du compte sont fermées.    |
| 1.5 | Se reconnecter : mot de passe, puis un code de l'application ; puis une autre fois avec un code de secours.                                       | Sans le code, aucune session. Un code de secours ne sert qu'une fois.                                                                                                                         |
| 1.6 | « Administration > Connexion » : exiger le second facteur des administrateurs.                                                                    | Un administrateur sans second facteur doit l'activer à sa prochaine connexion, avant toute session.                                                                                           |

## 2. Agent

| #   | Essai                                                                                                                                                                                                                                                                                            | Résultat attendu                                                                                                                                                                                                              |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2.1 | **Conversation simple.** « Conversations », une question générale, puis une question de suite.                                                                                                                                                                                                   | Une réponse en français, écrite au fil de l'eau ; la suite tient compte de la question précédente. « Administration > Consommation » montre les appels, au modèle principal (`claude-sonnet-5-5`), et leur coût.              |
| 2.2 | Déposer un document (« Documents ») puis poser une question à son sujet.                                                                                                                                                                                                                         | La réponse cite le document par son titre et, s'il en a, sa page.                                                                                                                                                             |
| 2.3 | **Action soumise à validation.** Déclarer la messagerie de l'entreprise (« Administration > Connecteurs », messagerie standard : serveurs IMAP et SMTP), relier son compte (« Mon compte », compte de messagerie), puis demander : « Envoie-moi un e-mail de test avec pour objet Recette 2.3 ». | Le run se met en pause ; « Validations » montre l'e-mail à envoyer, destinataire, objet et texte compris, tels qu'ils partiront. Validé, l'e-mail arrive tel quel ; refusé (un second essai), rien ne part et l'agent le dit. |
| 2.4 | **Mission.** « Missions », un objectif sur plusieurs étapes (par exemple : « Rédige une note d'une page sur nos règles de conservation des données, avec un plan puis le texte »), « Confier la mission ».                                                                                       | La mission avance par étapes, visibles dans « Missions » et « Opérations », dans ses limites ; à la fin, sa réponse est relue au regard de ses critères, et son auteur reçoit un e-mail qui annonce sa fin.                   |

## 3. Escalade de modèle

Dans « Administration > Modèle d'IA > Escalade de modèle » : modèle d'escalade
`claude-opus-5-5`, budget mensuel de 5 USD, 2 escalades au plus par run.

| #   | Essai                                                                                                                                                                              | Résultat attendu                                                                                                                                                                                                           |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3.1 | **Par le routeur.** Cocher « Routeur en amont », règle des « Missions » : « Modèle d'escalade » ; confier une mission.                                                             | Les appels de la mission vont à `claude-opus-5-5` dès le début : « Administration > Consommation » les montre au modèle d'escalade, et « Escalade de modèle » compte la dépense du mois.                                   |
| 3.2 | Règle des « Conversations » : « Selon un classement de la demande » ; poser une question simple, puis une demande longue et difficile (une analyse à plusieurs contraintes).       | La question simple reste au modèle principal ; la demande difficile part au modèle d'escalade. Chaque classement est compté dans la consommation.                                                                          |
| 3.3 | **Après échec du contrôle.** Cocher « Escalade après échec du contrôle » ; confier une mission dont les critères sont exigeants (par exemple : « au moins trois sources citées »). | Si la relecture refuse la réponse du modèle principal, la suite du run passe au modèle d'escalade, ce que « Opérations » et la consommation montrent. L'échec n'étant pas provoqué à coup sûr, refaites l'essai au besoin. |
| 3.4 | Baisser le budget sous la dépense du mois.                                                                                                                                         | Plus aucune escalade ce mois-ci : les runs restent au modèle principal, sans s'arrêter.                                                                                                                                    |

## 4. Formules de fourniture du modèle

Sur une entreprise d'essai, `essai`, créée par `create-company` (ADR-208).

| #   | Essai                                                                                                                                                                                                                                                                                                                                                          | Résultat attendu                                                                                                                                                                                                                                                                                                           |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 4.1 | **Plafond bas qui bloque.** `dc run --rm migrate mimorya-admin set-model-supply --slug essai --formula capped --cap 1` (le plus bas permis), `set-markup --slug essai --percent 100` (chaque appel compte double), `set-supply-alerts --slug essai --cap 50,100` ; puis des demandes longues (« rédige un texte de 1 500 mots sur… ») jusqu'à 1 USD refacturé. | Les administrateurs reçoivent un e-mail à 50 % du plafond, puis à 100 %, une fois chacun. Le plafond atteint, les runs s'arrêtent avec un message en français : la fourniture du mois est épuisée. « Administration > Modèle d'IA » et `model-supply` montrent la même consommation, au coût réel et au montant refacturé. |
| 4.2 | Remettre le coefficient : `set-markup --slug essai --percent 0`, et relever le plafond : `set-model-supply --slug essai --formula capped --cap 5`.                                                                                                                                                                                                             | Les runs reprennent ; le plafond changé réarme les alertes du mois.                                                                                                                                                                                                                                                        |
| 4.3 | **Crédit prépayé.** `set-model-supply --slug essai --formula prepaid`, puis `add-credit --slug essai --amount 0.20 --reference RECETTE-4.3`, et des conversations.                                                                                                                                                                                             | Le solde baisse à chaque appel (`model-supply`, et « Administration > Modèle d'IA ») ; à 20 % puis à 5 % du versement, un e-mail prévient les administrateurs ; à zéro, les runs s'arrêtent, dit en français.                                                                                                              |
| 4.4 | `add-credit --slug essai --amount 2 --reference RECETTE-4.4`.                                                                                                                                                                                                                                                                                                  | Les runs reprennent ; les alertes du crédit sont réarmées pour ce versement.                                                                                                                                                                                                                                               |
| 4.5 | `set-model-supply --slug essai --formula client-key`.                                                                                                                                                                                                                                                                                                          | L'entreprise n'a plus de modèle tant qu'elle n'a pas saisi sa propre clé ; la clé de l'exploitant n'est plus jamais appelée pour elle.                                                                                                                                                                                     |

## 5. Sauvegarde et restauration

| #   | Essai                                                                                                                                                                                                                | Résultat attendu                                                                                                                                                                                               |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5.1 | **Sauvegarde.** `COMPOSE_ENV_FILES=.env.production infra/backup/backup.sh --base`, puis `infra/backup/archive-status.sh`.                                                                                            | Un fichier `backups/mimorya-….base.tar.gz.age` (droits 600), illisible sans votre identité ; l'état de l'archive dit la dernière sauvegarde de base et le dernier WAL archivé.                                 |
| 5.2 | **Restauration sur une pile de test**, sur un autre serveur ou dans un autre dossier, avec la même clé maître : suivre `docs/RECOVERY_PLAN.md`, § 4.2 « Perte de l'hôte », avec la sauvegarde 5.1 et votre identité. | La pile restaurée démarre ; `check-install` n'y dit aucun `ÉCHEC` (hors domaine, s'il n'est pas le sien) ; les comptes se connectent ; les conversations, les documents et la formule de l'entreprise sont là. |
| 5.3 | Sur la pile restaurée : `dc exec api mimorya-admin verify-audit`.                                                                                                                                                    | Le journal d'audit de chaque entreprise se vérifie.                                                                                                                                                            |

## 6. Mise à jour de version

| #   | Essai                                                                                                | Résultat attendu                                                                                                                             |
| --- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| 6.1 | À la prochaine version, suivre `docs/INSTALLATION.md` § 11, pendant qu'une mission tourne.           | `check-install` dit les migrations à jour à la nouvelle version ; la mission reprend là où elle en était ; les sessions ouvertes le restent. |
| 6.2 | Revenir en arrière : restaurer la sauvegarde faite avant la mise à jour (5.2), à l'ancienne version. | La pile repart à l'ancienne version, avec les données d'avant la mise à jour.                                                                |

## Remonter un problème

Pour chaque problème, notez :

- **ce que vous faisiez**, l'essai de cette recette s'il y en a un, l'heure (de Paris) et
  l'entreprise ;
- **ce qui s'est passé**, le message affiché mot pour mot, et ce que vous attendiez ;
- **la version** : `grep MIMORYA_VERSION .env.production` et `git describe --tags` ;
- **la vérification** : la sortie de `dc exec api mimorya-admin check-install` ;
- **les journaux** autour de l'heure du problème :

  ```bash
  dc logs --since 30m --no-color api worker > journaux-api.txt
  dc logs --since 30m --no-color db proxy > journaux-base.txt
  ```

Les journaux de la plateforme ne contiennent ni en-têtes, ni corps de requêtes, ni
secrets : chaque requête y est une ligne (méthode, chemin, statut, durée, identifiant de
requête), et les erreurs n'y recopient aucune clé ni aucun jeton (SECURITY.md, § 7).
Relisez-les tout de même avant de les envoyer : une adresse e-mail ou un nom d'entreprise
peut y figurer. N'envoyez jamais `.env.production`, la clé maître ni une sauvegarde.
Joignez une capture d'écran si l'interface est en cause.
