# Architecture technique

Statut : **référence vivante**. Toute évolution structurante est tracée dans
[DECISIONS.md](DECISIONS.md). Le statut d'implémentation de chaque brique est tenu dans
[ROADMAP.md](ROADMAP.md).

## 1. Vue d'ensemble

**Monolithe modulaire TypeScript** (ADR-001), découpé en paquets à frontières nettes,
déployé en trois processus issus du même dépôt :

```mermaid
flowchart LR
  subgraph Clients
    W[Navigateur<br/>Next.js]
    G[Passerelles<br/>Slack · Teams · e-mail<br/><i>phase 2</i>]
  end
  subgraph Plateforme
    WEB[web<br/>Next.js SSR]
    API[api<br/>Fastify · REST + SSE]
    WRK[worker<br/>exécutions longues,<br/>tâches planifiées]
  end
  subgraph Données
    PG[(PostgreSQL 16<br/>RLS · pgvector · FTS)]
    OBJ[(Stockage objets<br/>documents<br/><i>phase 2</i>)]
  end
  subgraph Externes
    LLM[Fournisseurs LLM<br/>Anthropic · OpenAI-compatibles<br/>OpenRouter · local]
    MCP[Serveurs MCP]
    APPS[Applications métier<br/>ERP · CRM · agenda…]
  end
  W -->|/api/* même origine| WEB --> API
  G --> API
  API --> PG
  WRK --> PG
  API --> LLM
  WRK --> LLM
  API --> MCP
  WRK --> MCP
  WRK --> APPS
  API --> APPS
  API -.-> OBJ
```

- **web** : interface Next.js (App Router). Le navigateur ne parle qu'à l'origine web ;
  `/api/*` est relayé vers l'API (cookies `SameSite=Lax`, pas de CORS en production).
- **api** : authentification, API REST, flux temps réel SSE, exécution des tours de
  conversation, webhooks entrants.
- **worker** : exécutions longues (missions), reprises après interruption, tâches
  planifiées, ingestion de documents. En MVP, l'API peut embarquer le worker
  (`MIMORYA_PROCESS_ROLE=all`) pour simplifier le déploiement d'une PME.

Pourquoi pas des microservices : une équipe réduite, un domaine encore mouvant et des
transactions qui traversent plusieurs modules (autorisation + audit + exécution). Les
frontières de paquets permettent d'extraire un service plus tard si la charge l'exige.

## 2. Paquets et dépendances

```
apps/
  web/          Interface (Next.js, React, Tailwind)
  api/          Serveur HTTP (Fastify), composition des modules, SSE, worker embarqué
packages/
  core/         Domaine pur : autorisation RBAC/ABAC, organigramme, autonomie,
                contrat d'outil, règles de mémoire, données de démo. Aucune E/S.
  db/           Schéma Drizzle, migrations SQL, RLS, dépôts (repositories), seed
  agent/        Moteur agentique : fournisseurs LLM, boucle, PEP outils, budgets,
                vérification, mémoire, compétences, sous-agents
  connectors/   Cadre de connecteurs, connecteur de démonstration, pont MCP
```

Règles de dépendance (vérifiées en revue, à outiller plus tard) :

```mermaid
flowchart BT
  core
  db --> core
  agent --> core
  connectors --> core
  api --> db & agent & connectors & core
  web -. types uniquement .-> core
```

- `core` ne dépend de rien d'autre que `zod`.
- `agent` ne connaît pas la base : il dépend d'**interfaces** (`RunStore`,
  `MemoryStore`, `AuditSink`, `ApprovalStore`…) implémentées par `db` et injectées par
  `api`. Le moteur est ainsi testable en mémoire, sans base ni réseau.
- Les paquets internes exposent leur source TypeScript (`exports: ./src/index.ts`),
  consommée par `tsx`, Vitest et Next.js (`transpilePackages`). L'API est empaquetée
  avec esbuild pour la production.

## 3. Pile technique retenue

