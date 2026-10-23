# CLAUDE.md — règles de développement

Plateforme de collaborateur IA d'entreprise (nom de code provisoire **Mimorya**).
Lire d'abord : `docs/PRODUCT_VISION.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY.md`,
`docs/AGENT_ENGINE.md`, `docs/DATA_MODEL.md`, `docs/DECISIONS.md`, `docs/ROADMAP.md`.

## Langues

- Documentation, libellés d'interface, messages destinés aux utilisateurs et consignes
  données au modèle : **français**.
- Code, identifiants, commentaires, messages de commit : **anglais** (ADR-013).

## Carte du dépôt

Le serveur est un espace de travail Cargo (Rust, édition 2024) ; l'interface est
l'unique paquet Node (ADR-018).

| Chemin              | Rôle                                                                                          | Règle                                                     |
| ------------------- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `crates/core`       | Domaine pur : autorisation, organigramme, autonomie, contrat d'outil, règles de mémoire, démo | Aucune E/S (ni base, ni réseau, ni fichier).              |
| `crates/db`         | Migrations SQL (sqlx), RLS, dépôts, seed                                                      | Tout accès tenant passe par `Database::begin(tenant_id)`. |
| `crates/agent`      | Fournisseurs LLM, boucle, PEP, budgets, vérification, outils mémoire/compétences              | Dépend de ports (`runtime/ports.rs`), jamais de `db`.     |
| `crates/connectors` | Connecteurs (démo, MCP)                                                                       | Chaque outil filtre ses données avec `ctx.authz`.         |
| `crates/api`        | axum : sessions, routes, SSE, orchestration des runs, binaires `mimorya-api`/`-admin`/`-seed` | Compose les crates ; aucune règle métier dupliquée.       |
| `apps/web`          | Next.js                                                                                       | Parle uniquement à `/api/*` (relayé vers l'API).          |

Repères à l'intérieur des crates :

- `core/src/authz/` (types, moteur, conditions ABAC, organigramme), `core/src/tools/`
  (contrat d'outil, outils typés), `core/src/skill_rules.rs` (ce qu'une compétence peut
  contenir, ADR-052), `core/src/identity_providers/` (fournisseurs d'identité : émetteur de
  chaque type, domaines, ce que prouve un jeton ou une assertion SAML, nom d'un compte
  créé, enregistrement DNS qui prouve un domaine (`domain_proof.rs`), ADR-063, ADR-067,
  ADR-068, ADR-069), `core/src/schedule.rs` (heure d'une tâche planifiée, ADR-080),
  `core/src/demo/` (entreprise fictive par thème).
- `db/migrations/` (SQL numéroté, source de vérité du schéma),
  `db/src/repositories/` (un fichier par entité ; la mémoire : `memory.rs`, recherche
  hybride `memory_search.rs`, vecteurs `memory_embeddings.rs`, ADR-074),
  `db/src/notifications.rs` (canal PostgreSQL des événements des runs, ADR-084), `db/src/seed/` (organisation,
  connecteurs, mémoires, compétences de démonstration), `db/src/testing.rs`.
- `agent/src/llm/<fournisseur>/`, `agent/src/llm/embedding/` (modèles d'embeddings de la
  mémoire sémantique, ADR-074), `agent/src/runtime/` (boucle, appels modèle, appels
  d'outils, revue de réponse, reprise après un arrêt du service, leçon tirée d'un run
  (`lesson.rs`, ADR-076), ports),
  `agent/src/tools/` (registre, PEP,
  `platform/`), `agent/src/testing/` (fonctionnalité `testing`).
- `connectors/src/demo_interventions/` (données, planning, accès, `tools/` un fichier
  par outil), `connectors/src/mcp/`, `connectors/src/bin/mcp_demo_docs.rs`.
- `api/src/plugins/` (contexte de requête, limitation de débit, CSRF (sauf la réception
  des réponses SAML, ADR-068), session,
  en-têtes de sécurité, audit des modifications d'administration refusées, sur une ressource introuvable ou envoyant un identifiant inconnu,
  maintenance périodique : runs bloqués, liens expirés, absences terminées, demandes de
  validation échues et relances avant échéance, ADR-061, ADR-062 ; indexation de la
  mémoire en arrière-plan, `memory_indexing.rs`, ADR-074 ; tâches planifiées lancées
  chaque minute, `scheduled_tasks.rs`, ADR-080 ; baux des runs renouvelés et runs au
  bail expiré repris, `run_leases.rs`, ADR-083 ; événements des autres processus écoutés
  et fragments de texte envoyés, `run_event_relay.rs`, ADR-084),
  `api/src/routes/` (un module par ressource, `admin/`), `api/src/contract/` (réponses
  et corps de requêtes typés, un fichier par ressource, dont sont générés les types
  TypeScript, ADR-036, ADR-039),
  `api/src/services/admin/` (comptes et absence d'un collaborateur empêché
  (`absences.rs`, ADR-078), organisation, habilitations, compétences, sous verrou par
  entreprise), `api/src/services/account.rs` (son propre compte),
  `api/src/services/runs/` (tour de conversation, décisions, contexte, exécution,
  ports PostgreSQL, résultat, reprise au démarrage, ADR-053, circuit des validations,
  avis au responsable (`approval_notice/`, rattrapé au démarrage, ADR-070 ; à l'auteur
  d'une tâche planifiée dont le run attend sa validation, `author.rs`, ADR-082), expiration et
  relance des demandes, ADR-059 à ADR-062, absence et remplaçant
  (`approval_delegation.rs`, ADR-071 à ADR-073), leçon proposée en fin de run
  (`lesson_capture.rs`, ADR-076), bail de l'exécuteur sur chaque run (`leases.rs`,
  ADR-083)),
  `api/src/services/scheduled_tasks/` (tâches planifiées : ce qu'elles demandent, celles
  de chacun, leur lancement, ADR-080), `api/src/services/event_bus/` (événements des runs
  aux abonnés du processus et, par PostgreSQL, des autres : avis, relais, fragments de
  texte, ADR-084),
  `api/src/services/llm.rs` (fournisseur de modèle de
  chaque entreprise), `api/src/services/embeddings/` (modèle d'embeddings de chaque
  entreprise, vecteur d'une demande, indexation de la mémoire, ADR-074), `api/src/services/invitations/` (liens d'invitation, ADR-027),
  `api/src/services/password_reset/` (mot de passe oublié, ADR-028),
  `api/src/services/sso/` (connexion par le fournisseur d'identité de l'entreprise :
  départ (`start.rs`), tentative scellée, OpenID Connect, adresses contrôlées, liaison des
  comptes, acceptation des invitations, ADR-063, ADR-064 ; déclarations et clés gardées une
  heure, ADR-065 ; fournisseur obligatoire pour ses domaines (`requirement.rs`, ADR-066) ;
  compte créé à la première connexion par le lien de l'entreprise (`account.rs`, ADR-067) ;
  SAML 2.0 : métadonnées, vérification stricte et ce que dit l'assertion dans `saml/`,
  réception et assertions à usage unique dans `saml_return.rs`, ADR-068 ; lecture DNS de
  la preuve d'un domaine dans `dns.rs`, ADR-069 ; configuration dans `services/admin/`,
  dont `provider_designation.rs` : ce qui désigne un fournisseur déclaré, et
  `domain_proofs.rs`),
  `api/src/services/provisioning/` et `api/src/bin/admin/` (commandes d'exploitation :
  migrations, création d'une entreprise réelle et de son administrateur, ADR-033),
  `api/src/mail/` (envoi SMTP), `api/src/secrets/` (chiffrement AES-256-GCM des secrets),
  `api/tests/` (intégration, harnais dans `support/`, dont les fournisseurs d'identité
  simulés, OpenID Connect et SAML).
- `web/components/<fonctionnalité>/` (account, admin — dont `admin/llm/` : modèle des
  conversations, recherche dans la mémoire et leçons, ADR-075, ADR-077, et `admin/users/` :
  comptes, dont l'absence d'un collaborateur empêché, ADR-079 —, approvals — dont la
  section « Absence » et son formulaire, commun, ADR-073 —, chat, memory, operations,
  scheduled-tasks — tâches planifiées, ADR-081 —, shell), `web/components/ui/`
  (`Loaded` verrouille son contenu pendant un rechargement, ADR-042), `web/lib/api/` (client, `contract.ts`
  généré par `pnpm contract`, `useApiData`), `web/lib/use-synced-state.ts` (état d'un
  formulaire repris de la liste rechargée, ADR-041), `web/lib/focus-keeper.ts` (focus rendu
  au contrôle réactivé, ADR-043), `web/lib/labels.ts`, `web/lib/format.ts`, `web/proxy.ts` et
  `web/lib/security/` (CSP à nonce par page, ADR-030), `web/e2e/` (tests de bout en
  bout, un fichier par parcours, `support/`) et `api/examples/e2e_server/` (l'API qu'ils
  pilotent, avec un fournisseur OpenID Connect et un fournisseur SAML simulés, ADR-031,
  ADR-063, ADR-068, une relecture des leçons à part, `lessons.rs`, ADR-077, et une tâche
  planifiée dont l'occurrence est manquée au démarrage, `scheduled.rs`, ADR-081).
- `infra/` : `docker/` (images de l'API et de l'interface, liste des images publiées
  `images.json`, test de fumée de la démonstration, test d'installation de la production,
  vérifications partagées `checks.sh`), `caddy/`
  (reverse proxy : TLS, HSTS), `postgres/` (rôles et extensions ; production : `entrypoint-production.sh` vérifie les
  mots de passe, `init-production.sh` crée le rôle applicatif) ; `docker-compose.yml`
  (base de développement, sur 127.0.0.1), `compose.production.yml` (pile de production,
  ADR-034) et `compose.demo.yml` (pile de démonstration,
  ADR-032), qui téléchargent les images publiées (`.github/workflows/images.yml`,
  ADR-051) ; `compose.*.build.yml` les construisent depuis le dépôt (CI).

## Commandes

```bash
pnpm install                       # dépendances de l'interface web
docker compose up -d db            # PostgreSQL 16 + pgvector (ou instance locale, voir README)
pnpm db:seed                       # cargo run --bin mimorya-seed (DATABASE_URL, DEMO_PASSWORD, --reset)
pnpm db:migrate                    # mimorya-admin migrate : migrations seules (ADR-033)
pnpm dev:api                       # cargo run --bin mimorya-api : API sur :4000 (lit .env)
pnpm dev:web                       # Web sur :3000
pnpm contract                      # régénère apps/web/lib/api/contract.ts depuis crates/api/src/contract (ADR-036)
pnpm check                         # Rust : fmt + clippy -D warnings + tests ; web : typecheck + lint + format
pnpm test:int                      # cargo test-int : tests PostgreSQL (en série, base *_test recréée)
pnpm test:e2e                      # Playwright : parcours web contre l'API à modèle scénarisé (ADR-031)
DEMO_PASSWORD=… docker compose -f compose.demo.yml -f compose.demo.build.yml up -d --build   # pile de démonstration construite (ADR-032) ; images publiées : pull puis up -d sans le second fichier (ADR-051)
docker compose --env-file .env.production -f compose.production.yml pull && docker compose --env-file .env.production -f compose.production.yml up -d   # production, images publiées (ADR-034, ADR-051)
```

Avant chaque commit : `pnpm check && pnpm test:int` doivent passer.

## Processus de contribution

1. Travailler sur une branche dédiée, en commits cohérents ; ouvrir une PR (brouillon)
   vers `main`.
2. **Audit obligatoire avant fusion** : lancer l'agent `code-auditor`
   (`.claude/agents/code-auditor.md`, grille `REVIEW.md`) sur la branche. Il est en
   lecture seule, exécute tous les contrôles et rend un verdict.
3. Corriger chaque constat BLOQUANT et MAJEUR, pousser, puis relancer l'audit
   jusqu'au verdict **APPROUVÉ** (les constats MINEUR/SUGGESTION sont notés dans la PR
   ou la feuille de route).
4. CI verte sur le dernier commit + verdict APPROUVÉ → passer la PR en « prête »,
   fusionner dans `main` (squash, message de commit descriptif), puis repartir de
   `main` à jour pour la suite.
5. Ne jamais fusionner avec un contrôle rouge ou non exécuté, ni désactiver un test
   pour obtenir le vert.

## Structure du code

- **Une responsabilité par fichier.** Viser moins de 250 lignes par fichier source,
  400 au maximum ; au-delà, découper par sous-domaine. Les tests suivent la même
  règle (un fichier par thème dans `tests/`, données partagées dans `tests/support/`).
- **Modules par domaine** (fonctionnalité), pas par type technique : chaque module
  expose son API publique dans son `mod.rs` (`pub use`) ; le reste reste privé.
- **Nommage** : Rust — fichiers et fonctions en `snake_case`, types en `PascalCase`,
  constantes en `UPPER_SNAKE_CASE` ; web — fichiers en `kebab-case`, composants React
  en `PascalCase`. Le JSON de l'API est en `camelCase` (`serde(rename_all)`).
- **En-tête de module** : un commentaire `//!` court en tête des fichiers non triviaux
  dit ce que fait le module et pourquoi il existe.
- **Erreurs** : `anyhow` dans les services, `DomainError` (code + message français)
  pour les refus métier, converties en réponse HTTP par `ApiError` ; pas de
  `unwrap()`/`expect()` sur une donnée externe ; aucun `unsafe` (interdit par les lints).
- **API (`crates/api`)** : un module de routes par ressource (`routes/<ressource>.rs`),
  les middlewares transverses dans `plugins/`, la logique d'orchestration dans
  `services/<domaine>/`. Une réponse nouvelle ou modifiée est un type de `contract/`
  (pas de `json!`), de même qu'un corps de requête (strict : `deny_unknown_fields`,
  listé dans `request_bodies!`), puis `pnpm contract` ; l'interface l'importe depuis
  `@/lib/api` (corps envoyés avec `satisfies`, champs écrits un à un : pas de
  décomposition conditionnelle `...(c ? {…} : {})`, que `satisfies` ne vérifie pas).
- **Web (`apps/web`)** : pages minces qui composent des composants ; un composant par
  fichier dans `components/<fonctionnalité>/` ; primitives d'interface dans
  `components/ui/` ; accès API et types dans `lib/api/`.

## Règles d'architecture

1. **Monolithe modulaire** : respecter les frontières de crates ; pas de nouveau
   service ni de nouvelle infrastructure (Redis, file externe…) sans ADR.
2. **Base de données**
   - Toute table métier porte `tenant_id`, la RLS et la politique `tenant_isolation` ;
     le test `crates/db/tests/isolation.rs` échoue sinon.
   - Le runtime utilise exclusivement `DATABASE_APP_URL` (rôle `mimorya_app`, RLS
     appliquée). Le rôle propriétaire ne sert qu'aux migrations et au seed.
   - Migrations : nouveau fichier SQL numéroté dans `crates/db/migrations/`
     (`NNNN_description.sql`), écrit et relu à la main, appliqué par
     `run_migrations` avec le rôle propriétaire. **Ne jamais modifier une migration
     déjà publiée** (sqlx vérifie leur somme de contrôle).
   - Les lectures inter-entreprises n'existent que sous forme de fonctions
     `SECURITY DEFINER` minimales (`auth_*`, `system_*`).
3. **Moteur agentique**
   - Le modèle ne décide jamais d'une autorisation : toute capacité passe par un outil
     évalué par le PEP (`agent/src/tools/executor.rs`).
   - Un nouvel outil implémente `TypedTool` : `ToolInfo` avec permissions requises
     (`requires`), `risk`, `citing(pattern)` s'il renvoie des identifiants ; une
     entrée stricte (`#[serde(deny_unknown_fields)]`, `JsonSchema`, `Validate`) ;
     `describe_action` s'il modifie quelque chose ; des `citations`. Il ne révèle
     jamais l'existence d'une ressource hors périmètre.
   - Les résultats d'outils sont des données non fiables (`wrap_tool_result`) ; seule
     exception, la compétence active chargée par l'outil `skill_read` de la plateforme,
     remise comme procédure de l'entreprise (ADR-052). Aucun texte encadré (résultat,
     compétence, mémoire) ne peut ouvrir ni fermer une enveloppe (`neutralise_envelopes`).
   - Historique en ajout seul : ne jamais réécrire un message déjà envoyé au modèle
     (cache et blocs de raisonnement liés à la conversation).
   - Le prompt système reste stable (pas d'horodatage précis, pas de données
     variables) ; les mémoires récupérées voyagent avec le tour utilisateur.
4. **Fournisseurs LLM** : passer par `LlmProvider` / `ModelRouter`. Modèle par défaut :
   `claude-opus-5-5` (`DEFAULT_ANTHROPIC_MODEL`) ; ne pas disperser d'identifiants de
   modèle dans le code. Prix connus dans `llm/pricing.rs` ; un modèle non tarifé est
   signalé comme tel, jamais estimé au hasard.
5. **API** : chaque route extrait `Auth`, construit le contexte avec
   `access.context()` et vérifie la permission (`require`) ou la propriété de la
   ressource côté serveur ; une ressource hors périmètre répond 404. Les requêtes
   modifiantes exigent l'en-tête `X-Mimorya-Request: 1` et, si le navigateur la donne,
   l'origine de l'application. Une seule en est dispensée, de l'un comme de l'autre : la
   réception des réponses SAML (`POST /api/auth/saml/acs`, ADR-068), que le fournisseur
   poste depuis son propre site. Seules la lient au navigateur la tentative scellée dans
   son cookie, la demande qu'elle attend et la signature de l'assertion. Aucune autre
   exception sans ADR ; le test `only_the_saml_reception_goes_without_the_header`
   (`crates/api/tests/auth.rs`) tente de la contourner.

## Règles de sécurité (non négociables)

- Permissions non délégables (`NON_DELEGABLE_PERMISSIONS`) : toute nouvelle permission
  d'administration doit y figurer (`delegable: false`).
- Aucune compétence, mémoire ou consigne ne peut modifier une permission, une politique
  ou un niveau d'autonomie.
- Jamais de secret, jeton, mot de passe ni contenu de message dans les logs ou l'audit.
- Les paramètres validés par un humain sont exécutés tels quels.
- Toute nouvelle donnée personnelle : rétention, export et suppression prévus.
- Liste de contrôle complète : `docs/SECURITY.md` §11.

## Tests

- Unitaires : `#[cfg(test)]` dans le module ou `tests/*.rs` ; intégration PostgreSQL :
  tests marqués `#[ignore = "requires PostgreSQL"]`, lancés par `cargo test-int`
  (en série ; base `mimorya_test` recréée une fois par binaire de test).
- Le modèle est simulé avec `ScriptedProvider` (`mimorya_agent::testing`, fonctionnalité
  `testing`), le modèle d'embeddings avec `ConceptEmbedder` ; les ports avec
  `InMemoryPorts`. Aucun test n'appelle un fournisseur réel.
- Bout en bout : `apps/web/e2e/` (Playwright, `pnpm test:e2e`, en CI à chaque PR) contre
  l'exemple `e2e_server` (modèle scénarisé, e-mails écrits dans un dossier) ; chaque test
  échoue sur une violation de la CSP ou une erreur de page. Un parcours web nouveau ou
  modifié s'accompagne de son test. Les tests partagent l'entreprise de démonstration
  (recréée à chaque démarrage) et s'exécutent dans l'ordre des fichiers : chacun la
  laisse sans effet sur les suivants. Ce qu'il crée, il le supprime quand l'API le
  permet, même s'il échoue quand l'effet serait gênant (`afterEach`) ; un compte, que
  l'API ne supprime pas, porte une adresse propre au test et ne garde aucun rôle métier.
  Une assertion vérifie ce que l'API a enregistré (relu après rechargement), pas l'état
  local d'un formulaire.
- Toute règle d'autorisation ou d'isolation nouvelle s'accompagne d'un test qui tente
  de la contourner.

## Honnêteté produit

- Une fonctionnalité de démonstration est étiquetée comme telle (données fictives,
  badge « Démonstration »).
- Mettre à jour les statuts de `docs/ROADMAP.md` à chaque contribution ; ne jamais
  marquer ✅ ce qui n'est pas couvert par un test.
- Toute décision structurante → nouvelle entrée dans `docs/DECISIONS.md`.

## Données de démonstration

Entreprise fictive « Néo Maintenance » (`crates/core/src/demo`), connecteur
`demo_interventions` (`crates/connectors/src/demo_interventions`). Comptes
`prenom.nom@neo-maintenance.example`, mot de passe `DEMO_PASSWORD` (défaut de
développement : `demo-mimorya`, refusé en production). Le serveur de démonstration MCP :
`cargo run -p mimorya-connectors --bin mcp-demo-docs`.
