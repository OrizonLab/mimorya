# Installer la bêta sur un serveur

Ce guide installe la plateforme sur un serveur neuf, Debian ou Ubuntu, pour une équipe puis
deux ou trois entreprises pilotes. Il suit la pile de production (`compose.production.yml`,
ADR-034) avec les images publiées, à une version donnée : `v0.1.0-beta.1`. Comptez une
heure, DNS compris.

Les commandes se lancent sur le serveur **en `root`** (`sudo -i` depuis votre compte
d'administration), sauf celles que le guide dit de lancer sur votre poste. Le dépôt va dans
`/srv/mimorya`, l'emplacement que suit le plan de reprise (`docs/RECOVERY_PLAN.md`).

Un reverse proxy tient déjà les ports 80 et 443 du serveur, dans un conteneur (Nginx
Proxy Manager, nginx, Traefik…) ? Lisez d'abord l'annexe, « Derrière un reverse proxy déjà
installé » : elle change les étapes 5, 6 et 7, et c'est ce reverse proxy qui obtient le
certificat.

## 1. Le serveur

Pour une bêta de quelques dizaines d'utilisateurs :

| Ressource  | Conseillé | Minimum (votre équipe seule) |
| ---------- | --------- | ---------------------------- |
| Processeur | 4 vCPU    | 2 vCPU                       |
| Mémoire    | 8 Go      | 4 Go                         |
| Disque     | 80 Go SSD | 40 Go SSD                    |

Système : Debian 13 (ou Debian 12 tant qu'il est suivi) ou Ubuntu 24.04 LTS, à jour, en
amd64 : les images ne sont publiées que pour lui.

Le disque porte la base, les documents déposés (1 Go par entreprise par défaut,
`MIMORYA_DOCUMENT_QUOTA_MB`), l'archive des WAL et les sauvegardes locales avant leur copie
ailleurs. Choisissez un hébergeur dans l'Union européenne : la plateforme traite les
données personnelles de vos entreprises (registre des traitements, ADR-217).

Préparez un accès SSH par clé et un compte d'administration (`sudo`), puis mettez le
système à jour :

```bash
apt update && apt upgrade -y
apt install -y ca-certificates curl git openssl jq age ufw rsync
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

`age` (installé au § 1) chiffre les sauvegardes (ADR-166, ADR-206).

## 5. Le dépôt, à la version installée

La pile monte des fichiers du dépôt (Caddy, initialisation de la base, archivage des WAL) :
le dépôt s'extrait à la même version que les images.

```bash
git clone --branch v0.1.0-beta.1 https://github.com/OrizonLab/mimorya.git /srv/mimorya
cd /srv/mimorya
```

La pile se désigne une fois pour toutes, comme dans le plan de reprise : ajoutez ces lignes à
`/root/.bashrc`, puis ouvrez un nouveau shell `root` (`sudo -i`). Sans elles,
`docker compose` lirait `docker-compose.yml`, la base de développement.

```bash
cd /srv/mimorya
export COMPOSE_FILE=compose.production.yml COMPOSE_ENV_FILES=/srv/mimorya/.env.production
export MIMORYA_BACKUP_RECIPIENTS_FILE=/srv/mimorya/destinataires.txt
```

Toutes les commandes suivantes se lancent depuis `/srv/mimorya`, dans ce shell.

## 6. Secrets et `.env.production`

### Sur votre poste : les identités des sauvegardes

Les sauvegardes sont chiffrées pour des clés publiques ; les clés privées qui les ouvrent,
les identités, ne vont jamais sur le serveur. Installez `age` sur votre poste
(`apt install age`, `brew install age`, ou depuis <https://age-encryption.org>), puis
créez **deux identités**, gardées à deux endroits différents : une identité perdue rend ses
sauvegardes illisibles.

```bash
age-keygen -o sauvegardes-mimorya-1.key
age-keygen -o sauvegardes-mimorya-2.key
```

La ligne `# public key: age1…` de chacune donne sa clé publique. Rangez les deux fichiers
dans votre gestionnaire de secrets.

### Sur le serveur : le fichier de la pile

Les deux clés publiques, une par ligne, dans le fichier que lisent les sauvegardes :

```bash
printf '%s\n' 'age1…' 'age1…' > destinataires.txt
```

Puis le fichier de la pile, lisible par `root` seul :

```bash
install -m 600 .env.production.example .env.production
openssl rand -hex 32        # POSTGRES_PASSWORD
openssl rand -hex 32        # MIMORYA_APP_DB_PASSWORD (une autre)
openssl rand -base64 32     # MIMORYA_MASTER_KEY
```

Remplissez `.env.production` ; chaque variable y est décrite, avec son défaut et ce qui se
passe sans elle (ADR-221) :

| Variable                    | Valeur                                                       |
| --------------------------- | ------------------------------------------------------------ |
| `MIMORYA_DOMAIN`            | `beta.example.com`                                           |
| `MIMORYA_TLS`               | une adresse e-mail (certificat de Let's Encrypt) ; annexe    |
| `POSTGRES_PASSWORD`         | la première valeur `openssl rand -hex 32`                    |
| `MIMORYA_APP_DB_PASSWORD`   | la seconde                                                   |
| `MIMORYA_MASTER_KEY`        | la valeur `openssl rand -base64 32`                          |
| `MIMORYA_VERSION`           | `0.1.0-beta.1`                                               |
| `ANTHROPIC_API_KEY`         | la clé d'API de votre compte Anthropic (console d'Anthropic) |
| `LLM_MODEL`                 | `claude-sonnet-5-5`                                          |
| `SMTP_URL`, `MAIL_FROM`     | votre relais d'e-mails et son expéditeur, ensemble           |
| `MIMORYA_BACKUP_RECIPIENTS` | les deux clés publiques `age1…`, séparées par une espace     |

Copiez la clé maître et `.env.production` entier dans votre gestionnaire de secrets :
perdue, la clé maître rend illisibles les clés de modèle et les jetons des entreprises, et
le journal d'audit ne se vérifie plus. Pour la marque de l'installation (nom, couleurs,
logo), voir le README, « Marque de l'installation » (ADR-222).

## 7. Démarrage

```bash
docker compose pull
docker compose up -d
docker compose ps
```

Au premier démarrage, la base se crée, le service `migrate` applique les migrations puis
s'arrête, l'API, le worker et l'interface démarrent, Caddy obtient le certificat. Les
journaux, si un service ne démarre pas : `docker compose logs --tail 100 api` (ou `db`,
`proxy`…).

## 8. Vérification

```bash
docker compose exec api mimorya-admin check-install --mail-to vous@example.com
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

Une fois, vérifiez la clé d'Anthropic par un vrai appel (facturé, quelques jetons) :

```bash
docker compose exec api mimorya-admin check-install --call-model
```

Derrière un NAT sans retour, le serveur peut ne pas se joindre lui-même par son domaine :
si le domaine est `ÉCHEC` mais que `https://beta.example.com` s'ouvre depuis votre poste,
la plateforme sert quand même. Tant qu'un autre point est en `ÉCHEC`, ne l'ouvrez pas.

## 9. La première entreprise et sa formule

Créez votre entreprise et son premier administrateur, vous :

```bash
docker compose exec api mimorya-admin create-company --name 'Mon équipe' --slug equipe \
  --admin-email vous@example.com --admin-name 'Prénom Nom'
```

La commande affiche un lien d'invitation, valable sept jours : ouvrez-le, choisissez
votre mot de passe, puis activez le second facteur dans « Mon compte ». Une adresse
e-mail ne sert qu'à un compte sur toute la plateforme : chaque entreprise pilote aura
l'adresse de son propre administrateur.

Une entreprise neuve est en formule « clé du client » : elle n'a pas de modèle tant qu'elle
n'a pas saisi sa propre clé dans « Administration > Modèle d'IA ». Pour qu'elle utilise la
clé de l'exploitant, la vôtre (ADR-208), dans un plafond mensuel :

```bash
docker compose run --rm migrate mimorya-admin set-model-supply --slug equipe --formula capped --cap 100
docker compose run --rm migrate mimorya-admin model-supply --slug equipe
```

Une entreprise pilote, créée de même avec l'adresse de son administrateur, peut aussi
recevoir un crédit prépayé :

```bash
docker compose run --rm migrate mimorya-admin set-model-supply --slug pilote --formula prepaid
docker compose run --rm migrate mimorya-admin add-credit --slug pilote --amount 200 --reference FAC-2026-001
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
secours, puis un premier exercice de restauration (§ 6, que la recette de la bêta reprend,
`docs/RECETTE_BETA.md`). Les chemins y sont ceux de ce guide, `/srv/mimorya`.

## 11. Mettre à jour vers une nouvelle version

1. Lisez ce que la version change (`docs/DECISIONS.md`, `docs/ROADMAP.md`).
2. Sauvegardez, puis notez l'instant, en UTC :

   ```bash
   infra/backup/backup.sh --base
   date -u '+%Y-%m-%d %H:%M:%S UTC'     # l'instant d'avant la mise à jour
   ```

3. Extrayez la version et changez `MIMORYA_VERSION` dans `.env.production` :

   ```bash
   git fetch --tags
   git checkout v0.1.0-beta.2
   ```

4. Redémarrez, puis vérifiez :

   ```bash
   docker compose pull
   docker compose up -d
   docker compose exec api mimorya-admin check-install
   ```

Les migrations s'appliquent au démarrage, par le service `migrate`. Un run interrompu par
la mise à jour est repris par un worker.

**Revenir en arrière.** Les migrations ne reculent pas, et une ancienne version refuse une
base qui porte des migrations qu'elle ne connaît pas : revenir à la version précédente,
c'est restaurer la base telle qu'elle était à l'instant noté au point 2, sur l'hôte même,
votre identité apportée le temps de la restauration :

1. La base tournant encore, faites-lui archiver le segment des WAL en cours, et attendez
   qu'il le soit : sans lui, l'archive peut ne pas atteindre l'instant noté (un segment
   n'est archivé qu'une fois plein, ou après cinq minutes), et la restauration échoue.

   ```bash
   segment=$(docker compose exec -T db psql -U mimorya -d mimorya -tAc 'select pg_walfile_name(pg_switch_wal())')
   archived=non
   if [ -n "$segment" ]; then
     for attempt in $(seq 60); do
       archived=$(docker compose exec -T db psql -U mimorya -d mimorya -tAc \
           "select case when last_archived_wal >= '$segment' then 'oui' else 'non' end from pg_stat_archiver")
       [ "$archived" = oui ] && break
       sleep 5
     done
   fi
   echo "Segment ${segment:-introuvable} archivé : ${archived:-non}"
   ```

   « Segment introuvable » : la base n'a pas répondu (`docker compose ps db`). Encore
   « non » au bout de cinq minutes : l'archivage échoue ou prend du retard, et la
   restauration n'atteindrait pas l'instant noté. Dans les deux cas, n'allez pas plus loin.
   `infra/backup/archive-status.sh` dit ce qui bloque, et `docs/RECOVERY_PLAN.md`, § 4.6
   « Archivage en échec », comment y remédier ; reprenez ensuite à ce point.

2. Revenez au dépôt et à `MIMORYA_VERSION` de la version précédente
   (`git checkout v0.1.0-beta.1`).
3. Mettez de côté le volume de la base, puis retirez-le : `docs/RECOVERY_PLAN.md`, § 4.3,
   étapes 1 et 2.
4. Restaurez la sauvegarde du point 2 jusqu'à l'instant noté, puis démarrez et vérifiez :

   ```bash
   infra/backup/restore-base.sh backups/mimorya-….base.tar.gz.age identite.key '2026-12-01 09:30:00 UTC'
   docker compose up -d
   docker compose exec api mimorya-admin check-install
   ```

5. Retirez l'identité du serveur, puis suivez `docs/RECOVERY_PLAN.md`, § 5 « Après une
   reprise » : une sauvegarde de base aussitôt, la ligne temporelle notée, l'archive
   vérifiée.

Ce qui a été fait entre l'instant noté et le retour en arrière est perdu : prévenez les
entreprises avant de mettre à jour.

## 12. En cas de problème

`docs/RECETTE_BETA.md`, « Remonter un problème », dit quoi joindre : la version, la sortie
de `check-install` et les journaux, qui ne contiennent aucun secret.

## Annexe : derrière un reverse proxy déjà installé

Quand un reverse proxy tient déjà les ports 80 et 443 du serveur (Nginx Proxy Manager,
nginx, Traefik…), c'est lui qui sert la plateforme et obtient son certificat ; Caddy ne
démarre pas (ADR-225). Le fichier `compose.behind-proxy.yml` s'ajoute à la pile :
l'interface et l'API rejoignent un réseau à part, `mimorya-edge`, sous les noms
`mimorya-web` (port 3000) et `mimorya-api` (port 4000). La base et les workers n'y sont
pas, et la pile ne publie aucun port : le reverse proxy tourne donc **dans un conteneur**
relié à ce réseau. Un nginx installé sur l'hôte lui-même ne joindrait pas la pile.

Ce qui change, dans l'ordre du guide :

1. **§ 5** : `/root/.bashrc` nomme aussi ce fichier.

   ```bash
   export COMPOSE_FILE=compose.production.yml:compose.behind-proxy.yml COMPOSE_ENV_FILES=/srv/mimorya/.env.production
   ```

2. **§ 6** : `MIMORYA_TLS=internal`. La pile l'exige ; Caddy, qui ne démarre pas, ne s'en
   sert pas.
3. **Avant le § 7**, une fois, le réseau, puis une vérification qui ne démarre rien :

   ```bash
   docker network create mimorya-edge
   docker compose config --quiet       # refuse une variable obligatoire manquante
   docker compose config --services    # db, migrate, api, web, worker : pas de proxy
   ```

4. **§ 7** : le démarrage ne change pas, sinon que Caddy ne démarre pas : le certificat
   viendra du reverse proxy.
5. **Après le § 7**, le reverse proxy rejoint `mimorya-edge` et sert le domaine (plus tôt,
   il répondrait 502). Au § 8, « Domaine et certificat » parle alors du certificat du
   reverse proxy, pas de `MIMORYA_TLS`.
6. **Le plan de reprise** (`docs/RECOVERY_PLAN.md`) nomme aussi ce fichier, ce réseau et
   ce reverse proxy, sur l'hôte de secours comme ici.

### Nginx Proxy Manager

1. **Le réseau.** Dans le `docker-compose.yml` de Nginx Proxy Manager :

   ```yaml
   services:
     app: # le service de Nginx Proxy Manager, tel qu'il s'appelle chez vous
       networks: [default, mimorya-edge]
   networks:
     mimorya-edge:
       external: true
   ```

   Puis, dans son dossier, sans les variables qui désignent la pile de la plateforme :

   ```bash
   env -u COMPOSE_FILE -u COMPOSE_ENV_FILES docker compose up -d
   ```

   Le réseau existe avant les deux piles : aucune ne dépend du démarrage de l'autre.

2. **Le Proxy Host** (_Hosts_, _Proxy Hosts_, _Add Proxy Host_) :
   - _Details_ : _Domain Names_, le domaine de `MIMORYA_DOMAIN` ; _Scheme_ `http` ;
     _Forward Hostname / IP_ `mimorya-web` ; _Forward Port_ `3000`. Laissez _Block Common
     Exploits_ désactivé : ses règles refusent des requêtes légitimes (retours de
     connexion, SCIM, webhooks), et l'API fait ses propres contrôles. _Websockets Support_
     est inutile.
   - _Advanced_ : le bloc de `infra/proxy/api-location.conf`, tel quel. Il retire le
     préfixe `/api`, que l'API n'attend pas, et résout `mimorya-api` à chaque requête :
     Nginx Proxy Manager démarre et recharge sa configuration même quand la pile est
     arrêtée, sans faire tomber vos autres sites, et suit l'API recréée par une mise à
     jour. Il donne à l'API l'adresse d'où vient la connexion
     (`$realip_remote_addr`) : Nginx Proxy Manager prend sinon pour celle du client
     l'en-tête `X-Real-IP`, que le client écrit lui-même, dès qu'il se connecte depuis
     une adresse privée (un poste du réseau local, un client IPv6 relayé par Docker), et
     il échapperait à la limite des connexions.

     ```nginx
     location /api/ {
         resolver 127.0.0.11 valid=10s ipv6=off;
         resolver_timeout 5s;
         set $mimorya_api mimorya-api:4000;
         rewrite ^/api(/.*)$ $1 break;
         proxy_pass http://$mimorya_api;
         proxy_http_version 1.1;
         proxy_set_header Host $host;
         proxy_set_header X-Forwarded-For $realip_remote_addr;
         proxy_set_header X-Forwarded-Proto $scheme;
         proxy_buffering off;
         proxy_read_timeout 300s;
         client_max_body_size 20m;
     }
     ```

   - _SSL_ : _Request a new SSL Certificate_ (Let's Encrypt), _Force SSL_, _HTTP/2
     Support_, _HSTS Enabled_. _HSTS Subdomains_ seulement si tous les sous-domaines du
     domaine sont servis en HTTPS. Le domaine pointe vers le serveur, et le port 80 est
     joignable (§ 2 et § 3).

3. **La vérification** : `curl -sI https://beta.example.com/api/health` répond `200`, puis
   le § 8.

### Un autre reverse proxy

Il convient s'il tourne dans un conteneur relié à `mimorya-edge` et :

- sert le domaine en HTTPS, avec HSTS, et `/` à `mimorya-web:3000` ;
- sert `/api/` à `mimorya-api:4000` sans son préfixe, sans mettre les réponses en tampon
  (suivi en direct des conversations), avec un corps de 20 Mo ;
- ajoute à `X-Forwarded-For` l'adresse d'où vient la connexion, jamais une adresse
  qu'un en-tête du client lui donne ;
- résout les deux noms à chaque requête, et non à son démarrage.

Pour nginx, `infra/proxy/api-location.conf` s'inclut dans son bloc `server`, dont
`location /` résout `mimorya-web` de la même façon. La CI l'essaie ainsi devant la pile
(`infra/docker/edge-proxy.conf`), et essaie Nginx Proxy Manager 2.16.0 lui-même, ce bloc
dans son Proxy Host (`infra/docker/proxy-manager.sh`). Traefik n'est pas essayé par la
CI.

L'API prend pour adresse du client la dernière de `X-Forwarded-For` : elle limite ainsi les
tentatives de connexion de chacun. Un reverse proxy qui ne l'ajoute pas, ou un second
mandataire devant lui (Cloudflare en mode proxy), ferait compter tous les visiteurs comme
un seul. De même, quand Docker relaie l'IPv6 vers un conteneur qui n'en a pas, tous les
clients IPv6 arrivent depuis la passerelle de son réseau (une adresse en `172.` dans les
journaux de Nginx Proxy Manager) et comptent comme un seul.
