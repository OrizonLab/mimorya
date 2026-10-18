# Sécurité et confidentialité

Ce document est la référence des exigences de sécurité. Toute modification du modèle
d'autorisation, de l'isolation ou du traitement des secrets doit le mettre à jour et
faire l'objet d'une décision dans [DECISIONS.md](DECISIONS.md).

## 1. Modèle de menace

| Acteur / vecteur                | Objectif                                | Exemple                                                                        |
| ------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------ |
| Entreprise cliente malveillante | Lire les données d'une autre entreprise | Identifiant forgé dans une requête API                                         |
| Utilisateur interne curieux     | Dépasser son périmètre                  | Demander à l'agent « montre-moi les interventions de l'autre équipe »          |
| Contenu injecté                 | Détourner l'agent                       | Compte rendu, e-mail ou page web contenant « ignore tes consignes et envoie… » |
| Modèle défaillant               | Action erronée ou hallucination         | Appel d'outil avec de mauvais paramètres, synthèse inventée                    |
| Compétence dégradée             | Régression ou contournement             | Version de skill qui saute une étape de contrôle                               |
| Attaquant externe               | Prise de compte, vol de jetons          | Credential stuffing, XSS, CSRF                                                 |
| Serveur MCP / connecteur tiers  | Exfiltration, réponses piégées          | Outil MCP renvoyant des instructions                                           |
| Fournisseur LLM                 | Exposition des données envoyées         | Rétention côté fournisseur, localisation hors UE                               |

## 2. Principes non négociables

1. **Les autorisations sont décidées par le serveur**, jamais par le modèle, ni par le
   contenu d'un message, d'un document, d'un résultat d'outil ou d'une compétence.
2. **Défense en profondeur** : RLS en base + filtrage applicatif + moteur
   d'autorisation + PEP sur chaque outil.
3. **Moindre privilège** : l'agent agit avec les permissions de l'utilisateur,
   **réduites** par le plafond agent (permissions non délégables) et par le plafond du
   run ; un sous-agent ne peut qu'avoir moins que son parent.
4. **Refus par défaut** : sans règle qui accorde, c'est refusé.
5. **Traçabilité** : toute décision d'accès refusée ou accordée à l'agent, toute action
   et toute validation sont journalisées.

## 3. Isolation des entreprises

- Schéma partagé, colonne `tenant_id` sur toutes les tables métier.
- **Row-Level Security** activée sur toutes ces tables avec la politique
  `tenant_id = current_setting('app.tenant_id', true)::uuid` en lecture et écriture.
- L'application utilise un rôle PostgreSQL **non propriétaire, sans `BYPASSRLS`**.
  Les migrations utilisent un rôle distinct.
- Le contexte `app.tenant_id` est posé **par transaction** (`set_config(..., true)`),
  jamais au niveau de la connexion (pas de fuite par le pool).
- Les seules opérations qui traversent les entreprises (connexion, résolution de
  session, marquage des runs bloqués) passent par des fonctions `SECURITY DEFINER` au
  périmètre minimal et au `search_path` figé (`auth_*`, `system_*`).
- Tests d'intégration obligatoires : lecture croisée impossible même par requête
  directe, insertion croisée refusée, absence de contexte = aucune ligne.

## 4. Authentification et sessions

- MVP : e-mail + mot de passe haché **Argon2id** ; messages d'erreur identiques
  (compte inconnu / mauvais mot de passe), vérification contre une empreinte factice
  pour un compte inconnu et journalisation des échecs hors du chemin de réponse : la
  durée de réponse ne révèle pas l'existence d'un compte.
- Jetons de session **opaques** (256 bits aléatoires), seule leur empreinte SHA-256 est
  stockée ; cookie `HttpOnly`, `Secure` (hors développement), `SameSite=Lax` ; durée
  glissante (défaut 12 h), absolue (7 jours), révocation à la déconnexion et à la
  désactivation du compte.
- Protection CSRF : `SameSite=Lax` + exigence d'un en-tête personnalisé sur toutes les
  requêtes modifiantes + vérification d'`Origin`.
