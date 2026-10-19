# Mimorya

Plateforme de **collaborateur IA d'entreprise** (nom de code provisoire) : un agent qui
comprend l'organisation, agit avec les permissions de chaque collaborateur, demande une
validation humaine pour les actions sensibles, vérifie ses résultats et capitalise sur
l'expérience validée.

> Statut : **fondations du MVP** — voir [docs/ROADMAP.md](docs/ROADMAP.md) pour ce qui
> est opérationnel, en démonstration ou planifié.

## Documentation

| Document                                 | Contenu                                                        |
| ---------------------------------------- | -------------------------------------------------------------- |
| [Vision produit](docs/PRODUCT_VISION.md) | Objectifs, personas, principes, glossaire                      |
| [Architecture](docs/ARCHITECTURE.md)     | Vue d'ensemble, pile, flux, analyse de Hermes Agent, risques   |
| [Moteur agentique](docs/AGENT_ENGINE.md) | Boucle, PEP, validations, vérification, mémoire, apprentissage |
| [Modèle de données](docs/DATA_MODEL.md)  | Entités, isolation, rétention                                  |
| [Sécurité](docs/SECURITY.md)             | Modèle de menace, isolation, autorisations, RGPD               |
| [Décisions](docs/DECISIONS.md)           | Journal des décisions d'architecture                           |
| [Feuille de route](docs/ROADMAP.md)      | Phases, statuts, critères d'acceptation                        |
| [CLAUDE.md](CLAUDE.md)                   | Règles de développement                                        |

## Démarrage local

Le serveur (API, moteur agentique, connecteurs) est écrit en **Rust** ; l'interface
web en **Next.js** (ADR-018).

Prérequis : Rust stable (1.88 ou plus récent), Node.js 22 et pnpm 10 pour l'interface,
Docker (ou PostgreSQL 16 avec l'extension pgvector).

```bash
cp .env.example .env               # renseigner ANTHROPIC_API_KEY (ou un autre fournisseur)
docker compose up -d db
pnpm install                       # dépendances de l'interface web
pnpm db:seed                       # = cargo run --bin mimorya-seed : migrations + entreprise fictive
pnpm dev:api                       # = cargo run --bin mimorya-api : http://localhost:4000
NEXT_PUBLIC_DEMO_MODE=true pnpm dev:web   # http://localhost:3000 (comptes de démo affichés)
```

Sans Docker, créer les rôles et les extensions comme dans
[`infra/postgres/init.sql`](infra/postgres/init.sql) (le rôle propriétaire doit pouvoir
créer des bases pour les tests d'intégration).

Comptes de démonstration (données fictives) : `marc.lefevre@neo-maintenance.example`
(responsable technique), `julien.moreau@…` (chef d'équipe), `lucas.petit@…` (technicien),
`claire.martin@…` (direction), `isabelle.laurent@…` (administratif), `paul.renaud@…`
(administrateur technique) — mot de passe : valeur de `DEMO_PASSWORD`.

Sans fournisseur de modèle configuré, l'application fonctionne mais les conversations
sont désactivées (message explicite).

## Pile conteneurisée (démonstration)

Images Docker de l'API et de l'interface, derrière Caddy (HTTPS, HSTS), avec l'entreprise
**fictive** de démonstration (ADR-032) :

```bash
export DEMO_PASSWORD='…'                         # obligatoire : mot de passe de tous les comptes de démo
docker compose -f compose.demo.yml up -d --build # https://localhost (certificat local de Caddy)
infra/docker/smoke-test.sh                       # vérification : HSTS, CSP, API, connexion
```

Seul Caddy publie des ports (80 et 443) ; la base n'est joignable que par les conteneurs
de la pile. Variables facultatives (fichier `.env` ou environnement) : `ANTHROPIC_API_KEY`
(ou `LLM_PROVIDER`, `LLM_API_KEY`, `LLM_BASE_URL`, `LLM_MODEL`) pour converser,
`MIMORYA_MASTER_KEY`, `SMTP_URL` et `MAIL_FROM`, `MIMORYA_DOMAIN` (puis retirer
`tls internal` de `infra/caddy/Caddyfile` pour un certificat public).

**À savoir avant de l'exposer** : la pile contient des comptes d'administration fictifs
que la page de connexion liste, et qui se connectent tous avec `DEMO_PASSWORD` ; choisissez
un mot de passe fort et n'ouvrez pas la pile à des personnes à qui vous ne le confiez pas.
Ce n'est pas une installation de production : elle n'existe qu'avec les données de
démonstration (voir la feuille de route).

## Qualité

```bash
pnpm check        # Rust : fmt, clippy, tests unitaires ; web : typecheck, lint, format
pnpm test:int     # = cargo test-int : tests PostgreSQL (TEST_DATABASE_URL / TEST_DATABASE_APP_URL)
pnpm test:e2e     # parcours dans Chromium (Playwright) contre l'API avec un modèle scénarisé
```

Les tests de bout en bout (ADR-031) recréent l'entreprise de démonstration de la base
désignée par `E2E_DATABASE_URL` / `E2E_DATABASE_APP_URL` (par défaut, celle du
développement) et utilisent les ports 3000 et 4000 : arrêtez d'abord `pnpm dev:api` et
`pnpm dev:web`. Navigateur : `pnpm --filter @mimorya/web exec playwright install chromium`.

## Organisation du dépôt

| Chemin              | Contenu                                                                    |
| ------------------- | -------------------------------------------------------------------------- |
| `crates/core`       | Domaine pur : autorisation, organigramme, autonomie, contrat d'outil, démo |
| `crates/db`         | Migrations SQL, RLS, dépôts, seed                                          |
| `crates/agent`      | Fournisseurs LLM, boucle agentique, PEP, vérification, budgets             |
| `crates/connectors` | Connecteur de démonstration, pont MCP                                      |
| `crates/api`        | Serveur HTTP (axum), binaires `mimorya-api` et `mimorya-seed`              |
| `apps/web`          | Interface Next.js                                                          |
