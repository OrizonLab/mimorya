# Installer la bêta sur un serveur

Ce guide installe la plateforme sur un serveur neuf, Debian 12 ou Ubuntu 24.04, pour une
équipe puis deux ou trois entreprises pilotes. Il suit la pile de production
(`compose.production.yml`, ADR-034) avec les images publiées, à une version donnée :
`v0.1.0-beta.1`. Comptez une heure, DNS compris.

Les commandes se lancent en `root` (ou avec `sudo`) sur le serveur, sauf celles que le
guide dit de lancer sur votre poste.

## 1. Le serveur

Pour une bêta de quelques dizaines d'utilisateurs :

| Ressource  | Conseillé                                                                                  | Minimum (votre équipe seule) |
| ---------- | ------------------------------------------------------------------------------------------ | ---------------------------- |
| Processeur | 4 vCPU                                                                                     | 2 vCPU                       |
| Mémoire    | 8 Go                                                                                       | 4 Go                         |
| Disque     | 80 Go SSD                                                                                  | 40 Go SSD                    |
| Système    | Debian 12 ou Ubuntu 24.04 LTS, à jour, en amd64 (les images ne sont publiées que pour lui) |                              |

Le disque porte la base, les documents déposés (1 Go par entreprise par défaut,
`MIMORYA_DOCUMENT_QUOTA_MB`), l'archive des WAL et les sauvegardes locales avant leur
copie ailleurs. Choisissez un hébergeur dans l'Union européenne : la plateforme traite les
données personnelles de vos entreprises (registre des traitements, ADR-217).

Préparez un accès SSH par clé, un utilisateur administrateur, et mettez le système à
jour :

```bash
apt update && apt upgrade -y
apt install -y ca-certificates curl git openssl jq age ufw
```

## 2. Le nom de domaine

Créez chez votre registraire un enregistrement `A` (et `AAAA` si le serveur a une adresse
IPv6) qui fait pointer le domaine de la plateforme, par exemple `beta.example.com`, vers
l'adresse du serveur. Vérifiez depuis votre poste, une fois propagé :

```bash
dig +short beta.example.com     # l'adresse du serveur
```

Le certificat est obtenu automatiquement par Caddy, le reverse proxy de la pile, auprès de
Let's Encrypt : il faut que le domaine pointe vers le serveur et que les ports 80 et 443
soient joignables depuis Internet avant le premier démarrage.

## 3. Le pare-feu

Seuls SSH, HTTP et HTTPS s'ouvrent. Dans la pile, seul Caddy publie des ports : la base,
l'API et l'interface ne sont joignables que de l'intérieur.

```bash
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw enable
ufw status
```

## 4. Docker

Installez Docker Engine et son greffon Compose depuis le dépôt de Docker (sur Ubuntu,
remplacez `debian` par `ubuntu` dans les deux adresses) :

```bash
install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/debian/gpg -o /etc/apt/keyrings/docker.asc
chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
https://download.docker.com/linux/debian $(. /etc/os-release && echo "$VERSION_CODENAME") stable" \
  > /etc/apt/sources.list.d/docker.list
apt update
apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
docker compose version
```

`age` (installé au §1) chiffre les sauvegardes (ADR-166, ADR-206).

## 5. Le dépôt, à la version installée

La pile monte des fichiers du dépôt (Caddy, initialisation de la base, archivage des WAL) :
le dépôt s'extrait à la même version que les images.

```bash
git clone --branch v0.1.0-beta.1 https://github.com/OrizonLab/mimorya.git /opt/mimorya
cd /opt/mimorya
```

Toutes les commandes suivantes se lancent depuis `/opt/mimorya`. Pour les raccourcir :

```bash
alias dc='docker compose --env-file .env.production -f compose.production.yml'
```

## 6. Secrets et `.env.production`

### Sur votre poste : l'identité des sauvegardes

Les sauvegardes sont chiffrées pour une clé publique ; la clé privée qui les ouvre ne va
jamais sur le serveur. Sur votre poste :

```bash
age-keygen -o sauvegardes-mimorya.key
```

La ligne `# public key: age1…` donne la clé publique. Rangez le fichier dans votre
gestionnaire de secrets, et une seconde identité ailleurs si vous le pouvez : une identité
perdue rend ses sauvegardes illisibles.

### Sur le serveur : le fichier de la pile

```bash
install -m 600 .env.production.example .env.production
openssl rand -hex 32        # POSTGRES_PASSWORD
openssl rand -hex 32        # MIMORYA_APP_DB_PASSWORD (une autre)
openssl rand -base64 32     # MIMORYA_MASTER_KEY
```

Remplissez `.env.production` ; chaque variable y est décrite, avec son défaut et ce qui
se passe sans elle (ADR-221) :

| Variable                    | Valeur                                                       |
| --------------------------- | ------------------------------------------------------------ |
| `MIMORYA_DOMAIN`            | `beta.example.com`                                           |
| `MIMORYA_TLS`               | une adresse e-mail (certificat public de Let's Encrypt)      |
| `POSTGRES_PASSWORD`         | la première valeur `openssl rand -hex 32`                    |
| `MIMORYA_APP_DB_PASSWORD`   | la seconde                                                   |
| `MIMORYA_MASTER_KEY`        | la valeur `openssl rand -base64 32`                          |
| `MIMORYA_VERSION`           | `0.1.0-beta.1`                                               |
| `ANTHROPIC_API_KEY`         | la clé d'API de votre compte Anthropic (console d'Anthropic) |
| `LLM_MODEL`                 | `claude-sonnet-5-5`                                          |
| `SMTP_URL`, `MAIL_FROM`     | votre relais d'e-mails et son expéditeur, ensemble           |
| `MIMORYA_BACKUP_RECIPIENTS` | la clé publique `age1…` de votre identité (et de la seconde) |

