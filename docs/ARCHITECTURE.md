# Architecture technique

Statut : **référence vivante**. Toute évolution structurante est tracée dans
[DECISIONS.md](DECISIONS.md). Le statut d'implémentation de chaque brique est tenu dans
[ROADMAP.md](ROADMAP.md).

## 1. Vue d'ensemble

**Monolithe modulaire en Rust** (ADR-001, ADR-018), découpé en crates à frontières
nettes, avec une interface web Next.js ; déployé en trois processus issus du même dépôt :

```mermaid
flowchart LR
  subgraph Clients
    W[Navigateur<br/>Next.js]
    G[Passerelles<br/>Slack · Teams · e-mail<br/><i>phase 2</i>]
  end
  subgraph Plateforme
    WEB[web<br/>Next.js SSR]
    API[api<br/>Rust · axum · REST + SSE]
    WRK[worker<br/>exécutions longues,<br/>tâches planifiées]
  end
  subgraph Données
    PG[(PostgreSQL 16<br/>RLS · pgvector · FTS)]
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
  API -->|un fichier déposé| EXT[mimorya-extract<br/>processus borné]
```

- **web** : interface Next.js (App Router). Le navigateur ne parle qu'à l'origine web ;
  `/api/*` est relayé vers l'API (cookies `SameSite=Lax`, pas de CORS en production).
- **api** : authentification, API REST, flux temps réel SSE, exécution des tours de
  conversation, webhooks entrants.
- **worker** : exécution des runs, reprises après interruption, tâches planifiées,
  étapes des missions longues (ADR-089). Dans
  les piles Docker,
  c'est un service distinct de l'API (ADR-087) ; en rôle `all`, par défaut et en
  développement, l'API exécute elle-même les runs en arrière-plan. Le worker reprend les
  runs qu'un arrêt a interrompus quand c'est sûr
  (ADR-053), prévient les responsables des demandes dont l'arrêt a perdu l'avis (ADR-070),
  et un balayage périodique marque en échec les runs restés sans
  progression et fait expirer les demandes de validation échues, leurs runs poursuivant
  sans l'action (ADR-061), leurs responsables relancés la veille (ADR-062). Chaque run
  s'exécute sous le bail de son exécuteur, renouvelé tant qu'il s'exécute ; celui dont le
  bail a expiré est repris sans attendre un redémarrage (ADR-083, premier lot du worker).
  Le worker est le même programme lancé avec un autre rôle (`MIMORYA_ROLE`, ADR-085) :
  `all` (par défaut : sert le HTTP et exécute), `api` (sert le HTTP ; les runs qu'il
  lance attendent en file) ou `worker` (exécute les runs en file et les tâches de fond).
  Plusieurs `api` et plusieurs `worker` peuvent tourner ensemble : chaque balayage qui ne
  doit tourner qu'une fois (demandes échues, relances, avis perdus, indexation) prend un
  verrou consultatif PostgreSQL à son nom (ADR-086). Les
  caches que chaque processus garde (accès, modèle, modèle d'embeddings, connecteurs) se
  vident ensemble : une modification faite dans l'un est annoncée aux autres sur le canal
  PostgreSQL `mimorya_invalidations`.

Pourquoi pas des microservices : une équipe réduite, un domaine encore mouvant et des
transactions qui traversent plusieurs modules (autorisation + audit + exécution). Les
frontières de paquets permettent d'extraire un service plus tard si la charge l'exige.

## 2. Crates et dépendances

```
crates/                 Espace de travail Cargo (serveur)
  core/         Domaine pur : autorisation RBAC/ABAC, organigramme, autonomie,
                contrat d'outil, règles de mémoire, données de démo. Aucune E/S.
  db/           Migrations SQL (sqlx), RLS, transactions par entreprise, dépôts, seed
  agent/        Moteur agentique : fournisseurs LLM, boucle, PEP outils, budgets,
                vérification, outils mémoire et compétences
  connectors/   Contrat de connecteur, connecteur de démonstration, pont MCP (rmcp),
                API REST décrites par l'entreprise (ADR-112)
  extract/      Lecture des documents déposés (PDF, Word, texte), dans le processus
                `mimorya-extract`, jamais dans celui de l'API (ADR-095)
  api/          Serveur HTTP (axum) : sessions, routes, SSE, exécution des runs,
                binaires `mimorya-api`, `mimorya-admin` (exploitation, ADR-033),
                `mimorya-seed` (démonstration) et `mimorya-extract` (ADR-095)
apps/
  web/          Interface (Next.js, React, Tailwind) — seul paquet Node du dépôt
```

Règles de dépendance (imposées par les `Cargo.toml` : une dépendance non déclarée ne
compile pas) :

```mermaid
flowchart BT
  core
  db --> core
  agent --> core
  connectors --> core
  extract --> core
  api --> db & agent & connectors & extract & core
  web -. HTTP /api/* uniquement .-> api
```

- `core` n'effectue aucune E/S (dépendances : `serde`, `schemars`, `validator`,
  `regex`, `chrono`, `uuid`).
