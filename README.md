# Mimorya

Plateforme de **collaborateur IA d'entreprise** (nom de code provisoire) : un agent qui
comprend l'organisation, agit avec les permissions de chaque collaborateur, demande une
validation humaine pour les actions sensibles, vérifie ses résultats et capitalise sur
l'expérience validée.

> Statut : **fondations du MVP** — voir [docs/ROADMAP.md](docs/ROADMAP.md) pour ce qui
> est opérationnel, en démonstration ou planifié.

## Documentation

| Document                                   | Contenu                                                           |
| ------------------------------------------ | ----------------------------------------------------------------- |
| [Vision produit](docs/PRODUCT_VISION.md)   | Objectifs, personas, principes, glossaire                         |
| [Architecture](docs/ARCHITECTURE.md)       | Vue d'ensemble, pile, flux, analyse de Hermes Agent, risques      |
| [Moteur agentique](docs/AGENT_ENGINE.md)   | Boucle, PEP, validations, vérification, mémoire, apprentissage    |
| [Modèle de données](docs/DATA_MODEL.md)    | Entités, isolation, rétention                                     |
| [Sécurité](docs/SECURITY.md)               | Modèle de menace, isolation, autorisations, RGPD                  |
| [Décisions](docs/DECISIONS.md)             | Journal des décisions d'architecture                              |
| [Feuille de route](docs/ROADMAP.md)        | Phases, statuts, critères d'acceptation                           |
| [Plan de reprise](docs/RECOVERY_PLAN.md)   | Pertes et durées visées, préparation, procédures, exercices       |
| [Installation](docs/INSTALLATION.md)       | Installer la bêta pas à pas sur un serveur neuf, la mettre à jour |
| [Recette de la bêta](docs/RECETTE_BETA.md) | Essais à faire avant d'ouvrir, résultats attendus, problèmes      |
| [CLAUDE.md](CLAUDE.md)                     | Règles de développement                                           |

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
pnpm dev:api                       # = cargo run --bin mimorya-api : http://localhost:4000 (construit aussi mimorya-extract, qui lit les documents déposés)
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
`mimorya-web`, `mimorya-web-demo` et `mimorya-db`, la base de la pile de production,
pour amd64), une fois la CI verte, pour chaque commit
de `main` (ADR-051). `MIMORYA_VERSION` choisit lesquelles : une version (`1.2.0`, nommée
par une étiquette `v1.2.0` sur un commit de `main`), un commit (`sha-` suivi de ses
12 premiers caractères) ou `latest` (par défaut : il suit `main`, et peut rester sur un
commit antérieur jusqu'à la fusion suivante, ADR-051). Une image introuvable arrête la
pile : elle n'est jamais construite à sa place. Utilisez le fichier compose de la même
version ou du même commit (`git checkout v1.2.0`) : il monte des fichiers du dépôt
(Caddy, initialisation de la base). Pour construire les images depuis le dépôt plutôt
que les télécharger, sous des noms locaux :
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
mimorya-admin suspend-company --slug acme      # plus personne ne s'y connecte ; les délais courent toujours (ADR-191)
mimorya-admin reactivate-company --slug acme   # la rétablit, moins ce que ces délais ont retiré
mimorya-admin release-company --slug acme      # révoque ce que la plateforme a déclaré pour elle ailleurs (ADR-193)
mimorya-admin delete-company --slug acme       # DATABASE_URL : montre ce qu'une entreprise suspendue garde (ADR-192)
mimorya-admin delete-company --slug acme --confirm acme   # la supprime avec tout, son journal d'audit compris
mimorya-admin verify-audit --anchor <entreprise>:<place>:<sceau>   # journal d'audit scellé (ADR-178)
mimorya-admin model-supply --slug acme         # DATABASE_URL : formule de fourniture du modèle, consommation et solde (ADR-208)
mimorya-admin set-model-supply --slug acme --formula capped --cap 250   # clé de l'exploitant, 250 USD par mois
mimorya-admin set-model-supply --slug acme --formula prepaid            # clé de l'exploitant contre un crédit prépayé
mimorya-admin set-markup --slug acme --percent 15                       # coefficient de refacturation
mimorya-admin add-credit --slug acme --amount 500 --reference FAC-2026-0042   # versement ; négatif pour une correction
mimorya-admin set-supply-alerts --slug acme --cap 80,95 --credit 20,5    # parts qui alertent les administrateurs (ADR-211)
```

`create-company`, `invite`, `suspend-company`, `reactivate-company`, `release-company` et
`verify-audit` utilisent `DATABASE_APP_URL` comme l'API, les deux premières `WEB_ORIGIN`,
les deux dernières `MIMORYA_MASTER_KEY`. Une suspension est tracée dans le journal d'audit de l'entreprise, et
annoncée à chaque processus, qui oublie l'accès, le modèle et les embeddings de l'entreprise
et ferme les instances de ses connecteurs : au mieux, comme toute invalidation (ADR-085).
`delete-company` passe par le rôle propriétaire (`DATABASE_URL`), seul à pouvoir supprimer
une entreprise : dans la pile de production,
`docker compose run --rm migrate mimorya-admin delete-company --slug acme --confirm acme`.
La fourniture du modèle d'une entreprise (ADR-208) aussi : le rôle applicatif ne fait que la
lire. Une entreprise neuve est en formule 1, sa propre clé : sans elle, elle n'a pas de
modèle, jusqu'à ce que l'exploitant lui donne la formule 2, la clé de l'exploitant dans un
plafond mensuel (`--formula capped`, 100 USD sans `--cap`, sans plafond seulement avec
`--cap unlimited`), ou 3, contre un crédit prépayé (`--formula prepaid`, puis `add-credit`).
`model-supply` montre la formule, le coefficient, la consommation du mois sur la clé de
l'exploitant, au coût réel et au montant refacturé, les versements et le solde, recalculé à
chaque fois. Chaque changement est tracé dans le journal d'audit de l'entreprise (acteur
`system`) ; un versement n'est jamais modifié : une correction est un versement de plus,
avec sa propre référence. Les administrateurs des réglages de l'entreprise sont prévenus
par e-mail quand le mois atteint une part du plafond, ou que le solde passe sous une part
du dernier versement : par défaut 80 et 95 % du plafond, 20 puis 5 % du versement, réglés
par `set-supply-alerts` (`--cap`, `--credit` ou les deux ; `aucun` pour n'en garder
aucune ; `100` pour le plafond atteint, `0` pour le crédit épuisé). Chaque alerte part une
fois pour son mois et son plafond, ou pour son versement : un plafond changé ou un nouveau
versement la réarme (ADR-211).
Gardez sa sortie : le journal d'audit de l'entreprise part avec elle, et ses sauvegardes la
gardent jusqu'à leur retrait (après une restauration, supprimez-la de nouveau). Avant elle,
`release-company`, dans le service `api` qui a la clé maître
(`docker compose exec api mimorya-admin release-company --slug acme`), révoque chez leur
serveur d'autorisation les autorisations OAuth de ses serveurs MCP, retire ses bots
Telegram avec leur webhook et révoque chez Google les accords des comptes Google Workspace
reliés par ses personnes (ADR-194) ; la suppression est refusée tant qu'il en reste. Elle
dit ce qui reste chez d'autres services (comptes Microsoft 365 reliés, dont Microsoft ne
révoque pas l'accord d'une seule application, applications Slack et Teams) : à retirer chez
chacun. Le journal d'audit de chaque
entreprise est scellé chaque minute sous une clé qui en dérive. Le dernier sceau posé dans
chaque entreprise à chaque passage, et celui trouvé au démarrage, sont journalisés au
niveau `info` (`LOG_LEVEL=info`, par défaut) avec leur ancre toute écrite (`Audit log
sealed`, `Audit chain found` : `anchor=<entreprise>:<place>:<sceau>`) : conservez ces lignes
hors de la base. `verify-audit` échoue si une entrée scellée a été modifiée ou supprimée,
ou si une chaîne ne porte plus le sceau d'une ancre donnée telle quelle par `--anchor`
(autant que voulu) : seul ce sceau montre des entrées retirées de la fin d'une chaîne, ou
des sceaux posés à nouveau. Après une restauration, redémarrez l'API et les workers, et
vérifiez avec les ancres d'avant la sauvegarde : elles sont retrouvées, celles posées
depuis ne le sont plus, ce que la CI vérifie à chaque PR (ADR-180). Chaque entreprise vérifie aussi sa chaîne
depuis l'onglet « Journal d'audit » de son administration, contre les ancres qu'elle a
gardées (ADR-179), et le fait lire par son SIEM avec un jeton d'export, page après page
(`GET /api/audit/export`, sans `seal` au départ, puis `?after=<place>&seal=<sceau de cette
place>`, `Authorization: Bearer mae_…` ; une place que la chaîne restaurée ne porte plus
est refusée, ADR-181), ses jetons gérés depuis le même onglet (ADR-182). Le fournisseur d'identité de l'entreprise (Entra ID, Okta…) peut créer, lire, changer,
désactiver et retirer ses comptes par SCIM 2.0 à `<adresse de l'application>/api/scim/v2`
(un compte retiré est désactivé et sort de la vue de SCIM ; ses données restent sur la
plateforme jusqu'à ce qu'un administrateur l'efface, ADR-189), avec un jeton SCIM
(`Authorization: Bearer msc_…`) fait par un administrateur des comptes depuis l'onglet
« Utilisateurs » (ADR-183, ADR-184). Le
lien d'invitation (valable 7 jours) n'est affiché qu'une fois : transmettez-le par un canal
sûr, l'administrateur y choisit son mot de passe puis construit son organisation depuis
l'administration. Lancez ces commandes dans un terminal (par exemple
`docker compose exec api mimorya-admin …`), pas comme une tâche dont la sortie est
conservée par un collecteur de journaux : le lien y resterait. En développement :
`pnpm db:migrate`.

### Pile conteneurisée de production

Pour une première installation sur un serveur neuf, suivez le guide pas à pas,
`docs/INSTALLATION.md`, puis faites les essais de `docs/RECETTE_BETA.md` avant d'ouvrir la
plateforme (ADR-224). Ce qui suit en est le détail.

`compose.production.yml` (ADR-034) installe la même chose en conteneurs, sans données
fictives : PostgreSQL (rôles créés avec vos mots de passe), migrations, API, worker
(ADR-087), interface et Caddy (HTTPS, HSTS), dans son propre projet (`mimorya-production`, à part de la base de
développement). Seul Caddy publie des ports. Les identifiants se choisissent dans un
fichier d'environnement gardé hors du dépôt, lisible par vous seul.
`.env.production.example` décrit chaque variable en trois parties (obligatoires,
recommandées, facultatives) : son rôle, son défaut, comment la produire et ce qui se passe
sans elle (ADR-221). Copiez-le (`install -m 600 .env.production.example .env.production`)
et remplissez-le, ou écrivez l'essentiel directement :

```bash
(umask 077 && cat > .env.production <<VARIABLES
POSTGRES_PASSWORD=$(openssl rand -hex 32)
MIMORYA_APP_DB_PASSWORD=$(openssl rand -hex 32)
MIMORYA_MASTER_KEY=$(openssl rand -base64 32)   # scelle les secrets, clé des limites (ADR-024, ADR-152)
MIMORYA_DOMAIN=mimorya.acme.example
MIMORYA_TLS=exploitation@acme.example
MIMORYA_VERSION=1.2.0            # images publiées à installer (ADR-051) : version ou sha-…
MIMORYA_WORKERS=1                # workers qui exécutent les runs (ADR-087)
ANTHROPIC_API_KEY=…              # modèle de la plateforme, fourni par l'exploitant (ADR-208)
LLM_MODEL=claude-sonnet-5-5      # Opus en escalade, choisi par entreprise (ADR-207)
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
dernier commit de `main`, et peut rester sur un commit antérieur jusqu'à la fusion
suivante (ADR-051). Lancez ces commandes depuis le dépôt extrait à la même version
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
`internal` (certificat local de Caddy). `MIMORYA_MASTER_KEY` est obligatoire : sans
elle, aucune entreprise ne pourrait garder sa propre clé de modèle ni relier un compte ;
gardez-en une copie hors du serveur. Chaque variable que lit l'API est décrite dans
`.env.production.example` et transmise par `compose.production.yml`, sauf celles que la
pile fixe ou que l'image tient ; un test le vérifie à chaque PR
(`crates/api/tests/configuration_files.rs`), de même que `.env.example`. La CI suit ce
parcours, rôles et second worker compris, jusqu'à la première connexion de
l'administrateur avec les images construites depuis le dépôt
(`compose.production.build.yml`), pas avec les images téléchargées
(`infra/docker/production-test.sh`, réservé à une pile jetable : il y laisse une
entreprise de vérification).

#### Vérifier l'installation

Une fois la pile démarrée, une commande dit ce qui est prêt et ce qui manque (ADR-223) :

```bash
docker compose --env-file .env.production -f compose.production.yml exec api \
  mimorya-admin check-install --mail-to vous@example.com
```

Elle passe en revue la base et ses migrations, la clé maître, le domaine et son
certificat, le relais SMTP (avec `--mail-to`, un e-mail d'essai part à cette adresse),
l'archive des WAL, les workers, la marque et le modèle de la plateforme. Chaque point est
`OK`, `À FAIRE` (la plateforme sert, mais il manque quelque chose) ou `ÉCHEC` (à corriger
avant d'ouvrir : elle ne peut pas servir, ou ne protège plus ses données), avec ce qu'il
faut faire. La commande échoue tant qu'un point est en `ÉCHEC`. `--call-model` ajoute un
vrai appel au modèle pour vérifier sa clé ; il est facturé, quelques jetons chez
Anthropic.

#### Marque de l'installation

La pile porte la marque de l'installation sans reconstruire les images (ADR-222) :

- `MIMORYA_PRODUCT_NAME` et `MIMORYA_ASSISTANT_NAME` dans `.env.production`. Le nom du
  produit paraît dans l'onglet, la connexion, la barre latérale, les e-mails et les
  applications d'authentification ; celui de l'assistant dans les conversations et les
  consignes données au modèle. L'assistant prend le nom du produit par défaut ;
- `MIMORYA_ACCENT_LIGHT` et `MIMORYA_ACCENT_DARK` (`#rrggbb`), la couleur d'accent de
  chaque thème ;
- le logo et le favicon dans le dossier `./brand` de l'hôte (`MIMORYA_BRAND_FOLDER`),
  monté en lecture seule dans le service `api`, sous les noms `logo.svg`, `logo.png` ou
  `logo.webp`, et `favicon.svg`, `favicon.png` ou `favicon.ico`.

Sans réglage, tout reste « Mimorya ». L'API vérifie la marque à son démarrage et refuse
de démarrer, en disant pourquoi dans ses journaux (`docker compose … logs api`), si un
accent n'a pas le contraste de WCAG AA (4,5:1) avec un fond de l'interface, si un fichier
n'est pas le type que dit son nom ou pèse trop (512 Kio pour le logo, 128 Kio pour le
favicon), ou si un SVG contient un script, un gestionnaire d'événement, un élément hors
de l'espace de noms de SVG, une référence externe ou un encodage autre qu'UTF-8 (ADR-222
pour la liste). Les fichiers doivent être lisibles par l'utilisateur du conteneur `api`
(uid 10001) : `chmod a+r brand/*`. Après un changement, relancez l'API (`… up -d`) :
l'interface la relit dans la minute. Pensez aussi au nom de l'expéditeur des e-mails,
`MAIL_FROM`.

