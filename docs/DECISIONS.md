# Journal des décisions d'architecture (ADR)

Format court : contexte, décision, conséquences. Une décision n'est jamais modifiée :
elle est remplacée par une nouvelle entrée qui la référence (« remplace ADR-00X »).

---

## ADR-001 — Monolithe modulaire TypeScript

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : produit ambitieux mais équipe réduite ; domaine encore mouvant ;
  besoin de transactions traversant autorisation, audit et exécution.
- **Décision** : un dépôt pnpm, TypeScript strict de bout en bout ; paquets `core`,
  `db`, `agent`, `connectors` ; applications `api` (Fastify) et `web` (Next.js). Le
  futur worker réutilisera le code de l'API avec un autre point d'entrée. Python n'est
  introduit que pour un besoin spécifique (OCR, ML) sous forme de service annexe.
- **Conséquences** : types partagés, un seul outillage, déploiement simple pour une PME.
  Les frontières de paquets doivent être respectées pour pouvoir extraire des services.

## ADR-002 — Hermes Agent : reprendre les concepts et le format des compétences, pas le code

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : Hermes Agent (MIT) est un agent personnel Python d'environ 1 M de
  lignes, mono-utilisateur, à état sur disque, dont la sécurité repose sur l'isolation
  du système d'exploitation. Analyse détaillée : [ARCHITECTURE.md](ARCHITECTURE.md) §12.
- **Décision** : réimplémenter en TypeScript la boucle, la mémoire, la délégation, le
  cron et les passerelles en reprenant leurs principes ; adopter le format `SKILL.md`
  compatible agentskills.io (extensions sous `metadata.mimorya.*`) ; ne pas embarquer
  le code Hermes ni exécuter d'instance Hermes par entreprise.
- **Conséquences** : effort de développement plus élevé, mais isolation, permissions et
  audit natifs. Import possible des compétences MIT/Apache avec attribution. Le
  catalogue de plugins Hermes contient du GPL/AGPL : exclu.

## ADR-003 — PostgreSQL unique, schéma partagé, isolation par RLS

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : SaaS multi-entreprises pour TPE/PME, coût d'exploitation à maîtriser,
  exigence forte d'isolation.
- **Décision** : une base, un schéma, `tenant_id` partout, RLS sur toutes les tables
  métier, rôle applicatif sans `BYPASSRLS`, contexte posé par transaction. Fonctions
  `SECURITY DEFINER` limitées pour la connexion et la résolution de session.
- **Alternatives** : base par entreprise (isolation maximale, exploitation lourde) —
  conservée pour les déploiements privés et, plus tard, pour de grands comptes.
- **Conséquences** : chaque accès base passe par `withTenant()` ; tests d'isolation
  obligatoires.

## ADR-004 — Recherche hybride FTS + pgvector, vecteurs de dimension 1024

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : plein texte PostgreSQL (configuration `french`) toujours disponible ;
  embeddings optionnels en `vector(1024)` (index HNSW cosinus), fusion RRF. 1024 est
  supporté par de nombreux modèles multilingues (bge-m3, mistral-embed, voyage,
  text-embedding-3 avec `dimensions=1024`, e5-large).
- **Conséquences** : pas de base vectorielle séparée ; changer de modèle d'embedding
  impose une ré-indexation (colonne `embedding_model`).

## ADR-005 — Pas de Redis dans le MVP

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : files de travaux, verrous et diffusion d'événements reposent sur
  PostgreSQL (`SKIP LOCKED`, baux, puis `LISTEN/NOTIFY`). Redis sera introduit quand la
  charge le justifiera (cache, limitation de débit distribuée, diffusion multi-instances).
- **Conséquences** : un composant de moins à opérer pour les installations privées.

## ADR-006 — API Fastify séparée de Next.js

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : exécutions longues, SSE, webhooks entrants, workers, passerelles.
- **Décision** : Next.js sert l'interface ; Fastify porte l'API. Le navigateur appelle
  `/api/*` sur l'origine web, relayé vers l'API (même origine, pas de CORS).
