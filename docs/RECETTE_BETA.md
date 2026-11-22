# Recette de la bêta

Les essais à faire soi-même sur l'installation de bêta (`docs/INSTALLATION.md`), avec une
vraie clé d'Anthropic, avant d'ouvrir la plateforme à son équipe puis aux entreprises
pilotes, et de nouveau à chaque version de bêta. Chaque essai dit ce qu'il faut faire et ce
qui doit se passer ; notez la date et ce que vous avez constaté dans la dernière colonne de
votre copie.

Les commandes se lancent en `root` depuis `/srv/mimorya`, la pile désignée comme au § 5 du
guide (`COMPOSE_FILE`, `COMPOSE_ENV_FILES`, `MIMORYA_BACKUP_RECIPIENTS_FILE`).

Avant de commencer,
`docker compose exec api mimorya-admin check-install --mail-to vous@example.com --call-model`
ne dit aucun `ÉCHEC`, et l'e-mail d'essai est arrivé.

## 1. Comptes

| #   | Essai                                                                                                                                             | Résultat attendu                                                                                                                                                                              | Constaté |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| 1.1 | **Invitation par e-mail.** « Administration > Utilisateurs », « Créer le compte » pour une seconde adresse que vous lisez.                        | Un e-mail d'invitation arrive, au nom de la plateforme (ou de votre marque), avec un lien valable sept jours. Le lien ouvre le choix du mot de passe, puis une session dans l'entreprise.     |          |
| 1.2 | Rouvrir le même lien d'invitation.                                                                                                                | Refusé : un lien ne sert qu'une fois.                                                                                                                                                         |          |
| 1.3 | **Mot de passe oublié.** Se déconnecter, « Mot de passe oublié ? » sur la page de connexion, saisir l'adresse du compte 1.1.                      | Un e-mail de réinitialisation arrive ; le lien permet de choisir un nouveau mot de passe ; l'ancien ne connecte plus. Une adresse inconnue reçoit la même réponse à l'écran, et aucun e-mail. |          |
| 1.4 | **Second facteur.** « Mon compte > Second facteur », « Activer le second facteur » ; scanner le QR code avec une application (Aegis, 1Password…). | L'application affiche le code sous le nom du produit. Le premier code active le second facteur et montre dix codes de secours, une seule fois. Les autres sessions du compte sont fermées.    |          |
| 1.5 | Se reconnecter : mot de passe, puis un code de l'application ; puis une autre fois avec un code de secours.                                       | Sans le code, aucune session. Un code de secours ne sert qu'une fois.                                                                                                                         |          |
| 1.6 | « Administration > Connexion » : exiger le second facteur des administrateurs.                                                                    | Un administrateur sans second facteur doit l'activer à sa prochaine connexion, avant toute session.                                                                                           |          |

## 2. Agent