### Sauvegardes

La base se sauvegarde chiffrée par [age](https://age-encryption.org), à installer sur
l'hôte de la pile (ADR-166). Une sauvegarde est chiffrée pour des clés publiques :
l'identité qui l'ouvre, la clé privée, se garde hors du serveur, qui ne peut donc pas relire
ses propres sauvegardes. Prévoyez deux destinataires, dont un conservé à part : une
identité perdue rend ses sauvegardes illisibles. Gardez une copie de la clé maître
(`MIMORYA_MASTER_KEY`) avec l'identité, hors du serveur : elle scelle les secrets de la
base et ne fait pas partie d'une sauvegarde.

```bash
age-keygen -o identite.txt          # sur un poste à part ; la clé publique s'affiche
COMPOSE_ENV_FILES=.env.production MIMORYA_BACKUP_RECIPIENTS='age1… age1…' \
  infra/backup/backup.sh            # écrit backups/mimorya-<heure UTC>.dump.age (600)
```

Un fichier de destinataires (`MIMORYA_BACKUP_RECIPIENTS_FILE`) remplace la variable. Une
sauvegarde n'en remplace jamais une autre : son dossier doit accepter les liens physiques
(ext4, XFS, Btrfs, ZFS ou NFS, mais pas FAT, exFAT ni certains montages FUSE), ce que le
script vérifie avant le dump. Une sauvegarde tuée brutalement (`kill -9`, arrêt de la
machine) peut laisser un fichier `….dump.age.partial.XXXXXX` (ou
`….base.tar.gz.age.partial.XXXXXX`), chiffré, ou le lien d'essai `….probe.XXXXXXXX` qui le
nomme : supprimez-les.
La fréquence, la conservation et la copie hors site sont à votre charge, par exemple une
ligne de `cron` qui lance la sauvegarde chaque nuit et une copie vers un autre site. Une
donnée supprimée de la plateforme (un compte, une entreprise) reste dans les sauvegardes
déjà faites, et dans l'archive des WAL (voir plus bas), jusqu'à ce que leur conservation
les retire : fixez cette durée en conséquence.

Une sauvegarde se restaure dans une pile nouvelle, sa base seule démarrée : une base qui
contient déjà des tables est refusée, et le dump est lu en une transaction, tout ou rien.
La pile nouvelle garde ses propres mots de passe de base, et la clé maître de la pile
sauvegardée.

```bash
docker compose --env-file .env.production -f compose.production.yml up -d db
COMPOSE_ENV_FILES=.env.production infra/backup/restore.sh backups/mimorya-….dump.age identite.txt
docker compose --env-file .env.production -f compose.production.yml up -d
```

La CI sauvegarde la pile de production qu'elle vient d'installer, jette sa base, la
restaure dans une base nouvelle et vérifie que l'administrateur s'y connecte encore
(`infra/docker/backup-test.sh`).

#### Archive continue des WAL

Un dump ramène la base à l'heure de la sauvegarde. Pour pouvoir revenir à un instant
précis, la base archive aussi chacun de ses WAL, chiffré pour les mêmes clés publiques
(ADR-206). Donnez-les au service `db` dans `.env.production` ; sans elles, rien n'est
archivé, jamais en clair :

```bash
MIMORYA_BACKUP_RECIPIENTS='age1… age1…'   # dans .env.production
MIMORYA_WAL_ARCHIVE=/srv/mimorya/wal      # facultatif : ./wal-archive par défaut
```

Au démarrage, la base fait vérifier ces clés par `age` et refuse de démarrer, sans rien
créer, si l'une est refusée, ou si le dossier de l'archive n'accepte pas les liens
physiques (FAT, exFAT, certains montages CIFS ou FUSE), dont chaque archivage a besoin.
Elle prend le dossier de l'archive pour elle (uid 999, mode 700) : sur l'hôte, seuls `root` et le compte qui y porte l'uid 999, s'il en est un
(`getent passwd 999`), y accèdent ; copiez-le hors site en `root`. Chaque fichier y est
compressé, chiffré, et n'en remplace jamais un autre, dans un dossier par système de
base : `wal-archive/<identifiant>/<nom>.<sha256>.gz.age`. Un segment est archivé au plus
tard cinq minutes après sa première écriture. C'est la perte maximale de données tant que
l'archive survit : une archive restée sur l'hôte se perd avec lui, ajoutez-y l'intervalle
de sa copie hors site. Un archivage qui échoue (dossier plein ou en lecture seule) ne perd
rien : PostgreSQL garde ses WAL et réessaie, au prix de son propre disque. Surveillez-le
(ADR-215) : `infra/backup/archive-status.sh` dit où en est l'archive et sort en erreur
(1) si l'archivage est désactivé, si le dernier a échoué, si un WAL attend depuis plus de
dix minutes, ou, avec `--base-within <heures>`, si aucune sauvegarde de base n'a été
archivée depuis ce délai :

```bash
COMPOSE_ENV_FILES=.env.production infra/backup/archive-status.sh --base-within 26
```

L'archive se rejoue sur une sauvegarde de base, la copie des fichiers de la base,
chiffrée de même, à refaire régulièrement (chaque nuit, par exemple) :

```bash
COMPOSE_ENV_FILES=.env.production MIMORYA_BACKUP_RECIPIENTS='age1… age1…' \
  infra/backup/backup.sh --base     # écrit backups/mimorya-<heure UTC>.base.tar.gz.age (600)
```

L'archive grossit sans limite tant qu'elle n'est pas élaguée (environ 300 fichiers par
jour, jusqu'à quelques Mo chacun quand la base est active), et garde, chiffrées, les
données supprimées depuis de la plateforme. Choisissez combien de jours la base doit
pouvoir remonter, et élaguez chaque jour, après la sauvegarde de base (ADR-215) :

```bash
COMPOSE_ENV_FILES=.env.production infra/backup/prune.sh 30 --dry-run   # dit ce qui partirait
COMPOSE_ENV_FILES=.env.production infra/backup/prune.sh 30
```

La base doit tourner. L'archive est élaguée d'abord : elle ne garde que ce dont ont besoin
les sauvegardes de base de la période, celles archivées depuis 30 jours et la dernière
d'avant, et, après une restauration à un instant passé, celles d'avant la bifurcation que
la ligne nouvelle prolonge. Les historiques des lignes temporelles restent toujours.
L'archive d'un autre système (une base refaite par un dump) part entière quand son dernier
fichier a plus de 30 jours ; jamais celle de la base en service. Dans le dossier des
sauvegardes (`MIMORYA_BACKUPS_DIR`, `backups/` par défaut), parmi les fichiers nommés par
`backup.sh`, une sauvegarde de base reste si elle a moins de 30 jours, ou si elle est la
dernière commencée avant la fin d'une sauvegarde que l'archive garde ; un dump reste s'il
a moins de 30 jours, plus le dernier d'avant ; un fichier autrement nommé ne part jamais.
L'âge d'un fichier de l'archive est l'heure de son archivage, qu'une copie garde avec `cp
-p` ou `rsync -a`. Une copie hors site synchronisée sans suppression (`rsync -a` sans
`--delete`) garde plus longtemps ce que l'élagage retire ici, à votre rythme : une
sauvegarde de base gardée plus longtemps ne se restaure qu'avec une telle copie de
l'archive. Une archive sans aucune sauvegarde de base (des dumps seulement) n'est pas
élaguée : `archive-status.sh --base-within` le signale. Ne menez qu'une sauvegarde de base
à la fois : la conservation suppose qu'elles se suivent. Ne démarrez jamais une base
restaurée depuis une sauvegarde de base autrement que par `restore-base.sh` (voir plus
bas), sur le dossier d'archive de la base copiée : elle archiverait sous les mêmes noms
qu'elle et bloquerait son archivage.

La base de la pile de production est l'image `mimorya-db` (PostgreSQL 16, pgvector et
`age`) : une installation antérieure qui se met à jour voit son conteneur de base remplacé,
sur le même volume. La CI vérifie l'archive et la
sauvegarde de base de la pile qu'elle installe (`infra/docker/wal-archive-test.sh`), sa
conservation et son état (`infra/docker/archive-retention-test.sh`). Le
[plan de reprise](docs/RECOVERY_PLAN.md) dit quoi préparer et quoi faire selon l'incident
(ADR-216).

#### Restauration à un instant précis

Une sauvegarde de base et l'archive des WAL ramènent la base à un instant choisi, juste
avant une erreur (ADR-214), dans une pile nouvelle. Celle-ci a la même version, la même clé
maître et les mêmes mots de passe de base que la pile sauvegardée ; sa base n'a jamais
démarré (ou a été vidée par `down -v`), et le dossier de son archive tient une copie de
celle de la pile sauvegardée. L'identité qui ouvre la sauvegarde et l'archive n'est remise
à la base que le temps de la récupération, dans sa mémoire :

```bash
# .env.production de la pile nouvelle : MIMORYA_WAL_ARCHIVE vers la copie de l'archive
COMPOSE_ENV_FILES=.env.production infra/backup/restore-base.sh \
  backups/mimorya-….base.tar.gz.age identite.txt '2026-11-20 14:30:00+01'
docker compose --env-file .env.production -f compose.production.yml up -d
```

L'instant est une date, une heure et son fuseau (`UTC`, ou un décalage comme `+01` ou
`-05:30`) : la base revient à son dernier commit qui le précède ; sans instant, toute l'archive est
rejouée. Prenez la plus récente sauvegarde de base terminée avant l'instant : un instant
qui ne vient pas au moins une seconde après son début est refusé, avant toute
récupération. Le script refuse aussi une base démarrée ou qui
contient déjà des fichiers, et une archive sans le dossier de la base sauvegardée. Ce
dossier doit être à la base (uid 999) : une copie faite en `root` arrête la restauration,
comme un WAL illisible, dont le contenu diffère de son nom, ou qui manque alors qu'un
suivant est archivé, et une ligne temporelle nommée dont l'archive ne tient que
l'historique ; la raison en est dite. Après un échec, videz la base (`down -v`)
avant de réessayer. `MIMORYA_RESTORE_TIMEOUT` borne la récupération, une heure par défaut,
au-delà de quoi elle est arrêtée.

La restauration rejoue la ligne temporelle de la sauvegarde de base. Si la base sauvegardée
en a elle-même changé après la sauvegarde, parce qu'elle vient d'une restauration
antérieure, nommez sa ligne temporelle par son numéro (`MIMORYA_RESTORE_TIMELINE=3`, que
cette restauration avait dit) : la récupération suit son histoire.

La base restaurée prend une nouvelle ligne temporelle et archive à côté des fichiers de la
base sauvegardée, sans jamais les remplacer : plusieurs restaurations de la même archive ne
se gênent pas. La CI restaure ainsi trois fois la pile qu'elle installe, à deux instants
puis le long d'une ligne temporelle nommée par son numéro
(`infra/docker/point-in-time-test.sh`).

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
