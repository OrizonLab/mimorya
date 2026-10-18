# CLAUDE.md — règles de développement

Plateforme de collaborateur IA d'entreprise (nom de code provisoire **Mimorya**).
Lire d'abord : `docs/PRODUCT_VISION.md`, `docs/ARCHITECTURE.md`, `docs/SECURITY.md`,
`docs/AGENT_ENGINE.md`, `docs/DATA_MODEL.md`, `docs/DECISIONS.md`, `docs/ROADMAP.md`.

## Langues

- Documentation, libellés d'interface, messages destinés aux utilisateurs et consignes
  données au modèle : **français**.
- Code, identifiants, commentaires, messages de commit : **anglais** (ADR-013).

## Carte du dépôt

| Chemin                | Rôle                                                                                          | Règle                                                 |
| --------------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `packages/core`       | Domaine pur : autorisation, organigramme, autonomie, contrat d'outil, règles de mémoire, démo | Aucune E/S. Seule dépendance : `zod`.                 |
| `packages/db`         | Schéma Drizzle, migrations, RLS, dépôts, seed                                                 | Tout accès passe par `withTenant()`.                  |
| `packages/agent`      | Fournisseurs LLM, boucle, PEP, budgets, vérification, outils mémoire/compétences              | Dépend de ports (`runtime/ports.ts`), jamais de `db`. |
| `packages/connectors` | Connecteurs (démo, MCP)                                                                       | Chaque outil filtre ses données avec `ctx.authz`.     |
| `apps/api`            | Fastify : sessions, routes, SSE, orchestration des runs                                       | Compose les paquets ; aucune règle métier dupliquée.  |
| `apps/web`            | Next.js                                                                                       | Parle uniquement à `/api/*` (relayé vers l'API).      |

## Commandes

```bash
pnpm install
docker compose up -d db            # PostgreSQL 16 + pgvector (ou instance locale, voir README)
pnpm db:seed                       # migrations + entreprise fictive (DATABASE_URL, DEMO_PASSWORD)
pnpm dev:api                       # API sur :4000 (lit ../../.env)
pnpm dev:web                       # Web sur :3000
pnpm check                         # typecheck + lint + format + tests unitaires
pnpm test:int                      # tests d'intégration (PostgreSQL requis, base *_test recréée)
pnpm db:generate                   # migration à partir du schéma Drizzle
```

Avant chaque commit : `pnpm check && pnpm test:int` doivent passer.

## Règles d'architecture

1. **Monolithe modulaire** : respecter les frontières de paquets ; pas de nouveau
   service ni de nouvelle infrastructure (Redis, file externe…) sans ADR.
2. **Base de données**
   - Toute table métier porte `tenant_id` et la politique `tenantIsolation()` ; le test
     `isolation.int.test.ts` échoue sinon.
   - Le runtime utilise exclusivement `DATABASE_APP_URL` (rôle `mimorya_app`, RLS
     appliquée). Le rôle propriétaire ne sert qu'aux migrations et au seed.
   - Migrations : `pnpm db:generate` puis relecture du SQL ; SQL spécifique (rôles,
     fonctions, index) via `drizzle-kit generate --custom`. **Ne jamais modifier une
     migration déjà publiée.**
   - Les lectures inter-entreprises n'existent que sous forme de fonctions
     `SECURITY DEFINER` minimales (`auth_*`, `system_*`).
3. **Moteur agentique**
   - Le modèle ne décide jamais d'une autorisation : toute capacité passe par un outil
     évalué par le PEP (`tools/executor.ts`).
   - Un nouvel outil déclare `requiredPermissions`, `risk`, un schéma Zod strict,
     `describeAction` s'il modifie quelque chose, `citationPattern` s'il renvoie des
     identifiants, et des `citations`. Il ne révèle jamais l'existence d'une ressource
     hors périmètre.
   - Les résultats d'outils sont des données non fiables (`wrapUntrusted`).
   - Historique en ajout seul : ne jamais réécrire un message déjà envoyé au modèle
     (cache et blocs de raisonnement liés à la conversation).
   - Le prompt système reste stable (pas d'horodatage précis, pas de données
     variables) ; les mémoires récupérées voyagent avec le tour utilisateur.
4. **Fournisseurs LLM** : passer par `LlmProvider` / `ModelRouter`. Modèle par défaut :
   `claude-opus-5-5` (`DEFAULT_ANTHROPIC_MODEL`) ; ne pas disperser d'identifiants de
   modèle dans le code. Prix connus dans `llm/pricing.ts` ; un modèle non tarifé est
   signalé comme tel, jamais estimé au hasard.
5. **API** : chaque route appelle `requireAuth`, construit le contexte avec
   `access.context()` et vérifie la permission ou la propriété de la ressource côté
   serveur. Les requêtes modifiantes exigent l'en-tête `X-Mimorya-Request: 1`.

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

- Unitaires : `*.test.ts` ; intégration PostgreSQL : `*.int.test.ts` (fichiers exécutés
  en série, base `mimorya_test` recréée).
- Le modèle est simulé avec `ScriptedProvider` (`@mimorya/agent/testing`) ; les ports
  avec `InMemoryPorts`. Aucun test n'appelle un fournisseur réel.
- Toute règle d'autorisation ou d'isolation nouvelle s'accompagne d'un test qui tente
  de la contourner.

## Honnêteté produit

- Une fonctionnalité de démonstration est étiquetée comme telle (données fictives,
  badge « Démonstration »).
- Mettre à jour les statuts de `docs/ROADMAP.md` à chaque contribution ; ne jamais
  marquer ✅ ce qui n'est pas couvert par un test.
- Toute décision structurante → nouvelle entrée dans `docs/DECISIONS.md`.

## Données de démonstration

Entreprise fictive « Néo Maintenance » (`packages/core/src/demo`), connecteur
`demo_interventions` (`packages/connectors/src/demo-interventions`). Comptes
`prenom.nom@neo-maintenance.example`, mot de passe `DEMO_PASSWORD` (défaut de
développement : `demo-mimorya`). Le serveur de démonstration MCP :
`pnpm --filter @mimorya/connectors mcp:demo-docs`.