| #   | Essai                                                                                                                                                                                                                                                                                                                       | Résultat attendu                                                                                                                                                                                                              | Constaté |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| 2.1 | **Conversation simple.** « Conversations », une question générale, puis une question de suite.                                                                                                                                                                                                                              | Une réponse en français, écrite au fil de l'eau ; la suite tient compte de la question précédente. « Administration > Consommation » montre les appels, au modèle principal (`claude-sonnet-5-5`), et leur coût.              |          |
| 2.2 | **Document.** Depuis votre compte, « Administration > Utilisateurs », compte 1.1, « Attribuer le rôle » : « Gestionnaire des connaissances », sur « Toute l'entreprise ». Depuis le compte 1.1, déposer un document dans « Documents », espace de l'entreprise ; puis, depuis votre compte, poser une question à son sujet. | La réponse cite le document par son titre et, s'il en a, sa page. Personne ne change ses propres droits : c'est pourquoi le rôle se donne à un autre compte.                                                                  |          |
| 2.3 | **Action soumise à validation.** Déclarer la messagerie de l'entreprise (« Administration > Connecteurs », messagerie standard : serveurs IMAP et SMTP), relier son compte (« Mon compte », compte de messagerie), puis demander : « Envoie-moi un e-mail de test avec pour objet Recette 2.3 ».                            | Le run se met en pause ; « Validations » montre l'e-mail à envoyer, destinataire, objet et texte compris, tels qu'ils partiront. Validé, l'e-mail arrive tel quel ; refusé (un second essai), rien ne part et l'agent le dit. |          |
| 2.4 | **Mission.** « Missions », un objectif sur plusieurs étapes (par exemple : « Rédige une note d'une page sur nos règles de conservation des données, avec un plan puis le texte »), « Confier la mission ».                                                                                                                  | La mission avance par étapes, visibles dans « Missions » et « Opérations », dans ses limites ; à la fin, sa réponse est relue au regard de ses critères, et son auteur reçoit un e-mail qui annonce sa fin.                   |          |

## 3. Escalade de modèle

Dans « Administration > Modèle d'IA > Escalade de modèle » : modèle d'escalade
`claude-opus-5-5`, budget mensuel de 5 USD, 10 escalades au plus par run. Chaque appel au
modèle d'escalade compte pour une : au-delà de la borne, le run revient au modèle
principal. L'escalade se voit dans « Administration > Consommation », au rôle « Escalade »,
et dans la dépense du mois que montre « Escalade de modèle ».

| #   | Essai                                                                                                                                                                                                                                                                                           | Résultat attendu                                                                                                                                                                                                                | Constaté |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| 3.1 | **Par le routeur.** Cocher « Routeur en amont », règle des « Missions » : « Modèle d'escalade » ; confier une mission.                                                                                                                                                                          | Les appels de la mission vont à `claude-opus-5-5` dès le début, dans la borne de 10 par run : la consommation les montre au rôle « Escalade », et la dépense du mois augmente.                                                  |          |
| 3.2 | Règle des « Conversations » : « Selon un classement de la demande » ; poser une question simple, puis une demande longue et difficile (une analyse à plusieurs contraintes).                                                                                                                    | La question simple reste au modèle principal ; la demande difficile part au modèle d'escalade. Chaque classement est compté dans la consommation.                                                                               |          |
| 3.3 | **Après échec du contrôle.** Remettre la règle des « Missions » sur « Modèle principal » (un run déjà escaladé ne l'est plus après échec), cocher « Escalade après échec du contrôle », confier une mission dont les critères sont exigeants (par exemple : « au moins trois sources citées »). | Si la relecture refuse la réponse du modèle principal, la suite du run passe au modèle d'escalade : la consommation montre ses appels au rôle « Escalade ». L'échec ne se provoque pas à coup sûr : refaites l'essai au besoin. |          |
| 3.4 | Une fois la dépense du mois au-delà de 1 USD, baisser le budget à 1 USD, le plus bas permis (sous la dépense, l'enregistrement est accepté ; sous 1 USD, il est refusé).                                                                                                                        | Plus aucune escalade ce mois-ci : les runs restent au modèle principal, sans s'arrêter.                                                                                                                                         |          |

## 4. Formules de fourniture du modèle

Sur une entreprise d'essai, créée pour l'occasion avec l'adresse d'un autre administrateur
(une adresse ne sert qu'à un compte sur toute la plateforme) :

```bash
docker compose exec api mimorya-admin create-company --name 'Essai' --slug essai \
  --admin-email essai@example.com --admin-name 'Administrateur Essai'
```

Les commandes de fourniture passent par le service `migrate`, qui tient le rôle
propriétaire : chacune s'écrit `docker compose run --rm migrate mimorya-admin <commande>`.

| #   | Essai                                                                                                                                                                                                                                                                                                                                            | Résultat attendu                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Constaté |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------- |
| 4.1 | **Plafond bas qui bloque.** `set-model-supply --slug essai --formula capped --cap 1` (le plus bas permis), `set-markup --slug essai --percent 100` (chaque appel compte double), `set-supply-alerts --slug essai --cap 50,100` ; puis, dans l'entreprise, des demandes longues (« rédige un texte de 1 500 mots sur… ») jusqu'à 1 USD refacturé. | Les administrateurs de l'entreprise reçoivent un e-mail quand le mois franchit 50 % du plafond, puis 100 % : un par part, la plus pressante seule si deux sont franchies d'une minute à l'autre. Le plafond atteint, les runs s'arrêtent : « Le plafond mensuel de consommation du modèle de votre entreprise est atteint : contactez votre administrateur. » `model-supply` montre la consommation au coût réel et au montant refacturé ; « Administration > Modèle d'IA » montre le seul montant refacturé, le même. |          |
| 4.2 | Remettre le coefficient (`set-markup --slug essai --percent 0`) et relever le plafond (`set-model-supply --slug essai --formula capped --cap 5`).                                                                                                                                                                                                | Les runs reprennent ; le plafond changé réarme les alertes du mois.                                                                                                                                                                                                                                                                                                                                                                                                                                                    |          |
| 4.3 | **Crédit prépayé.** `set-model-supply --slug essai --formula prepaid`, `add-credit --slug essai --amount 0.20 --reference RECETTE-4.3`, puis des conversations.                                                                                                                                                                                  | Le solde baisse à chaque appel (`model-supply`, et « Administration > Modèle d'IA ») ; sous 20 % puis 5 % du versement, un e-mail prévient les administrateurs (la part la plus pressante seule si le solde passe les deux d'une minute à l'autre) ; à zéro, les runs s'arrêtent : « Le crédit de consommation du modèle de votre entreprise est épuisé : contactez votre administrateur. »                                                                                                                            |          |
| 4.4 | `add-credit --slug essai --amount 2 --reference RECETTE-4.4`.                                                                                                                                                                                                                                                                                    | Les runs reprennent ; les alertes du crédit sont réarmées pour ce versement.                                                                                                                                                                                                                                                                                                                                                                                                                                           |          |
| 4.5 | `set-model-supply --slug essai --formula client-key`.                                                                                                                                                                                                                                                                                            | L'entreprise n'a plus de modèle tant qu'elle n'a pas saisi sa propre clé ; la clé de l'exploitant n'est plus jamais appelée pour elle.                                                                                                                                                                                                                                                                                                                                                                                 |          |

## 5. Sauvegarde et restauration

La restauration se joue **sur l'hôte de secours** (`docs/RECOVERY_PLAN.md`, § 3), jamais
sur le serveur de production ni dans un autre dossier de celui-ci : la pile porte un nom
fixe, `mimorya-production`, et une seconde copie y viserait la base de production. Seule la
base restaurée y démarre : une pile entière tournerait en même temps que la production,
avec ses tâches planifiées, ses e-mails et ses jetons.

| #   | Essai                                                                                                                                                                                 | Résultat attendu                                                                                                                                                                                                   | Constaté |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------- |
| 5.1 | **Sauvegarde**, sur le serveur : `infra/backup/backup.sh --base`, puis `infra/backup/archive-status.sh`.                                                                              | Un fichier `backups/mimorya-….base.tar.gz.age` (droits 600), illisible sans vos identités ; l'état de l'archive dit la dernière sauvegarde de base et le dernier WAL archivé.                                      |          |
| 5.2 | **Restauration**, sur l'hôte de secours, le lendemain de 5.1 : l'exercice du plan de reprise, § 6, étapes 1 à 3, depuis la copie hors site de la nuit, une de vos identités apportée. | La base restaurée démarre seule ; `psql` y montre les entreprises et leurs dernières données ; `docker compose run --rm --no-deps api mimorya-admin verify-audit` vérifie le journal d'audit de chaque entreprise. |          |
| 5.3 | Jeter la pile restaurée par la commande gardée du plan de reprise (§ 4), retirer l'identité de l'hôte de secours, noter les durées (§ 6, étapes 4 et 5).                              | L'hôte de secours ne garde ni base restaurée, ni l'archive placée pour la restauration (`/srv/mimorya/wal-archive`), ni identité ; la copie hors site et ses instantanés restent, par conception.                  |          |

## 6. Mise à jour de version

| #   | Essai                                                                                                                     | Résultat attendu                                                                                                                                 | Constaté |
| --- | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ | -------- |
| 6.1 | À la prochaine version, suivre `docs/INSTALLATION.md` § 11, pendant qu'une mission tourne.                                | `check-install` dit les migrations à jour à la nouvelle version ; la mission reprend là où elle en était ; les sessions ouvertes le restent.     |          |
| 6.2 | Revenir en arrière, comme le dit le même § 11 : version précédente, base restaurée à l'instant noté avant la mise à jour. | La pile repart à l'ancienne version, `check-install` sans `ÉCHEC`, avec les données d'avant la mise à jour ; ce qui a été fait depuis est perdu. |          |

## Remonter un problème

Pour chaque problème, notez :

- **ce que vous faisiez**, l'essai de cette recette s'il y en a un, l'heure (de Paris) et
  l'entreprise ;
- **ce qui s'est passé**, le message affiché mot pour mot, et ce que vous attendiez ;
- **la version** : `grep MIMORYA_VERSION .env.production` et `git describe --tags` ;
- **la vérification** : la sortie de `docker compose exec api mimorya-admin check-install` ;
- **les journaux** autour de l'heure du problème :

  ```bash
  docker compose logs --since 30m --no-color api worker > journaux-api.txt
  docker compose logs --since 30m --no-color db proxy > journaux-base.txt
  ```

Les journaux de la plateforme ne contiennent ni en-têtes, ni corps de requêtes, ni
secrets : chaque requête y est une ligne (méthode, chemin, statut, durée, identifiant de
requête), et les erreurs n'y recopient aucune clé ni aucun jeton (SECURITY.md, § 7).
Relisez-les tout de même avant de les envoyer : une adresse e-mail ou un nom d'entreprise
peut y figurer. N'envoyez jamais `.env.production`, la clé maître, une identité ni une
sauvegarde. Joignez une capture d'écran si l'interface est en cause.