- **Conséquences** : deux processus en développement ; le reverse proxy de production
  route `/api` directement vers l'API.

## ADR-007 — Couche LLM maison sur SDK officiels

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : interface `LlmProvider` avec adaptateurs Anthropic (`@anthropic-ai/sdk`)
  et OpenAI-compatible (`openai`, couvre OpenAI, OpenRouter, Gemini, Mistral, Ollama,
  vLLM), plus un fournisseur scénarisé pour les tests. Pas de framework d'agent tiers.
- **Conséquences** : maîtrise du format des outils, des coûts et des erreurs ;
  maintenance des adaptateurs à notre charge.

## ADR-008 — Drizzle ORM et migrations SQL versionnées

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : schéma déclaré en TypeScript, migrations générées par drizzle-kit puis
  relues ; migrations manuelles pour RLS, rôles, fonctions et index spécifiques.
- **Conséquences** : SQL lisible et revu ; exécution des migrations par le rôle
  propriétaire uniquement.

## ADR-009 — Sessions opaques en base, SSO OIDC en phase 2

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : jetons opaques aléatoires (empreinte stockée), cookie `HttpOnly`,
  révocables instantanément ; pas de JWT de session. Mots de passe Argon2id. OIDC
  ajouté en phase 2 sur la même table de sessions.

## ADR-010 — Moteur d'autorisation maison (RBAC à périmètre + ABAC JSON)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : besoin de périmètres hiérarchiques, de plafonds agent et d'un
  fonctionnement hors ligne en installation privée.
- **Alternatives** : OpenFGA / SpiceDB (ReBAC, service supplémentaire), Cerbos (service
  supplémentaire), Casbin (modèle générique peu adapté aux plafonds agent).
- **Décision** : moteur pur en TypeScript dans `core`, synchrone, testé ; conditions
  ABAC dans un langage JSON minimal et auditable.
- **Conséquences** : à réévaluer si les besoins relationnels se complexifient
  (partages fins document par document).

## ADR-011 — Un compte = une entreprise (MVP)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : l'e-mail est unique globalement et rattaché à une seule entreprise.
- **Conséquences** : simplifie connexion et isolation ; une table `identities` sera
  nécessaire pour les consultants multi-entreprises (phase 3).

## ADR-012 — Exécutions longues par baux PostgreSQL

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : un run en file est réservé par `FOR UPDATE SKIP LOCKED` avec un bail ;
  l'état est persisté à chaque étape ; un bail expiré permet la reprise par un autre
  worker. pg-boss sera évalué pour les tâches planifiées (phase 2).

## ADR-013 — Documentation en français, code en anglais

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : documentation produit et technique, libellés d'interface et messages
  utilisateur en français ; identifiants, commentaires de code et messages de commit en
  anglais. Internationalisation de l'interface en phase 3.

## ADR-014 — Agent : permissions non délégables et plafond de run

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : les permissions d'administration (comptes, rôles, politiques,
  connecteurs, compétences, paramètres, audit, validation) ne peuvent jamais être
  exercées par un agent, quels que soient les droits de l'utilisateur. Chaque run peut
  en outre être restreint à une liste de permissions ; un sous-agent hérite d'un
  sous-ensemble.
- **Conséquences** : l'administration reste une action humaine explicite ; aucune
  injection ni compétence ne peut élever des privilèges.

## ADR-015 — Exécution de l'API via `tsx` en MVP

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les paquets internes exposent leur source TypeScript ; un bundle
  esbuild complet de Fastify et des SDK est fragile, et une compilation par paquet
  ajoute de l'outillage avant que le besoin de performance au démarrage existe.
- **Décision** : l'API est lancée avec `tsx` en développement comme en production
  (MVP). La compilation (tsc par paquet ou bundle) sera introduite avec les images
  Docker de déploiement privé.
- **Conséquences** : démarrage un peu plus lent et transpilation à la volée ; aucun
  impact fonctionnel.

