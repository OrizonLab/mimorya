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