Copiez aussi `MIMORYA_MASTER_KEY` dans votre gestionnaire de secrets : perdue, les clés
de modèle et les jetons des entreprises sont illisibles, et le journal d'audit ne se
vérifie plus. Pour la marque de l'installation (nom, couleurs, logo), voir le README,
« Marque de l'installation » (ADR-222).

## 7. Démarrage

```bash
dc pull
dc up -d
dc ps
```

Au premier démarrage, la base se crée, le service `migrate` applique les migrations puis
s'arrête, l'API, le worker et l'interface démarrent, Caddy obtient le certificat. Les
journaux, si un service ne démarre pas : `dc logs --tail 100 api` (ou `db`, `proxy`…).

## 8. Vérification

```bash
dc exec api mimorya-admin check-install --mail-to vous@example.com
```

La commande dit chaque point `OK`, `À FAIRE` ou `ÉCHEC`, avec ce qu'il faut faire
(ADR-223) :

| Point                   | Attendu                                                    |
| ----------------------- | ---------------------------------------------------------- |
| Base de données         | `OK`, joignable par le rôle applicatif                     |
| Migrations              | `OK`, à jour                                               |
| Clé maître              | `OK`, présente                                             |
| Domaine et certificat   | `OK`, le domaine répond, son certificat est reconnu        |
| Relais SMTP             | `OK`, l'e-mail d'essai est accepté ; vérifiez qu'il arrive |
| Sauvegardes             | `OK`, WAL archivés pour vos destinataires                  |
| Workers                 | `OK`, 1 worker à l'écoute (ou `MIMORYA_WORKERS`)           |
| Marque                  | `OK`, « Mimorya » ou la vôtre                              |
| Modèle de la plateforme | `À FAIRE` tant qu'il n'est pas appelé                      |

Une fois, vérifiez la clé d'Anthropic par un vrai appel (quelques jetons, facturés) :

```bash
dc exec api mimorya-admin check-install --call-model
```

Derrière un NAT sans retour, le serveur peut ne pas se joindre lui-même par son domaine :
si le domaine est `ÉCHEC` mais que `https://beta.example.com` s'ouvre depuis votre poste,
la plateforme sert quand même. Tant qu'un autre point est en `ÉCHEC`, ne l'ouvrez pas.

## 9. La première entreprise et sa formule

Créez votre entreprise et son premier administrateur, vous :

```bash
dc exec api mimorya-admin create-company --name 'Mon équipe' --slug equipe \
  --admin-email vous@example.com --admin-name 'Prénom Nom'
```

La commande affiche un lien d'invitation, valable sept jours : ouvrez-le, choisissez
votre mot de passe, puis activez le second facteur dans « Mon compte ».

Une entreprise neuve est en formule « clé du client » : elle n'a pas de modèle tant
qu'elle n'a pas saisi sa propre clé dans « Administration > Modèle d'IA ». Pour qu'elle
utilise la clé de l'exploitant, la vôtre (ADR-208), dans un plafond mensuel :

```bash
dc run --rm migrate mimorya-admin set-model-supply --slug equipe --formula capped --cap 100
dc run --rm migrate mimorya-admin model-supply --slug equipe
```

Ou contre un crédit prépayé, pour une entreprise pilote :

```bash
dc run --rm migrate mimorya-admin set-model-supply --slug pilote --formula prepaid
dc run --rm migrate mimorya-admin add-credit --slug pilote --amount 200 --reference FAC-2026-001
```

Ces commandes passent par le service `migrate`, qui tient le rôle propriétaire : le rôle
applicatif ne fait que lire la formule. Les administrateurs de l'entreprise sont prévenus
par e-mail à 80 et 95 % du plafond, ou à 20 et 5 % du dernier versement
(`set-supply-alerts` pour d'autres parts, ADR-211).

Dans l'application, « Administration > Modèle d'IA > Escalade de modèle » choisit le
modèle des demandes difficiles, `claude-opus-5-5`, son budget mensuel et ses mécanismes
(ADR-207).

## 10. Les sauvegardes programmées

L'archive des WAL tourne déjà ; il reste à programmer les sauvegardes de base, leur
élagage, la surveillance de l'archive et leur copie hors du serveur. Suivez
`docs/RECOVERY_PLAN.md`, § 3 « Préparation » : les tâches planifiées de `root`, l'hôte de
secours, puis un premier exercice de restauration (la recette de la bêta en fait un,
`docs/RECETTE_BETA.md`).

## 11. Mettre à jour vers une nouvelle version

1. Lisez ce que la version change (`docs/DECISIONS.md`, `docs/ROADMAP.md`).
2. Sauvegardez : `COMPOSE_ENV_FILES=.env.production infra/backup/backup.sh --base`.
3. Extrayez la version et changez `MIMORYA_VERSION` dans `.env.production` :

   ```bash
   git fetch --tags
   git checkout v0.1.0-beta.2
   ```

4. Redémarrez, puis vérifiez :

   ```bash
   dc pull
   dc up -d
   dc exec api mimorya-admin check-install
   ```

Les migrations s'appliquent au démarrage, par le service `migrate`. Un run interrompu par
la mise à jour est repris par un worker. Les migrations ne reviennent jamais en arrière :
revenir à la version précédente, c'est restaurer la sauvegarde faite au point 2
(`docs/RECOVERY_PLAN.md`).

## 12. En cas de problème

`docs/RECETTE_BETA.md`, « Remonter un problème », dit quoi joindre : la version, la
sortie de `check-install` et les journaux, qui ne contiennent aucun secret.