## ADR-016 — Exécution des runs dans le processus de l'API (MVP)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les tours de conversation sont courts (secondes à minutes) et l'état
  est persisté à chaque étape.
- **Décision** : l'API lance les runs en arrière-plan dans son processus et diffuse les
  événements en mémoire (SSE). Un balayage périodique marque en échec les runs sans
  progression depuis 15 minutes (`system_fail_stale_runs`). La reprise automatique
  après arrêt et le worker séparé arrivent en phase 2 (ADR-012).
- **Conséquences** : une seule instance d'API en MVP ; le passage à plusieurs instances
  impose la diffusion via PostgreSQL `LISTEN/NOTIFY` et le worker.

## ADR-017 — Repli serveur et cache de prompt chez Anthropic

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : l'adaptateur Anthropic active le cache de prompt sur le préfixe stable
  (outils + consignes) et, sur les modèles qui le permettent, le repli côté serveur
  (`fallbacks: "default"`) : une demande refusée pour raison de politique est rejouée
  sur le modèle de repli défini par Anthropic au lieu d'échouer. Les tours de
  l'assistant sont rejoués tels quels (blocs de raisonnement compris).
- **Conséquences** : le modèle ayant réellement servi la réponse est celui enregistré
  dans la consommation ; option désactivable (`serverSideFallback: false`).

## ADR-018 — Serveur en Rust, interface web en Next.js

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Remplace** : ADR-001 (langage du serveur), ADR-006 (Fastify), ADR-007 (SDK
  TypeScript), ADR-008 (Drizzle) et ADR-015 (`tsx`). ADR-010 reste valable : le moteur
  d'autorisation est désormais en Rust dans `crates/core`.
- **Contexte** : le porteur du produit a demandé un développement en Rust ; une première
  version complète existait en TypeScript (référence : commit `63defdf`). Choix retenu :
  « option A » — toute la logique serveur en Rust, l'interface reste en Next.js.
- **Décision** : le serveur est un espace de travail Cargo (`crates/core`, `db`,
  `agent`, `connectors`, `api`) ; l'interface `apps/web` ne communique qu'en HTTP via
  `/api/*`. Bibliothèques : axum + tokio (HTTP, SSE), sqlx (SQL paramétré, migrations
  SQL versionnées dans `crates/db/migrations`, appliquées par le rôle propriétaire),
  serde + schemars + validator (une structure donne le JSON Schema montré au modèle et
  la validation stricte), reqwest (adaptateurs Anthropic et OpenAI-compatible écrits
  sur l'API HTTP), rmcp (MCP), argon2 (mots de passe). Le contrat JSON de l'API est
  conservé à l'identique ; les migrations existantes sont reprises telles quelles.
- **Conséquences** :
  - les migrations SQL deviennent la source de vérité du schéma (plus de schéma
    TypeScript ni de contrôle de dérive) ; le test d'isolation vérifie que chaque table
    portant `tenant_id` a la RLS active et une politique ;
  - deux binaires compilés (`mimorya-api`, `mimorya-seed`) ; tests unitaires par
    `cargo test`, tests PostgreSQL par `cargo test-int` (marqués `#[ignore]`) ;
  - les bases de développement créées par drizzle-kit doivent être recréées (table de
    suivi des migrations différente : `_sqlx_migrations`) ;
  - les types de réponse de l'API sont maintenus à la main côté web
    (`apps/web/lib/api/types.ts`) ; leur génération depuis le serveur est à prévoir ;
  - compilation plus longue qu'en TypeScript, compensée par la vérification statique.

## ADR-019 — Administration : périmètre entreprise, pas d'auto-administration, mots de passe provisoires

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : l'administration passe en écriture (comptes, organisation,
  habilitations). Une permission d'administration détenue sur une seule unité
  donnait déjà accès à des écrans couvrant toute l'entreprise (audit, connecteurs) ;
  aucun service d'envoi d'e-mails n'existe encore.