- `extract` ne lit que son entrée standard et n'écrit que sa sortie standard ; l'API ne
  l'appelle pas, elle lance le processus `mimorya-extract` qui le contient
  (`services/documents/extractor.rs`, ADR-095).
- `agent` ne connaît pas la base : il dépend de **ports** (`RunStore`, `MemoryStore`,
  `AuditSink`, `ApprovalStore`… dans `runtime/ports.rs`) implémentés par `api` sur les
  dépôts de `db`. Le moteur est ainsi testable en mémoire, sans base ni réseau
  (`ScriptedProvider`, `InMemoryPorts`, fonctionnalité `testing`).
- `web` ne partage aucun code avec le serveur : il consomme le contrat JSON de l'API.
  Ses types sont générés depuis les réponses et les corps de requêtes typés du serveur
  (`crates/api/src/contract` → `apps/web/lib/api/contract.ts`, ADR-036, ADR-039,
  ADR-040) : réponses, erreurs, événements du flux et corps de toutes les requêtes.

## 3. Pile technique retenue

| Domaine         | Choix                                                                                        | Justification                                                                                                                                                                       |
| --------------- | -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Langage         | Rust (édition 2024) pour le serveur ; TypeScript (strict) pour l'interface                   | Sûreté mémoire, typage fort de bout en bout, performances et binaire unique à déployer (ADR-018). Python réservé à d'éventuels traitements spécialisés (OCR, ML) en service annexe. |
| Frontend        | Next.js 16, React 19, Tailwind CSS 4                                                         | Standard du marché, SSR, responsive, écosystème de composants accessible.                                                                                                           |
| API             | axum 0.8 + tokio                                                                             | Standard de l'écosystème Rust, middlewares composables, SSE natif (ADR-006, ADR-018).                                                                                               |
| Base            | PostgreSQL 16                                                                                | Transactions, JSONB, RLS pour l'isolation, FTS, extensions ; une seule base à opérer pour une PME (ADR-003).                                                                        |
| Vectoriel       | pgvector (HNSW, cosinus, 1024 dims) + FTS PostgreSQL                                         | Recherche hybride sans infrastructure supplémentaire (ADR-004).                                                                                                                     |
| Accès base      | sqlx 0.8 (requêtes SQL paramétrées) + migrations SQL versionnées                             | SQL explicite, compatible RLS (`set_config` transactionnel), migrations lisibles (ADR-008 remplacé par ADR-018).                                                                    |
| Validation      | serde + schemars + validator                                                                 | Une seule structure Rust donne le JSON Schema montré au modèle et la validation stricte des entrées (outils, API).                                                                  |
| Files d'attente | PostgreSQL (`FOR UPDATE SKIP LOCKED`, baux)                                                  | Pas de composant supplémentaire en MVP ; Redis/pg-boss évalués à la montée en charge (ADR-005, ADR-012).                                                                            |
| LLM             | Adaptateurs maison sur HTTP (`reqwest`) : Anthropic Messages (flux SSE) et OpenAI-compatible | Contrôle fin des appels d'outils, du cache, du coût et du routage ; OpenAI-compatible couvre OpenRouter, Gemini, Mistral, Ollama, vLLM (ADR-007).                                   |
| MCP             | `rmcp` (SDK Rust officiel, client)                                                           | SDK officiel ; transports stdio (privé) et Streamable HTTP (SaaS).                                                                                                                  |
| Mots de passe   | Argon2id (crate `argon2`, paramètres OWASP)                                                  | Recommandation OWASP (ADR-009).                                                                                                                                                     |
| SSO             | `openidconnect` (OpenID Connect : découverte, PKCE, vérification du jeton d'identité)        | Bibliothèque éprouvée plutôt qu'une vérification de jetons faite main ; branchée sur le client HTTP durci des connecteurs (ADR-063).                                                |
| Tests           | `cargo test` (unitaires) + tests d'intégration sur PostgreSQL réel (`cargo test-int`)        | Un seul outil, exécution en série des suites qui partagent la base de test.                                                                                                         |
| Observabilité   | Logs structurés (`tracing`), identifiant de requête ; OpenTelemetry en phase 3               | Exploitable tout de suite, standard ouvert ensuite.                                                                                                                                 |
| Conteneurs      | Docker + docker-compose (dev, déploiement privé simple)                                      | Reproductible ; Kubernetes seulement si nécessaire.                                                                                                                                 |

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
      T-->>E: résultat (balisé « donnée non fiable » ; compétence chargée : « procédure de l'entreprise », ADR-052)
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

1. Un processus `api` crée le run (ou le relance après une décision) en statut `queued`,
   l'inscrit dans son journal (`run_queued`, affiché « En file ») et réveille le worker
   (canal `mimorya_run_queue`) ; en rôle `all`, il l'exécute lui-même (ADR-085).
2. Un worker le réserve (`FOR UPDATE SKIP LOCKED`, bail `lease_expires_at`).
3. Chaque étape est **persistée** (messages, événements, état) : en cas d'arrêt, un
   autre worker reprend au dernier point de contrôle une fois le bail expiré, quand rien
   d'autre qu'une lecture n'était en cours (ADR-053). Le bail est pris dans la
   transaction qui fait passer le run à `running` et renouvelé toutes les 20 s ; un run
   dont le bail a expiré, ou qui n'en a jamais eu, est repris dans la minute, au
   démarrage d'un worker comme ensuite, par celui qui le verrouille le premier (ADR-083,
   ADR-086). Chaque écriture de l'exécuteur sur le run (journal, état, demandes, issue)
   verrouille le run tant qu'il porte son bail : un exécuteur qui l'a perdu s'arrête sans
   rien écrire ni appeler de plus ; seuls l'audit, la consommation et l'appel d'outil
   terminé, qui disent ce qui s'est passé, s'enregistrent sans condition (ADR-086).
4. Une validation humaine met le run en `awaiting_approval` ; la décision le remet en
   file.
5. Les tâches planifiées créent des runs à heure fixe (ADR-080).
6. Une **mission** (ADR-089) avance par étapes, chacune un run de sa conversation, en
   file : son budget d'étape atteint, l'étape se termine sur un point d'étape et, dans
   la même transaction, met la suivante en file tant que les limites de la mission le
   permettent ; la réponse finale, revue au regard de ses critères, la clôt. Son auteur
   peut l'arrêter : l'étape en cours perd son statut et son bail, et son exécuteur
   s'arrête sans rien écrire de plus. Il est prévenu par e-mail d'une étape qui attend
   sa validation et, par la passe de chaque minute, de sa fin, sauf s'il l'a arrêtée
   (ADR-091) ou a choisi de ne pas recevoir ces e-mails (ADR-094).

### 4.3 Connexion par le fournisseur d'identité de l'entreprise (ADR-063)

```mermaid
sequenceDiagram
  participant N as Navigateur
  participant A as API
  participant F as Fournisseur (Microsoft 365…)
  N->>A: POST /api/auth/sso/discover (e-mail)
  A-->>N: fournisseur de l'entreprise du compte, ou aucun
  N->>A: POST /api/auth/sso/start
  A->>F: découverte (émetteur, clés), gardée une heure (ADR-065)
  A-->>N: adresse d'autorisation + cookie de tentative scellé
  N->>F: connexion de la personne
  F-->>N: redirection vers /api/auth/sso/callback (code, state)
  N->>A: retour + cookie de tentative
  A->>F: échange du code (PKCE, secret client)
  A->>A: jeton vérifié, compte relié ou retrouvé, session ouverte
  A-->>N: 303 vers /chat (ou /login?sso=echec)
```

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
  outils (structure d'entrée validée, niveau de risque, permissions requises), sa
  configuration et ses secrets.
- Une **instance de connecteur** est activée par entreprise, avec sa configuration, ses
  secrets chiffrés, sa table de correspondance permissions/risques et son journal.
- **MCP** : un pont transforme chaque outil d'un serveur MCP en outil de la plateforme.
  Les serveurs MCP ignorent tout de nos utilisateurs : l'administrateur **associe**
  chaque outil MCP à des permissions et à un niveau de risque (par défaut `write`,
  donc soumis à validation) ; les résultats sont traités comme non fiables. Une
  entreprise ajoute ses serveurs en HTTPS depuis l'administration, après les avoir
  essayés si elle le souhaite (ADR-050) ; le serveur ne les joint qu'à des adresses
  publiques (ADR-025). Les politiques d'un connecteur ne
  portent que sur ses propres outils. Les connecteurs démarrent en parallèle ; une
  panne (serveur, jeton, politiques, noms d'outils en collision) n'écarte que son
  connecteur, et un échec est mémorisé une minute : un serveur muet ne retarde que
  les runs lancés pendant une tentative de démarrage (une par minute, partagée par
  les runs qui l'attendent, ADR-049), du délai de démarrage au plus. L'état (« Non testé », « Opérationnel »,
  « En erreur ») et la dernière erreur, jeton masqué, sont affichés à
  l'administrateur.
- **API REST** : une entreprise décrit une API opération par opération (méthode, chemin,
  paramètres typés) ; chaque opération devient un outil `rest_<préfixe>_<opération>`,
  soumis aux mêmes règles qu'un outil MCP (`connector:use`, `write` par défaut,
  association par l'administrateur, résultats non fiables, adresses publiques). La
  requête ne sort jamais du chemin écrit, la personne qui valide la lit entière, et la
  réponse lue par le modèle est bornée, jeton masqué (ADR-112). Ses opérations peuvent
  être lues d'une description OpenAPI 3 en JSON, que les administrateurs relisent avant
  d'en déclarer : rien n'est téléchargé, enregistré ni appelé à la lecture (ADR-114). Ce
  que partagent les serveurs MCP et les API REST (réseau, préfixe, jeton, masquage) est
  dans `connectors::outbound`.
- Les secrets ne transitent jamais par le contexte du modèle ni par les journaux.

Connecteurs prévus : démonstration (MVP), MCP générique (MVP), déclencheurs (ADR-109),
REST générique (ADR-112),
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
reprise via `Last-Event-ID` (événements numérotés en base). Le processus qui enregistre
un événement le diffuse à ses abonnés en mémoire et l'annonce aux autres sur le canal
PostgreSQL `mimorya_run_events`, dans la transaction qui l'enregistre ; chacun relit
alors l'événement dans le journal du run, dans l'entreprise de l'abonné qui le suit. Les
fragments de texte du modèle voyagent dans l'avis lui-même, quand un autre processus
écoute. Le canal est commun aux entreprises (ADR-084, SECURITY.md §3). Redis ne viendrait
qu'au-delà de ce que PostgreSQL tient.

## 9. Couche LLM

- Interface unique `LlmProvider.complete({system, messages, tools, …})` → texte, appels
  d'outils, usage (tokens, coût estimé).
- Adaptateurs : **Anthropic** (natif), **OpenAI-compatible** (OpenAI, OpenRouter,
  Gemini via point d'accès compatible, Mistral, Ollama, vLLM), **script** (tests
  déterministes).
- **Routage** par classe de tâche (`chat`, `planning`, `verification`, `summarize`,
  `embedding`) configurable par entreprise, avec contraintes de confidentialité
  (ex. « données confidentielles → modèle local uniquement », phase 2). Fait : la
  vérification, par le modèle de contrôle que l'entreprise choisit (ADR-108), celui
  des conversations jugeant à sa place si son appel échoue dans un run.
- **Embeddings** : `EmbeddingProvider` (adaptateur compatible OpenAI `/embeddings` :
  Mistral, OpenAI, serveurs listés par l'exploitant), choisi par l'entreprise et
  désactivé par défaut (ADR-074) ; vecteurs de 1024 dimensions calculés en arrière-plan
  pour les mémoires actives, et pour chaque demande au moment de chercher.
- Suivi des coûts : chaque appel enregistre fournisseur, modèle, tokens et coût estimé
  dans `usage_records`.

## 10. Observabilité et exploitation

- Logs structurés (`tracing`) avec `request_id` et `run_id` ; **jamais** d'en-têtes,
  de secrets, de mots de passe, de jetons ni de contenu de message.
- Journal d'audit métier séparé (table `audit_log`, en ajout seul).
- Événements de run persistés : rejouables pour le support et l'analyse.
- Phase 3 : OpenTelemetry (traces), métriques Prometheus, tableaux de bord
  d'exploitation, alertes de coût.

## 11. Déploiement

| Mode          | Description                                                                                               |
| ------------- | --------------------------------------------------------------------------------------------------------- |
| Développement | `docker compose up db` + `cargo run --bin mimorya-api` / `pnpm dev:web`                                   |
| Privé (PME)   | `docker compose` : web, api (rôle `api`), worker(s) (ADR-087), PostgreSQL ; sauvegardes `pg_dump`/WAL     |
| SaaS          | Même image en rôles `api` (plusieurs) et `worker` (ADR-085), PostgreSQL managé, montée en charge de l'API |

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
| Boucle agentique (`run_conversation`, budgets d'itérations, classification d'erreurs, repli de fournisseur) | **Concept repris**           | Réimplémentée en Rust, avec PEP, budgets de coût, persistance et reprise.                                               |
| Compression de contexte (élagage des sorties d'outils, protection tête/queue, résumé par modèle auxiliaire) | **Concept repris**           | Phase 2 ; prompt système stable pour le cache fournisseur.                                                              |
| Mémoire `MEMORY.md` / `USER.md` bornée, chargée en début de session                                         | **Concept repris**           | Devient la mémoire personnelle bornée ; plus espaces d'équipe/entreprise/expérience, en base, filtrés par autorisation. |
| Recherche dans les sessions passées (SQLite FTS5)                                                           | **Concept repris**           | FTS PostgreSQL + pgvector, filtrée par utilisateur.                                                                     |
| Skills `SKILL.md` + chargement progressif (liste → contenu → références)                                    | **Format adopté**            | Compatibilité agentskills.io, extensions sous `metadata.mimorya.*`, versions en base, évaluations, validation humaine.  |
| Création/patch autonome de skills, revue post-tour, « curator » qui archive                                 | **Concept repris, encadré**  | Propositions en statut `draft`/`candidate`, jamais actives sans validation ; aucune skill ne porte de permission.       |
| Délégation (`delegate_task`, outils interdits aux enfants, profondeur 1, parallélisme borné)                | **Concept repris**           | Sous-agents en runs enfants persistés, plafond de permissions ⊆ parent.                                                 |
| Registre d'outils, toolsets, approbation des commandes dangereuses                                          | **Concept repris**           | Contrat d'outil typé avec niveau de risque ; approbations liées aux niveaux d'autonomie ; pas de shell libre.           |
| Client MCP (stdio, HTTP, OAuth)                                                                             | **Concept repris**           | SDK MCP Rust officiel (`rmcp`) ; association permissions/risques par l'administrateur.                                  |
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
