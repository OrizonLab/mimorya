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
