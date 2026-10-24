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
export DEMO_PASSWORD='…'                         # obligatoire (autre que demo-mimorya) : comptes de démo
docker compose -f compose.demo.yml pull          # images publiées (ADR-051)
docker compose -f compose.demo.yml up -d         # https://localhost (certificat local de Caddy)
infra/docker/smoke-test.sh                       # vérification : HSTS, CSP, API, connexion, rôles
```

La même image tourne en deux services (ADR-087) : `api` sert l'API et met en file les runs
qu'il lance, `worker` les exécute avec toutes les tâches de fond, sans ouvrir de port.
Ajouter des workers côte à côte (ADR-086) : `MIMORYA_WORKERS=2` (1 par défaut, au moins
1, dans `.env` ou l'environnement exporté), puis `docker compose -f compose.demo.yml up -d`.
L'API reste unique dans la pile : Caddy n'en répartit pas plusieurs. Ses limites
d'authentification sont partagées par la base (ADR-088) ; sa limite globale par requête
reste comptée par processus. Le test de fumée
attend autant de workers que la pile en lance ; il lit la configuration avec `jq`.

Les images sont publiées sur le registre de GitHub (`ghcr.io/orizonlab/mimorya-api`,
`mimorya-web`, `mimorya-web-demo`, pour amd64), une fois la CI verte, pour chaque commit
de `main` (ADR-051). `MIMORYA_VERSION` choisit lesquelles : une version (`1.2.0`, nommée
par une étiquette `v1.2.0` sur un commit de `main`), un commit (`sha-` suivi de ses
12 premiers caractères) ou `latest` (par défaut, le dernier commit de `main`). Une image
introuvable arrête la pile : elle n'est jamais construite à sa place. Utilisez le fichier
compose de la même version ou du même commit (`git checkout v1.2.0`) : il monte des
fichiers du dépôt (Caddy, initialisation de la base). Pour construire les images depuis
le dépôt plutôt que les télécharger, sous des noms locaux :
`docker compose -f compose.demo.yml -f compose.demo.build.yml up -d --build`.

Seul Caddy publie des ports (80 et 443) ; la base n'est joignable que par les conteneurs
de la pile. Variables facultatives (fichier `.env` ou environnement) : `ANTHROPIC_API_KEY`
(ou `LLM_PROVIDER`, `LLM_API_KEY`, `LLM_BASE_URL`, `LLM_MODEL`) pour converser,
`MIMORYA_MASTER_KEY`, `SMTP_URL` et `MAIL_FROM`, `MIMORYA_DOMAIN` et `MIMORYA_TLS` (une
adresse e-mail pour un certificat public ; `internal` par défaut, certificat local).

**À savoir avant de l'exposer** : la pile contient des comptes d'administration fictifs
que la page de connexion liste, et qui se connectent tous avec `DEMO_PASSWORD` ; choisissez
un mot de passe fort et n'ouvrez pas la pile à des personnes à qui vous ne le confiez pas.
Ce n'est pas une installation de production : elle n'existe qu'avec les données de
démonstration (voir la feuille de route).

## Installation privée (exploitation)

Sans données fictives : la commande d'exploitation `mimorya-admin` (ADR-033), livrée dans
l'image de l'API, applique les migrations puis crée chaque entreprise avec son premier
administrateur, invité par un lien.

Prérequis : PostgreSQL 16 avec l'extension pgvector ; un rôle propriétaire du schéma
(`DATABASE_URL`) qui peut créer les extensions `citext` et `vector` ; le rôle applicatif
`mimorya_app` créé **avant** les migrations, avec la connexion et un mot de passe choisi
(sinon les migrations le créent sans droit de connexion : `ALTER ROLE mimorya_app LOGIN
PASSWORD '…'` ensuite) :

```sql
CREATE ROLE mimorya_app LOGIN PASSWORD '…' NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
```

```bash
mimorya-admin migrate          # DATABASE_URL (rôle propriétaire) : schéma seul, sans données
mimorya-admin create-company --name 'Acme Industrie' --slug acme \
  --admin-email jeanne.martin@acme.example --admin-name 'Jeanne Martin'
mimorya-admin invite --email jeanne.martin@acme.example   # nouveau lien, si le premier a expiré
```

`create-company` et `invite` utilisent `DATABASE_APP_URL` et `WEB_ORIGIN` comme l'API. Le
lien d'invitation (valable 7 jours) n'est affiché qu'une fois : transmettez-le par un canal
sûr, l'administrateur y choisit son mot de passe puis construit son organisation depuis
l'administration. Lancez ces commandes dans un terminal (par exemple
`docker compose exec api mimorya-admin …`), pas comme une tâche dont la sortie est
conservée par un collecteur de journaux : le lien y resterait. En développement :
`pnpm db:migrate`.

### Pile conteneurisée de production

`compose.production.yml` (ADR-034) installe la même chose en conteneurs, sans données
fictives : PostgreSQL (rôles créés avec vos mots de passe), migrations, API, worker
(ADR-087), interface et Caddy (HTTPS, HSTS), dans son propre projet (`mimorya-production`, à part de la base de
développement). Seul Caddy publie des ports. Les identifiants se choisissent dans un
fichier d'environnement gardé hors du dépôt, lisible par vous seul :

```bash
(umask 077 && cat > .env.production <<VARIABLES
POSTGRES_PASSWORD=$(openssl rand -hex 32)
MIMORYA_APP_DB_PASSWORD=$(openssl rand -hex 32)
MIMORYA_DOMAIN=mimorya.acme.example
MIMORYA_TLS=exploitation@acme.example
MIMORYA_VERSION=1.2.0            # images publiées à installer (ADR-051) : version ou sha-…
MIMORYA_WORKERS=1                # workers qui exécutent les runs (ADR-087)
VARIABLES
)
docker compose --env-file .env.production -f compose.production.yml pull
docker compose --env-file .env.production -f compose.production.yml up -d
docker compose --env-file .env.production -f compose.production.yml exec api \
  mimorya-admin create-company --name 'Acme Industrie' --slug acme \
  --admin-email jeanne.martin@acme.example --admin-name 'Jeanne Martin'