| Domaine         | Choix                                                                                           | Justification                                                                                                                                                                               |
| --------------- | ----------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Langage         | TypeScript (strict) partout                                                                     | Un seul langage front/back, types partagés, SDK officiels Anthropic/OpenAI/MCP de premier rang. Python réservé à d'éventuels traitements spécialisés (OCR, ML) en service annexe (ADR-001). |
| Frontend        | Next.js 16, React 19, Tailwind CSS 4                                                            | Standard du marché, SSR, responsive, écosystème de composants accessible.                                                                                                                   |
| API             | Fastify 5                                                                                       | Performant, validation par schéma, écosystème de plugins (cookies, rate-limit, helmet), streaming natif pour SSE (ADR-006).                                                                 |
| Base            | PostgreSQL 16                                                                                   | Transactions, JSONB, RLS pour l'isolation, FTS, extensions ; une seule base à opérer pour une PME (ADR-003).                                                                                |
| Vectoriel       | pgvector (HNSW, cosinus, 1024 dims) + FTS PostgreSQL                                            | Recherche hybride sans infrastructure supplémentaire (ADR-004).                                                                                                                             |
| ORM             | Drizzle ORM + migrations SQL versionnées                                                        | Proche du SQL, typé, compatible RLS (`SET LOCAL`), migrations lisibles (ADR-008).                                                                                                           |
| Validation      | Zod 4                                                                                           | Schémas d'entrée des outils → JSON Schema pour les LLM, validation API.                                                                                                                     |
| Files d'attente | PostgreSQL (`FOR UPDATE SKIP LOCKED`, baux)                                                     | Pas de composant supplémentaire en MVP ; Redis/pg-boss évalués à la montée en charge (ADR-005, ADR-012).                                                                                    |
| LLM             | Couche d'abstraction maison sur SDK officiels (`@anthropic-ai/sdk`, `openai`)                   | Contrôle fin des appels d'outils, du coût et du routage ; OpenAI-compatible couvre OpenRouter, Gemini, Mistral, Ollama, vLLM (ADR-007).                                                     |
| MCP             | `@modelcontextprotocol/sdk` (client)                                                            | SDK officiel ; transports stdio (privé) et Streamable HTTP (SaaS).                                                                                                                          |
| Mots de passe   | Argon2id (`@node-rs/argon2`)                                                                    | Recommandation OWASP ; SSO/OIDC en phase 2 (ADR-009).                                                                                                                                       |
| Tests           | Vitest (unitaires + intégration sur PostgreSQL réel)                                            | Rapide, ESM natif, un seul outil.                                                                                                                                                           |
| Observabilité   | Logs JSON structurés (pino via Fastify), identifiants de corrélation ; OpenTelemetry en phase 3 | Exploitable tout de suite, standard ouvert ensuite.                                                                                                                                         |
| Conteneurs      | Docker + docker-compose (dev, déploiement privé simple)                                         | Reproductible ; Kubernetes seulement si nécessaire.                                                                                                                                         |

## 4. Flux principaux

### 4.1 Tour de conversation

```mermaid
sequenceDiagram
  participant U as Utilisateur
  participant A as API
  participant Z as Autorisation (core)
  participant E as Moteur agent
  participant L as LLM
  participant T as Outil / connecteur
  participant D as PostgreSQL
  U->>A: POST /conversations/:id/messages (cookie de session)
  A->>D: session → utilisateur, entreprise (SET LOCAL app.tenant_id)
  A->>Z: sujet (rôles, périmètres, attributs)
  A->>D: crée run (status=running) + message utilisateur
  A-->>U: 202 + run_id ; U s'abonne au flux SSE du run
  loop jusqu'à réponse finale ou budget épuisé
    E->>L: contexte (consignes fixes, contexte orga, mémoires autorisées, outils visibles)
    L-->>E: texte et/ou appels d'outils
    E->>Z: PEP : permissions requises + autonomie
    alt autorisé
      E->>T: exécute (périmètre de données lié au sujet)
      T-->>E: résultat (balisé « donnée non fiable »)
    else validation requise
      E->>D: crée approval, run → awaiting_approval
    else refusé
      E-->>L: refus motivé (le modèle s'adapte)
    end
    E->>D: événements, audit, usage
    E-->>U: événements SSE (étapes, outils, texte)
  end
  E->>E: vérification (ancrage des sources + critères), correction bornée
  E->>D: message final, mémoire proposée, statut
```