- **Décision** :
  - les permissions d'administration (`TENANT_WIDE_PERMISSIONS`, `crates/core`) ne
    sont honorées qu'avec un périmètre entreprise, et un rôle qui en contient ne peut
    être attribué qu'à ce périmètre ;
  - un administrateur ne modifie ni son propre compte ni ses propres habilitations ;
  - un compte est créé (ou réinitialisé) avec un mot de passe provisoire aléatoire,
    affiché une fois à l'administrateur, que la personne remplace depuis « Mon compte »
    (ses autres sessions sont alors fermées) ;
  - une unité ne peut être supprimée que si plus rien n'en dépend (sous-unités,
    membres, habilitations, mémoire) ; les comptes ne sont jamais supprimés par
    l'application (désactivation).
- **Conséquences** : le dernier administrateur ne peut pas se retirer ses droits ;
  la transmission du mot de passe provisoire repose sur l'administrateur jusqu'à
  l'invitation par e-mail ; la séparation des pouvoirs complète (double validation
  des actions d'administration) reste à faire (phase 3).

## ADR-020 — Changements d'administration sérialisés par entreprise

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : chaque requête charge une vue des droits et de l'organigramme
  (organigramme mis en cache 30 s). Deux administrateurs agissant au même instant
  pouvaient valider leurs changements sur des vues périmées : deux déplacements croisés
  créaient un cycle (l'organigramme ne se chargeait plus et toute l'entreprise
  devenait inaccessible), deux retraits croisés laissaient l'entreprise sans
  administrateur, une habilitation créée pendant une suppression d'unité disparaissait
  par cascade.
- **Décision** :
  - tout changement d'administration (comptes, habilitations, organisation) s'exécute
    dans une transaction qui prend d'abord un verrou consultatif PostgreSQL propre à
    l'entreprise (`pg_advisory_xact_lock`, `TenantTx::lock_administration`) : les
    changements d'une même entreprise passent l'un après l'autre, sans nouvelle
    infrastructure ;
  - sous ce verrou, le changement revérifie sur les données en base que
    l'administrateur est toujours actif et détient la permission sur toute
    l'entreprise, et applique les règles de l'organigramme à l'arbre relu en base
    (pas au cache) ;
  - la suppression d'une unité verrouille sa ligne (`FOR UPDATE`) avant de compter ce
    qui en dépend : ce qui la référencerait entre-temps (mémoire écrite par un run,
    par exemple) attend puis échoue ;
  - le cache des autorisations ne réenregistre pas des données chargées pendant une
    invalidation (génération incrémentée à chaque invalidation) ;
  - le changement de son propre mot de passe, hors verrou d'administration, n'écrit
    que si l'empreinte lue n'a pas changé et que la session appelante est toujours
    ouverte : une réinitialisation simultanée par un administrateur l'emporte ;
  - une connexion relit la ligne du compte, verrouillée (`FOR NO KEY UPDATE`), avant
    d'ouvrir sa session : elle est refusée si le mot de passe vérifié a changé ou si
    le compte n'est plus actif, et une révocation en attente passe après elle ;
  - ordre des verrous commun à ces opérations : ligne du compte, puis sessions.