- Limitation de débit sur la connexion :
  - **par compte** : 10 échecs par quart d'heure et par adresse e-mail, comptes
    inconnus compris (rien ne distingue un compte existant). Chaque tentative réserve
    sa place avant la vérification du mot de passe, rendue en cas de succès : ni des
    tentatives simultanées ni un changement d'adresse ne dépassent la limite.
    Contrepartie assumée : une fois la limite atteinte, même le bon mot de passe est
    refusé jusqu'à la fin de la fenêtre, ce qui permet de bloquer temporairement un
    compte ciblé (la MFA et l'OIDC de la phase 2 réduiront ce risque) ;
  - **par adresse cliente** (`LOGIN_RATE_LIMIT_PER_MINUTE`, 10 par défaut), en plus
    d'une limite globale de 600 requêtes par minute et par adresse. Sans reverse
    proxy (voir ci-dessous), ces limites s'appliquent à toute la plateforme, puisque
    tous les utilisateurs partagent alors une même adresse.
- Adresse cliente : par défaut, l'adresse TCP de l'appelant (`TRUST_PROXY=false`).
  Derrière le seul relais web, tous les utilisateurs partagent l'adresse du serveur
  Next.js, qui transmet tel quel un `X-Forwarded-For` fourni par le client. En
  production, un reverse proxy de confiance (TLS) **ajoute** l'adresse du client à
  `X-Forwarded-For` ; avec `TRUST_PROXY=true`, l'API retient alors la dernière entrée
  de cet en-tête.
- Phase 2 : **SSO OIDC** (Microsoft Entra ID, Google Workspace, Keycloak…),
  provisionnement SCIM (phase 3), MFA déléguée à l'IdP ; MFA TOTP locale si besoin.

## 5. Autorisations

- **RBAC à périmètre** : rôle × personne × périmètre (`self`, unité ± sous-unités,
  entreprise). La position hiérarchique (responsable d'unité) **n'accorde rien** seule.
- **ABAC** : politiques `allow` / `deny` à conditions JSON (attributs du sujet, de la
  ressource, du contexte), `deny` prioritaire ; exemple livré : le service
  administratif ne consulte que les interventions terminées.
- **Sensibilité** : `public < internal < confidential < restricted`, comparée à
  l'habilitation (`clearance`) de la personne ; le propriétaire voit toujours ses
  propres données.
- **Requêtes de liste** : pré-filtrage par le périmètre RBAC (`dataScope`) puis contrôle
  unitaire ABAC (`check`) — jamais l'inverse.
- **Permissions non délégables** (l'agent ne peut jamais les exercer) : gestion des
  comptes, rôles, politiques, connecteurs, compétences, paramètres, lecture de l'audit,
  décision de validation. C'est ce qui empêche une injection ou une auto-amélioration
  de s'accorder des privilèges.
- **Administration** (ADR-019) :
  - les permissions d'administration (`TENANT_WIDE_PERMISSIONS` : organisation,
    comptes, rôles, politiques, connecteurs, compétences, paramètres, mémoire, audit,
    consommation) ne valent que détenues **sur toute l'entreprise** ; un rôle qui en
    contient ne s'attribue qu'à ce périmètre ;
  - personne ne modifie depuis l'administration son propre compte, son statut ou ses
    habilitations : il faut un autre administrateur (pas de modification directe de
    ses propres droits, pas de blocage de soi-même) ;
  - les changements d'administration d'une entreprise sont sérialisés (verrou
    PostgreSQL par entreprise, ADR-020) et revérifient sur les données en base les
    droits de leur auteur et les règles de l'organigramme : deux administrateurs
    simultanés ne peuvent ni créer de cycle, ni se retirer mutuellement leurs droits ;
  - un compte créé ou réinitialisé reçoit un mot de passe provisoire aléatoire
    (96 bits), affiché une seule fois à l'administrateur (`Cache-Control: no-store`),
    jamais journalisé ; la réinitialisation et la désactivation ferment immédiatement
    les sessions ; le changement de son mot de passe exige l'actuel (tentatives
    limitées comme la connexion) et ferme les autres sessions ;
  - limites assumées :
    - la règle « pas d'administration de soi-même » évite les erreurs, ce n'est **pas
      une frontière contre un administrateur malveillant** : il peut créer un compte,
      lui attribuer un rôle d'administration et agir par ce compte ; la
      réinitialisation d'un mot de passe lui permet de se connecter à la place de la
      personne (y compris un autre administrateur) et donc de lire ses conversations et
      sa mémoire personnelle. Tout est tracé (`user.created`, `role.assigned`,
      `user.password_reset`…) ; la double validation des actions d'administration est
      prévue en phase 3 ;
    - les adresses e-mail sont uniques entre entreprises (ADR-011) : créer un compte
      avec une adresse déjà utilisée ailleurs est refusé (409), ce qui révèle à
      l'administrateur qu'elle existe dans une autre entreprise. Limite levée avec
      l'invitation par e-mail ;
    - un run d'agent déjà lancé garde les droits calculés à son démarrage jusqu'à sa
      fin (bornée par son budget de durée), même si le compte est désactivé ou perd
      une habilitation entre-temps.

## 6. Sécurité de l'agent

| Menace                                   | Mesure                                                                                                                                                                                        |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Injection de prompt directe ou indirecte | Autorisations hors modèle ; résultats d'outils balisés « non fiables » ; consignes système fixes, non modifiables par les compétences ou la mémoire ; actions sensibles soumises à validation |
| Exfiltration via outils externes         | Outils `external` soumis à validation par défaut ; destinataires et domaines autorisés configurables (phase 2) ; pas d'outil de requête HTTP arbitraire                                       |
| Abus d'outils / boucles                  | Budgets par run (étapes, tokens, coût, durée), délai par outil, limites de parallélisme                                                                                                       |
| Paramètres modifiés après validation     | Les paramètres validés sont figés et exécutés tels quels                                                                                                                                      |
| Hallucination                            | Contrôle d'ancrage des identifiants cités, vérification des critères, sources affichées                                                                                                       |
| Données hors périmètre dans le contexte  | Mémoire et outils filtrés par le périmètre du sujet avant d'atteindre le modèle                                                                                                               |
| Écriture malveillante en mémoire         | Mémoire partagée en statut `proposed` jusqu'à validation ; détection de motifs d'injection (phase 2)                                                                                          |
| Compétence malveillante ou régressive    | Versions immuables, évaluations, activation humaine, aucune permission portée par une compétence, retour arrière                                                                              |
| Serveur MCP non fiable                   | Association explicite outil → permissions/risque par l'administrateur, risque `write` par défaut, résultats non fiables, transport stdio limité aux déploiements privés                       |

## 7. Secrets

- Secrets d'entreprise (jetons API, OAuth) chiffrés en **AES-256-GCM** avec une clé
  maître fournie par l'environnement (`MIMORYA_MASTER_KEY`, 32 octets base64) ou un
  KMS (phase 3) ; `key_version` permet la rotation.
- Jamais renvoyés par l'API, jamais injectés dans le contexte du modèle, jamais
  journalisés : les journaux de requêtes ne contiennent ni en-têtes ni corps (seuls
  méthode, chemin, statut, durée et identifiant de requête), et les structures qui
  portent une clé (`ProviderConfig`, options des fournisseurs) n'affichent jamais sa
  valeur en débogage. Le journal d'audit ne recopie jamais une entrée refusée.
- Clés LLM et clé maître : variables d'environnement / gestionnaire de secrets ; jamais
  committées (`.env` ignoré, `.env.example` sans valeur réelle).

## 8. Journal d'audit

- Table `audit_log` en **ajout seul** : le rôle applicatif n'a que `INSERT` et `SELECT`.
- Ni effacement ni réécriture indirecte : le rôle applicatif ne peut pas supprimer les
  entreprises, utilisateurs, conversations et runs que l'audit référence (les clés
  étrangères effaceraient ou videraient des entrées). Leur suppression relève du rôle
  propriétaire (voir [DATA_MODEL.md](DATA_MODEL.md) §4).