```

Fixer `MIMORYA_VERSION` (une version, sinon un commit `sha-…` listé dans les paquets du
dépôt) garde la même version d'un redémarrage à l'autre ; sans elle, `latest` suit le
dernier commit de `main`. Lancez ces commandes depuis le dépôt extrait à la même version
(`git checkout v1.2.0`) : le fichier compose et les fichiers qu'il monte (Caddy,
initialisation de la base) vont avec les images. Pour mettre à jour : extraire la
nouvelle version, changer `MIMORYA_VERSION`, puis relancer `pull` et `up -d` (les
migrations s'appliquent au démarrage ; un run interrompu par la mise à jour est repris par
un worker). Depuis une version qui lançait un seul conteneur `api` en rôle `all` (avant
l'ADR-086), arrêtez-le d'abord (`… stop api`). Pour plus de workers, augmentez
`MIMORYA_WORKERS` dans `.env.production` puis relancez `up -d` : chaque `up -d` réapplique
ce nombre. À l'arrêt, un worker a deux minutes pour finir ses runs. Une image introuvable arrête la pile, jamais
remplacée par une construction locale. Pour construire depuis le dépôt plutôt que
télécharger, sous des noms locaux : ajouter `-f compose.production.build.yml` et
`up -d --build`.

Les deux mots de passe de base passent dans des URL de connexion : lettres et chiffres
seulement, 16 au moins. La base refuse de démarrer sinon, sans rien créer, et le redit à
chaque redémarrage jusqu'à correction. La base ne les lit qu'à sa création ; les migrations
et l'API s'en servent à chaque démarrage. Pour en changer, modifiez d'abord le rôle, la
saisie restant masquée et hors de l'historique (`docker compose … exec db psql -U mimorya`,
puis `\password mimorya_app`, de même pour `mimorya`), puis la variable, puis relancez
`up -d`. `MIMORYA_TLS` vaut une
adresse e-mail pour un certificat public (ports 80 et 443 joignables depuis Internet) ou
`internal` (certificat local de Caddy). Mêmes variables facultatives que la pile de
démonstration (fournisseur de modèle, `MIMORYA_MASTER_KEY`, relais SMTP). La CI suit ce
parcours, rôles et second worker compris, jusqu'à la première connexion de
l'administrateur avec les images construites depuis le dépôt
(`compose.production.build.yml`), pas avec les images téléchargées
(`infra/docker/production-test.sh`, réservé à une pile jetable : il y laisse une
entreprise de vérification). Les sauvegardes restent à faire (voir la feuille de route).

## Qualité

```bash
pnpm check        # Rust : fmt, clippy, tests unitaires ; web : typecheck, lint, format
pnpm test:int     # = cargo test-int : tests PostgreSQL (TEST_DATABASE_URL / TEST_DATABASE_APP_URL)
pnpm test:e2e     # parcours dans Chromium (Playwright) contre l'API avec un modèle scénarisé
pnpm contract     # régénère apps/web/lib/api/contract.ts après un changement du contrat de l'API (ADR-036)
```

Les tests de bout en bout (ADR-031) recréent l'entreprise de démonstration de la base
désignée par `E2E_DATABASE_URL` / `E2E_DATABASE_APP_URL` (par défaut, celle du
développement) et utilisent les ports 3000 et 4000 : arrêtez d'abord `pnpm dev:api` et
`pnpm dev:web`. Navigateur : `pnpm --filter @mimorya/web exec playwright install chromium`.

## Organisation du dépôt

| Chemin              | Contenu                                                                                           |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| `crates/core`       | Domaine pur : autorisation, organigramme, autonomie, contrat d'outil, démo                        |
| `crates/db`         | Migrations SQL, RLS, dépôts, seed                                                                 |
| `crates/agent`      | Fournisseurs LLM, boucle agentique, PEP, vérification, budgets                                    |
| `crates/connectors` | Connecteur de démonstration, pont MCP                                                             |
| `crates/extract`    | Lecture des documents déposés, dans le processus `mimorya-extract`                                |
| `crates/api`        | Serveur HTTP (axum), binaires `mimorya-api`, `mimorya-admin`, `mimorya-seed` et `mimorya-extract` |
| `apps/web`          | Interface Next.js                                                                                 |