- **Conséquences** : les changements d'administration d'une entreprise ne sont pas
  parallèles (sans effet pratique à cette échelle) ; ces courses sont testées de façon
  déterministe : le test tient les lignes concernées et attend que les requêtes soient
  en file dans `pg_locks` (`crates/api/tests/support/locks.rs`, `admin_concurrency.rs`,
  `account_password.rs`, `login_races.rs`) ; comme personne ne retire ses
  propres droits et que chaque retrait revérifie les droits de son auteur, il reste
  toujours au moins un administrateur actif (hors échéance d'habilitation choisie).

## ADR-021 — Rôles personnalisés et mot de passe provisoire à remplacer

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : une entreprise doit pouvoir définir ses propres rôles métier, et un
  mot de passe provisoire transmis par un administrateur ne doit pas rester en usage.
- **Décision** :
  - un rôle personnalisé n'accorde que des permissions du catalogue (plateforme et
    connecteurs), écrites en toutes lettres, sans joker : il ne grandit pas avec le
    catalogue ; sa clé (`custom_…`) est dérivée de son nom à la création, avec un
    suffixe (`_2`, `_3`…) si elle est déjà prise (un rôle renommé garde sa clé) ;
  - les rôles de la plateforme ne se modifient ni ne se suppriment ; personne ne
    modifie ni ne supprime un rôle qu'il détient (même règle que pour ses propres
    habilitations) ; un rôle qui contient une permission d'administration reste
    attribué sur toute l'entreprise (une modification qui laisserait une habilitation
    limitée à une unité est refusée) ; un rôle encore attribué ne se supprime pas ;
  - ces changements passent sous le verrou d'administration (ADR-020) ; la
    revérification des droits de l'auteur relit les rôles et les politiques en base,
    et une attribution relit le rôle attribué ;
  - un compte créé ou réinitialisé porte `must_change_password` ; la résolution de
    session le renvoie, et l'extracteur d'identité `Auth` refuse alors toute route
    authentifiée ; seules `/auth/me` et `/account/password` acceptent cette identité
    (`AuthPendingPasswordChange`), la déconnexion reste possible et les routes sans
    identité (connexion, santé) ne sont pas concernées ; le changement de mot de
    passe lève l'obligation, la session courante continue.
- **Conséquences** : migration `0007` (colonne et fonction `auth_resolve_session`
  recréée) ; l'interface montre un écran dédié tant que le mot de passe est
  provisoire ; l'invitation par e-mail reste à faire ; sur une instance déjà en
  service, les comptes créés ou réinitialisés avant la migration ne sont pas
  concernés (à réinitialiser au besoin).

## ADR-022 — Politiques d'accès : l'administration n'écrit que des refus

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les politiques ABAC (`access_policies`) complètent les rôles. Une
  politique d'**autorisation** accorde l'accès à une ressource hors du périmètre des
  rôles ; écrite par un administrateur, elle peut viser ce même administrateur (une
  condition sur son identifiant, ou une condition large) et lui ouvrir les données
  métier, contournant la règle « pas d'administration de soi-même » (ADR-019) que
  seule une double validation (phase 3) peut garantir. Une politique illisible par le
  moteur empêcherait en outre de charger les autorisations de toute l'entreprise.