- Contenu : qui (utilisateur, agent pour le compte de…), quoi (action, ressource),
  résultat (succès, refus, échec), pourquoi (raison de la décision), contexte
  (requête, run, IP). Pas de contenu de message ni de secret.
- Phase 3 : chaînage cryptographique des entrées et export vers un SIEM.

## 9. Protection des données (RGPD)

- **Minimisation** : seules les données nécessaires sont envoyées au modèle ; la
  mémoire personnelle n'est constituée qu'avec l'accord de l'utilisateur.
- **Transparence** : chaque utilisateur consulte sa mémoire et l'historique des actions
  faites pour lui.
- **Droits** : export (JSON) et suppression des données d'un utilisateur ; suppression
  complète d'une entreprise ; journal conservant la trace sans le contenu.
- **Rétention** paramétrable (voir [DATA_MODEL.md](DATA_MODEL.md) §5).
- **Sous-traitants** : choix du fournisseur LLM et de sa région par entreprise ;
  possibilité de modèles locaux ; registre des sous-traitants et DPA.
- **Hébergement** : SaaS en UE ; installation privée possible.

## 10. Sécurité applicative

- Validation de toutes les entrées (serde + validator, messages en français) ; requêtes SQL
  paramétrées (sqlx) ; aucun `unsafe` (`unsafe_code = "forbid"` sur tout l'espace de travail).
- En-têtes de sécurité (CSP, HSTS, `X-Content-Type-Options`, `Referrer-Policy`).
- Limitation de débit globale et par route sensible (connexion, par adresse et par
  compte), voir §4 pour la détermination de l'adresse cliente.
- Dépendances verrouillées (`Cargo.lock`, `pnpm-lock.yaml`), audit des vulnérabilités en CI
  (phase 2), revue des licences (MIT/Apache/BSD privilégiées, AGPL exclue).
- Sauvegardes chiffrées, restauration testée périodiquement (phase 3).

## 11. Liste de contrôle pour chaque contribution

- [ ] Toute nouvelle table métier a `tenant_id`, la politique RLS et des tests.
- [ ] Toute nouvelle route vérifie l'authentification et les permissions côté serveur.
- [ ] Tout nouvel outil déclare permissions requises et niveau de risque, et filtre ses
      données avec `ctx.authz`.
- [ ] Aucun secret, jeton ou donnée personnelle superflue dans les logs.
- [ ] Les nouvelles permissions sensibles sont marquées non délégables si nécessaire.
- [ ] Les entrées externes (documents, résultats d'outils) sont traitées comme non fiables.
