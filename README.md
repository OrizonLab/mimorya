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

Prérequis : Node.js 22, pnpm 10, Docker (ou PostgreSQL 16 avec l'extension pgvector).

```bash
cp .env.example .env               # renseigner ANTHROPIC_API_KEY (ou un autre fournisseur)
docker compose up -d db
pnpm install
pnpm db:seed                       # migrations + entreprise fictive « Néo Maintenance »
pnpm dev:api                       # http://localhost:4000
NEXT_PUBLIC_DEMO_MODE=true pnpm dev:web   # http://localhost:3000
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

## Qualité

```bash
pnpm check        # typecheck, lint, format, tests unitaires
pnpm test:int     # tests d'intégration PostgreSQL (TEST_DATABASE_URL / TEST_DATABASE_APP_URL)
```