- **Décision** :
  - depuis l'administration (`policy:manage`, sur toute l'entreprise), on crée,
    modifie, active, désactive et supprime des politiques de **refus** uniquement ;
    les politiques d'autorisation existantes sont listées en lecture seule ;
  - une politique porte sur des permissions du catalogue ou sur `domaine:*`, jamais
    sur une permission d'administration (`TENANT_WIDE_PERMISSIONS`) ni sur celles que
    lisent ses écrans (`org:read`, `user:read`) : un refus ne peut pas bloquer les
    administrateurs ;
  - chaque permission déclare où la plateforme la vérifie (`Enforcement` : pas encore,
    sur la personne, ressource par ressource). Une politique ne vise que des
    permissions vérifiées (un refus sur une permission jamais vérifiée n'aurait aucun
    effet). Le moteur applique dans `data_scope` (donc routes, outils de l'agent,
    périmètres de liste, `/auth/me`) les refus dont la condition ne lit que le
    collaborateur et le contexte ; une condition sur la ressource ne s'évalue que
    là où chaque ressource est vérifiée (`check`) et n'est acceptée que pour ces
    permissions ;
  - la condition est désérialisée strictement puis vérifiée (`core::policy_rules`) :
    attributs sous `subject.`, `resource.` ou `context.`, au moins un attribut par
    comparaison, valeur requise sauf pour « est renseigné », listes pour « fait partie
    de », nombres pour les comparaisons d'ordre, pas de groupe vide, profondeur et
    longueur bornées, valeurs courtes (texte, nombre ou liste), sans caractère nul ;
    c'est la forme sérialisée de la condition typée qui est stockée. Une politique
    active, ou qu'on active, ou dont les permissions ou la condition changent, est
    revérifiée en entier avec ce qui est stocké (permissions et condition peuvent
    précéder une règle, ou avoir été écrites hors de l'API) ; une politique inactive
    se renomme ou reste désactivée quoi qu'elle contienne ;
  - les nombres se comparent par valeur (`2` et `2.0` sont égaux), comme l'éditeur
    web les affiche ;
  - là où la plateforme montre qu'une ressource existe sans la lire, un refus de
    lecture s'applique aussi : le créneau d'une intervention illisible n'apparaît que
    comme « occupé » (disponibilités, chevauchements d'une proposition) ; l'index des
    compétences du prompt suit `skill:read` de l'agent ; valider une action en
    attente reprend l'agent et exige donc `agent:use` ;
  - les changements passent sous le verrou d'administration (ADR-020), sont audités
    (avec l'état précédent) et s'appliquent aussitôt (cache invalidé).
- **Conséquences** : les entreprises ne peuvent pas encore élargir l'accès par
  politique (elles le font par rôles) ; un nouveau connecteur déclare l'`Enforcement`
  de chacune de ses permissions ; les attributs des collaborateurs utilisés par les
  conditions (`subject.attributes.*`) se modifient depuis l'administration
  (ADR-023).

## ADR-023 — Attributs des collaborateurs : posés par l'administration, typés et courts

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : les conditions des politiques lisent `subject.attributes.<nom>`
  (service, site, type de contrat…). Jusqu'ici, seul le seed posait ces attributs.
  Changer un attribut change les personnes que vise une politique, comme changer
  l'habilitation.
- **Décision** :
  - un administrateur des comptes (`user:manage`, sur toute l'entreprise) remplace
    l'ensemble des attributs d'un compte (`PATCH /admin/users/{id}`,
    `attributes`), jamais sur son propre compte : il pourrait lever un refus qui le
    vise ;
  - règles (`core::collaborator_attributes`) :
    - 20 attributs au plus ;
    - nom : une minuscule puis des lettres ou des chiffres, 40 caractères au plus,
      lisible dans un chemin à points ;
    - valeur : un texte d'une ligne visible (200 caractères au plus, sans caractère
      de contrôle, de mise en forme invisible comme l'espace sans largeur ou le
      forçage de sens, ni séparateur de ligne), un nombre (entier exact dans un
      navigateur, ±2^53−1), oui/non, ou une liste de 1 à 20 textes ;
  - le changement passe sous le verrou d'administration (ADR-020). L'audit nomme
    les attributs ajoutés, retirés ou modifiés (`attributesChanged`), **jamais leurs
    valeurs** : ce sont des données personnelles, et le journal, en ajout seul et
    conservé longtemps, garde la trace sans le contenu ;
  - le changement s'applique à la requête suivante : le sujet est relu à chaque
    requête. Un run déjà lancé garde le sujet calculé à son démarrage (SECURITY.md
    §5) ;
  - les valeurs ne sont montrées qu'aux administrateurs des comptes (`user:manage`
    sur toute l'entreprise). L'éditeur de politiques propose les noms en usage dans
    l'entreprise, sans les valeurs ;
  - l'interface type chaque valeur (texte, nombre, oui/non, liste, un élément par
    ligne). Des attributs écrits hors de l'interface sous une autre forme ne s'y
    modifient pas, pour ne pas les altérer.
- **Conséquences** : les attributs font partie du compte. Ils suivent sa rétention,
  son export et sa suppression. Le **nom** d'un attribut, lui, reste dans l'audit et
  dans l'éditeur de politiques : l'interface rappelle de ne placer de donnée sensible
  ni dans la valeur ni dans le nom (un nom comme « enArretMaladie » en est une). Les
  textes sont enregistrés en forme canonique (NFC) et sans caractère ignoré à
  l'affichage (`Default_Ignorable_Code_Point`, dont les jointeurs sans chasse). Les
  valeurs texte des politiques suivent la même règle (`core::plain_text`) : une valeur
  collée en forme décomposée correspond toujours à l'attribut.

## ADR-024 — Modèle par entreprise et magasin de secrets chiffrés

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : jusqu'ici, toutes les entreprises utilisaient le fournisseur de modèle
  du serveur (`LLM_PROVIDER`). Or une entreprise doit pouvoir choisir son fournisseur,
  avec sa propre clé : c'est elle qui décide où partent ses données (sous-traitant,
  région) et qui paie sa consommation. Une clé d'API ne peut pas être stockée en
  clair.
- **Décision** :
  - **magasin de secrets** (`crates/api/src/secrets`) :
    - chiffrement AES-256-GCM (`ring`) sous une clé maîtresse fournie par
      l'environnement (`MIMORYA_MASTER_KEY`, 32 octets en base64), avec un nonce
      aléatoire par écriture ;
    - l'entreprise et le nom du secret servent de données associées : une valeur
      copiée vers une autre entreprise ou un autre nom ne s'ouvre pas ;
    - la table `secrets` (déjà prévue) ne reçoit que le texte chiffré, le nonce, le
      tag et la version de clé ;
    - sans clé maîtresse, aucun secret ne peut être enregistré (503 explicite) ;
  - **choix du fournisseur** (`tenant_llm_settings`, migration 0008, sous RLS) :
    fournisseur, modèle et adresse. La clé est le secret `llm.api_key`. La
    modification exige `settings:manage` sur toute l'entreprise et passe sous le
    verrou d'administration ;
  - **adresses** :
    - Anthropic, OpenAI, OpenRouter et Gemini s'utilisent à leur adresse officielle,
      sans adresse à saisir ;
    - Ollama et « compatible OpenAI » ne s'utilisent qu'à une adresse listée par
      l'exploitant (`MIMORYA_TENANT_LLM_ENDPOINTS`). La liste est commune : chaque
      adresse est visible et utilisable par toutes les entreprises (une liste par
      entreprise viendra si un serveur doit être réservé à l'une d'elles) ;
    - une entreprise ne fait donc jamais appeler par le serveur une adresse de son
      choix (réseau interne de l'exploitant) ;
    - l'adresse officielle est écrite dans la configuration construite, et la clé
      est toujours celle de l'entreprise : rien ne vient de l'environnement du
      serveur (`ANTHROPIC_BASE_URL`, clé de la plateforme) ;
  - **clé** :
    - jamais renvoyée par l'API ni écrite dans l'audit ou les journaux ;
    - elle ne sert qu'au fournisseur et à l'adresse pour lesquels elle a été saisie :
      changer de fournisseur ou d'adresse exige de la saisir de nouveau, sinon
      l'ancienne est effacée ;
  - **résolution** (`LlmService`, mise en cache 60 s par entreprise et invalidée à
    chaque changement) :
    - sans configuration, les runs utilisent le fournisseur du serveur ;
    - avec une configuration, ils n'utilisent qu'elle. Elle est revérifiée à chaque
      chargement avec les règles du moment : une adresse que l'exploitant retire de
      la liste n'est plus jamais appelée ;
    - si elle devient inutilisable (adresse retirée, clé illisible après un
      changement de clé maîtresse, clé absente), les runs s'arrêtent : **pas de
      repli vers la plateforme**, qui enverrait les données ailleurs que là où
      l'entreprise l'a décidé. La raison est montrée aux administrateurs, et le refus
      est mis en cache comme le reste ; une panne passagère du serveur n'est pas
      présentée comme un refus ;
  - un **test de connexion** envoie une courte requête au fournisseur résolu ; sa
    consommation est enregistrée ;
  - **audit** : `llm.configured` (fournisseur, modèle, adresse, clé remplacée ou non,
    état précédent), `llm.reset` et `llm.tested`, sans clé.
- **Conséquences** :
  - perdre la clé maîtresse rend illisibles les clés enregistrées : les entreprises
    concernées doivent les saisir de nouveau ;
  - la rotation de la clé maîtresse (`key_version`) et un KMS restent à faire
    (phase 3) ;
  - avec plusieurs instances de l'API, un changement met jusqu'à 60 s à atteindre
    les autres.