### 4.2 Mission longue et reprise

1. La mission crée un `agent_run` en statut `queued`.
2. Un worker le réserve (`FOR UPDATE SKIP LOCKED`, bail `lease_expires_at`).
3. Chaque étape est **persistée** (messages, événements, état) : en cas d'arrêt, un
   autre worker reprend au dernier point de contrôle une fois le bail expiré.
4. Une validation humaine met le run en `awaiting_approval` ; la décision le remet en
   file.
5. Les tâches récurrentes (phase 2) créent des runs selon une expression cron.

## 5. Moteur agentique et mémoire

Détaillés dans [AGENT_ENGINE.md](AGENT_ENGINE.md). En résumé :

- boucle **planifier → agir → observer → vérifier → corriger** bornée par des budgets
  (étapes, tokens, coût, durée) ;
- **PEP** (point d'application des politiques) sur chaque appel d'outil ;
- **sous-agents** avec plafond de permissions strictement inclus dans celui du parent ;
- mémoire à **quatre espaces** (personnel, unité, entreprise, expérience), recherche
  hybride FTS + vecteurs **pré-filtrée par les autorisations** ;
- **compétences** au format `SKILL.md` (compatible agentskills.io), versionnées,
  évaluées, activées après validation humaine, réversibles.

## 6. Connecteurs et outils

- Un **connecteur** déclare : ses permissions métier (ex. `intervention:read`), ses
  outils (schéma d'entrée Zod, niveau de risque, permissions requises), sa
  configuration et ses secrets.
- Une **instance de connecteur** est activée par entreprise, avec sa configuration, ses
  secrets chiffrés, sa table de correspondance permissions/risques et son journal.
- **MCP** : un pont transforme chaque outil d'un serveur MCP en outil de la plateforme.
  Les serveurs MCP ignorent tout de nos utilisateurs : l'administrateur **associe**
  chaque outil MCP à des permissions et à un niveau de risque (par défaut `write`,
  donc soumis à validation) ; les résultats sont traités comme non fiables.
- Les secrets ne transitent jamais par le contexte du modèle ni par les journaux.

Connecteurs prévus : démonstration (MVP), MCP générique (MVP), webhooks/REST générique,
e-mail (IMAP/SMTP, Microsoft Graph, Gmail), agendas, Slack/Teams/Telegram/Discord
(passerelles conversationnelles), Google Workspace / Microsoft 365, bases SQL en lecture,
GED, navigateur automatisé en bac à sable (phase 2+).

## 7. Isolation multi-entreprises

Défense en profondeur (détails : [SECURITY.md](SECURITY.md)) :

1. **Applicative** : chaque requête est exécutée dans une transaction qui positionne
   `app.tenant_id` ; les dépôts filtrent systématiquement par entreprise.
2. **Base** : **Row-Level Security** sur toutes les tables métier ; l'application se
   connecte avec un rôle **sans** `BYPASSRLS` et non propriétaire des tables. Sans
   `app.tenant_id`, aucune ligne n'est visible.
3. **Moteur** : `AuthorizationEngine.check` refuse toute ressource d'une autre
   entreprise avant toute autre règle.
4. **Tests** : suites d'intégration qui tentent des lectures croisées.

## 8. Temps réel

SSE (`text/event-stream`) pour suivre un run : simple, compatible proxys HTTP,
reprise via `Last-Event-ID` (événements numérotés en base). En MVP la diffusion est
en mémoire du processus ; avec plusieurs instances, elle passera par
`LISTEN/NOTIFY` PostgreSQL puis Redis si nécessaire.

## 9. Couche LLM

- Interface unique `LlmProvider.complete({system, messages, tools, …})` → texte, appels
  d'outils, usage (tokens, coût estimé).
- Adaptateurs : **Anthropic** (natif), **OpenAI-compatible** (OpenAI, OpenRouter,
  Gemini via point d'accès compatible, Mistral, Ollama, vLLM), **script** (tests
  déterministes).
- **Routage** par classe de tâche (`chat`, `planning`, `verification`, `summarize`,
  `embedding`) configurable par entreprise, avec contraintes de confidentialité
  (ex. « données confidentielles → modèle local uniquement », phase 2).
- Suivi des coûts : chaque appel enregistre fournisseur, modèle, tokens et coût estimé
  dans `usage_records`.

## 10. Observabilité et exploitation

- Logs JSON structurés avec `request_id`, `tenant_id`, `run_id` ; **jamais** de
  secrets, de mots de passe ni de jetons (liste de rédaction centralisée).
- Journal d'audit métier séparé (table `audit_log`, en ajout seul).
- Événements de run persistés : rejouables pour le support et l'analyse.
- Phase 3 : OpenTelemetry (traces), métriques Prometheus, tableaux de bord
  d'exploitation, alertes de coût.

## 11. Déploiement

| Mode          | Description                                                                                             |
| ------------- | ------------------------------------------------------------------------------------------------------- |
| Développement | `docker compose up db` + `pnpm dev:api` / `pnpm dev:web`                                                |
| Privé (PME)   | `docker compose` : web, api (worker embarqué), PostgreSQL ; sauvegardes `pg_dump`/WAL                   |
| SaaS          | Images web/api/worker séparées, PostgreSQL managé, montée en charge horizontale de l'API et des workers |

Configuration exclusivement par variables d'environnement (voir `.env.example`).
Les secrets d'infrastructure (clé maître de chiffrement, clés LLM) viennent de
l'environnement ou d'un gestionnaire de secrets ; ceux des entreprises sont stockés
chiffrés en base.

## 12. Analyse de Hermes Agent (Nous Research)

Analyse réalisée sur le dépôt `NousResearch/hermes-agent` (commit `bb834a2`, v0.22.x).

**Constat** : licence **MIT** (Copyright 2025 Nous Research) — réutilisation commerciale
permise avec mention. Projet Python d'environ 1 M de lignes hors tests, état sur disque
(fichiers + SQLite), conçu explicitement comme **agent personnel mono-utilisateur**
(« Hermes Agent is a single-tenant personal agent » — SECURITY.md du projet) : ni
entreprise, ni utilisateurs, ni RBAC ; la frontière de sécurité revendiquée est le
système d'exploitation. Certains éléments du catalogue de plugins sont sous GPL/AGPL
(à exclure).

| Mécanisme Hermes                                                                                            | Décision                     | Adaptation                                                                                                              |
| ----------------------------------------------------------------------------------------------------------- | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Boucle agentique (`run_conversation`, budgets d'itérations, classification d'erreurs, repli de fournisseur) | **Concept repris**           | Réimplémentée en TS, avec PEP, budgets de coût, persistance et reprise.                                                 |
| Compression de contexte (élagage des sorties d'outils, protection tête/queue, résumé par modèle auxiliaire) | **Concept repris**           | Phase 2 ; prompt système stable pour le cache fournisseur.                                                              |
| Mémoire `MEMORY.md` / `USER.md` bornée, chargée en début de session                                         | **Concept repris**           | Devient la mémoire personnelle bornée ; plus espaces d'équipe/entreprise/expérience, en base, filtrés par autorisation. |
| Recherche dans les sessions passées (SQLite FTS5)                                                           | **Concept repris**           | FTS PostgreSQL + pgvector, filtrée par utilisateur.                                                                     |
| Skills `SKILL.md` + chargement progressif (liste → contenu → références)                                    | **Format adopté**            | Compatibilité agentskills.io, extensions sous `metadata.mimorya.*`, versions en base, évaluations, validation humaine.  |
| Création/patch autonome de skills, revue post-tour, « curator » qui archive                                 | **Concept repris, encadré**  | Propositions en statut `draft`/`candidate`, jamais actives sans validation ; aucune skill ne porte de permission.       |
| Délégation (`delegate_task`, outils interdits aux enfants, profondeur 1, parallélisme borné)                | **Concept repris**           | Sous-agents en runs enfants persistés, plafond de permissions ⊆ parent.                                                 |
| Registre d'outils, toolsets, approbation des commandes dangereuses                                          | **Concept repris**           | Contrat d'outil typé avec niveau de risque ; approbations liées aux niveaux d'autonomie ; pas de shell libre.           |
| Client MCP (stdio, HTTP, OAuth)                                                                             | **Concept repris**           | SDK MCP TypeScript officiel ; association permissions/risques par l'administrateur.                                     |
| Passerelles (Telegram, Discord, Slack, e-mail…) et clé de session par plateforme                            | **Concept repris** (phase 2) | Identité de canal **liée à un compte utilisateur** ; permissions identiques à celles du web.                            |
| Cron (`jobs.json`, tick 60 s)                                                                               | **Concept repris** (phase 2) | Tâches en base, exécutées par les workers, appartenant à un utilisateur.                                                |
| Abstraction fournisseurs (format OpenAI interne, Anthropic natif, OpenRouter)                               | **Concept repris**           | Même approche : adaptateur natif Anthropic + OpenAI-compatible.                                                         |
| Code Python, stockage fichiers/SQLite, état global de processus                                             | **Non réutilisé**            | Incompatible avec le multi-entreprises, l'isolation et le passage à l'échelle.                                          |

**Option écartée** : exécuter une instance Hermes par entreprise derrière son API
compatible OpenAI. Cela réutiliserait le code mais au prix d'une isolation par
conteneur coûteuse, d'une mémoire partagée entre utilisateurs d'une même instance et
d'aucun contrôle fin des permissions — incompatible avec les exigences de ce produit.
Le contenu des skills MIT/Apache d'Hermes pourra être importé (avec attribution) une
fois le format compatible en place.

## 13. Contraintes et risques majeurs

| Risque                                                                         | Impact   | Mesures                                                                                           |
| ------------------------------------------------------------------------------ | -------- | ------------------------------------------------------------------------------------------------- |
| Fuite de données entre entreprises                                             | Critique | RLS + filtrage applicatif + tests d'isolation systématiques ; rôle base sans `BYPASSRLS`.         |
| Fuite entre utilisateurs d'une même entreprise (via mémoire, recherche, agent) | Critique | Pré-filtrage des recherches par périmètre, mémoire personnelle non partagée, plafond agent.       |
| Injection de prompt (documents, e-mails, résultats d'outils)                   | Élevé    | Autorisations hors modèle, données balisées non fiables, validations, permissions non délégables. |
| Actions erronées à impact externe                                              | Élevé    | Niveaux d'autonomie, validations, risques déclarés par outil, journal d'audit.                    |
| Hallucinations dans les synthèses                                              | Élevé    | Vérification d'ancrage (identifiants cités ∈ sources), critères de réussite, sources affichées.   |
| Dérive des coûts LLM                                                           | Moyen    | Budgets par run, quotas par entreprise, routage vers modèles économiques, suivi d'usage.          |
| Dépendance à un fournisseur LLM                                                | Moyen    | Abstraction fournisseurs, routage configurable, support des modèles locaux.                       |
| Régression d'une compétence auto-améliorée                                     | Moyen    | Versions, évaluations, non-régression, retour arrière, validation humaine.                        |
| Conformité RGPD (sous-traitants LLM hors UE, conservation)                     | Élevé    | Choix de fournisseurs/régions par entreprise, minimisation, rétention, export/suppression, DPA.   |
| Complexité excessive trop tôt                                                  | Moyen    | Monolithe modulaire, PostgreSQL seul en MVP, phases strictes.                                     |
| Qualité variable des modèles pour l'appel d'outils                             | Moyen    | Tests de scénario par modèle, sélection de modèles validés, schémas d'outils simples.             |
