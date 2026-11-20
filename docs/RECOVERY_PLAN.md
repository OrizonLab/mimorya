# Plan de reprise

Ce plan dit comment une installation de production (`compose.production.yml`) protège ses
données, combien elle peut en perdre et en combien de temps elle repart, et quoi faire,
dans l'ordre, selon ce qui est arrivé (ADR-216). Il s'appuie sur les sauvegardes chiffrées
(ADR-166), l'archive continue des WAL (ADR-206), la restauration à un instant précis
(ADR-214) et la conservation de l'archive (ADR-215).

Les commandes se lancent en `root`, depuis le dépôt extrait à la version de l'installation
(ici `/srv/mimorya`, l'archive dans `/srv/mimorya/wal-archive` et les sauvegardes dans
`/srv/mimorya/backups`, les emplacements par défaut), la pile désignée une fois pour
toutes :

```bash
cd /srv/mimorya
export COMPOSE_FILE=compose.production.yml COMPOSE_ENV_FILES=/srv/mimorya/.env.production
export MIMORYA_BACKUP_RECIPIENTS_FILE=/srv/mimorya/destinataires.txt
```

Sans les deux premières, `docker compose` lirait `docker-compose.yml`, la base de
développement ; sans la troisième, `backup.sh` n'aurait aucun destinataire. Si vous avez
déplacé l'archive (`MIMORYA_WAL_ARCHIVE`), remplacez ses chemins partout ci-dessous.

## 1. Ce qui est protégé, et où

Tout ce que la plateforme garde (entreprises, comptes, conversations, mémoires,
documents déposés, journal d'audit, secrets scellés) est dans la base PostgreSQL. Le
reste, sur l'hôte, se refait ou se garde à part :

| Élément                                              | Protection                                                                                                                                                        | Où le garder                                                |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Base                                                 | dump et sauvegarde de base chaque nuit, archive continue des WAL, chiffrés par `age` pour des clés publiques                                                      | copie hors site, tirée par l'hôte de secours (§ 3)          |
| Clé maître (`MIMORYA_MASTER_KEY`)                    | aucune dans les sauvegardes : sans elle, les secrets de la base (jetons, mots de passe) sont illisibles                                                           | hors du serveur, avec les identités `age`                   |
| Identités `age` (clés privées)                       | deux destinataires au moins, l'un gardé à part : une identité perdue rend ses sauvegardes illisibles                                                              | hors du serveur ; apportées le temps d'une restauration     |
| `.env.production` (mots de passe, domaine, réglages) | les mots de passe de la base font partie d'une sauvegarde de base : la pile restaurée garde les mêmes                                                             | hors du serveur, avec la clé maître                         |
| Version de l'installation (`MIMORYA_VERSION`)        | images publiées par version (ADR-051) ; le dépôt extrait à la même version                                                                                        | noter la version en service (§ 7)                           |
| Certificats TLS (Caddy)                              | avec un certificat public (ACME), refaits au démarrage ; avec `MIMORYA_TLS=internal`, la racine locale que les postes ont adoptée est dans le volume `caddy-data` | avec `internal`, une copie de sa racine, avec la clé maître |

## 2. Objectifs

### Perte de données (RPO)

- **Erreur dans les données** (suppression par mégarde, entreprise supprimée à tort) : la
  base revient à l'instant choisi, juste avant l'erreur ; ce qui a été fait ensuite se
  reprend à la main.
- **Perte de l'hôte** : `archive_timeout` (cinq minutes, ADR-206) plus l'intervalle de la
  copie hors site de l'archive, soit **une dizaine de minutes** avec une copie toutes les
  cinq, tant que l'archivage et la copie sont à jour. L'un et l'autre alertent quand ils
  échouent (§ 3), l'état de l'archive après dix minutes de retard.
- **Base abîmée, hôte intact** : l'archive est sur l'hôte, à jour à cinq minutes près.
- **Sans archive** (destinataires non donnés à la base) : le dernier dump copié, soit
  **environ un jour**, plus le délai de sa copie hors site.

La période gardée par la conservation (`prune.sh <jours>`, ADR-215) borne jusqu'où la base
peut remonter : 30 jours dans l'exemple du § 3.

### Durée de reprise (RTO)

La durée d'une reprise est la somme de ses étapes, à mesurer par un exercice (§ 6) :

| Étape                                                          | Ordre de grandeur                                            |
| -------------------------------------------------------------- | ------------------------------------------------------------ |
| Hôte de secours prêt (Docker, `age`, `rsync`, dépôt, réglages) | 30 min, moins s'il est préparé à l'avance                    |
| Sauvegarde de base et archive placées depuis la copie          | taille ÷ débit, peu si la copie est sur l'hôte de secours    |
| Sauvegarde de base déchiffrée et déballée                      | quelques minutes par dizaine de Go                           |
| WAL rejoués depuis la sauvegarde de base                       | borné par une journée de WAL avec une sauvegarde chaque nuit |
| Pile démarrée et vérifiée (§ 5)                                | 15 min                                                       |

Objectif proposé : **quatre heures** pour une installation de quelques dizaines de Go, à
confirmer ou corriger par l'exercice. Une sauvegarde de base plus fréquente raccourcit le
rejeu. `restore-base.sh` arrête une récupération qui dure plus de
`MIMORYA_RESTORE_TIMEOUT` (une heure par défaut) : réglez-le d'après la durée mesurée.

## 3. Préparation

À faire une fois l'installation en service, et à revoir à chaque exercice.

### Sur l'hôte de production

- [ ] Destinataires des sauvegardes (deux au moins) donnés à la base dans `.env.production`
      (`MIMORYA_BACKUP_RECIPIENTS`), et les mêmes clés publiques, une par ligne, dans
      `/srv/mimorya/destinataires.txt`, que lit `backup.sh`. Identités gardées hors du
      serveur, avec la clé maître et `.env.production`.
- [ ] Tâches planifiées de `root` (`crontab -e`) ; `votre-alerte` est votre commande
      d'alerte :

  ```cron
  PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  COMPOSE_FILE=compose.production.yml
  COMPOSE_ENV_FILES=/srv/mimorya/.env.production
  MIMORYA_BACKUP_RECIPIENTS_FILE=/srv/mimorya/destinataires.txt
  # Chaque nuit : le dump ; puis la sauvegarde de base et, si elle a réussi, la conservation.
  0 2 * * *    cd /srv/mimorya && infra/backup/backup.sh || votre-alerte "dump"
  15 2 * * *   cd /srv/mimorya && infra/backup/backup.sh --base && infra/backup/prune.sh 30 || votre-alerte "sauvegarde de base"
  # Toutes les dix minutes : l'état de l'archive.
  */10 * * * * cd /srv/mimorya && infra/backup/archive-status.sh --base-within 26 >/dev/null || votre-alerte "archive des WAL"
  ```

- [ ] Les deux clés SSH publiques faites sur l'hôte de secours (voir plus bas, à faire
      d'abord) admises pour `root`, chacune limitée à la lecture d'un seul dossier par
      `rrsync` (`/usr/bin/rrsync` depuis rsync 3.2.4, qui demande `python3` ; avant, à
      installer depuis `/usr/share/doc/rsync/scripts/`), dans `/root/.ssh/authorized_keys`,
      `PermitRootLogin forced-commands-only` dans la configuration de `sshd` :

  ```text
  command="rrsync -ro /srv/mimorya/wal-archive/",restrict ssh-ed25519 AAAA… archive@secours
  command="rrsync -ro /srv/mimorya/backups/",restrict ssh-ed25519 AAAA… sauvegardes@secours
  ```

- [ ] Avec `MIMORYA_TLS=internal`, une copie de la racine locale de Caddy, faite hors du
      dépôt, sortie de l'hôte (gardée avec la clé maître) puis effacée :

  ```bash
  (umask 077 && docker compose run --rm --no-deps -T --entrypoint tar proxy -czf - -C /data/caddy pki \
    >/root/caddy-pki.tar.gz)
  ```

### Sur l'hôte de secours

- [ ] Docker, `age`, `rsync`, le dépôt extrait à la version de l'installation,
      `.env.production` et la clé maître à portée.
- [ ] Les dossiers de la copie et des instantanés, les deux clés SSH, et l'empreinte de
      l'hôte de production connue :

  ```bash
  mkdir -p /srv/mimorya-copie/wal-archive /srv/mimorya-copie/backups /srv/mimorya-instantanes
  ssh-keygen -t ed25519 -N '' -f /root/.ssh/archive && ssh-keygen -t ed25519 -N '' -f /root/.ssh/sauvegardes
  ssh-keyscan production >>/root/.ssh/known_hosts   # empreinte à vérifier
  ```

- [ ] La copie hors site, **tirée** par l'hôte de secours, en `root`, une destination par
      verrou, un transfert bloqué abandonné au bout de dix minutes (`--timeout`).
      `--numeric-ids` garde à l'archive son propriétaire, la base (uid 999) ;
      `--no-devices --no-specials` refuse les fichiers spéciaux qu'un hôte compromis
      enverrait. La copie suit l'hôte, suppressions comprises (`--delete`) : un intrus sur
      l'hôte de production peut la vider. Seul le contenu des instantanés déjà faits est
      hors de sa portée : ils partagent leurs fichiers avec la copie (`cp -al`), dont il
      peut changer le propriétaire ou les droits, que le placement rétablit (§ 4). Ils
      sont datés par eux-mêmes et gardés 90 jours ici, ce qui borne aussi ce que la copie
      garde des données supprimées. L'instantané prend le verrou des sauvegardes avant
      celui de l'archive, hors de la grille des cinq minutes. Gardez `rsync` à jour.

  ```cron
  */5 * * * *  flock -n /run/copie-archive.lock rsync -a --timeout=600 --numeric-ids --no-devices --no-specials --delete --exclude '.*' -e 'ssh -i /root/.ssh/archive' root@production:./ /srv/mimorya-copie/wal-archive/ || votre-alerte "copie de l'archive"
  10 3 * * *   flock /run/copie-sauvegardes.lock rsync -a --timeout=600 --numeric-ids --no-devices --no-specials --delete --exclude '*.partial.*' --exclude '*.probe.*' -e 'ssh -i /root/.ssh/sauvegardes' root@production:./ /srv/mimorya-copie/backups/ || votre-alerte "copie des sauvegardes"
  52 4 * * *   flock /run/copie-sauvegardes.lock flock /run/copie-archive.lock sh -c 'd=/srv/mimorya-instantanes/$(date +\%F) && cp -al /srv/mimorya-copie "$d" && touch "$d" && find /srv/mimorya-instantanes -mindepth 1 -maxdepth 1 -mtime +90 -exec rm -rf {} +' || votre-alerte "instantané"
  ```

- [ ] Un premier exercice de reprise fait et minuté (§ 6).

## 4. Procédures

Deux règles valent pour toutes :

- **Ne rien jeter de l'installation en panne** avant que la reprise soit vérifiée : son
  volume de base, son archive et ses journaux peuvent encore servir.
- **Une pile restaurée ne tourne jamais entière en même temps que l'ancienne.** Elle a les
  mêmes réglages et les mêmes jetons : ses workers relanceraient les tâches planifiées,
  enverraient des e-mails et des messages aux passerelles, et renouvelleraient les jetons
  OAuth d'hier. Tant que l'ancienne peut tourner, seule la base de la nouvelle démarre
  (`docker compose up -d db`) ; la pile entière ne démarre qu'une fois l'ancienne
  arrêtée. `restore-base.sh` termine en disant de démarrer la pile : dans ce cas, ne
  démarrez que la base.

L'identité `age` est apportée sur l'hôte le temps de la restauration, puis retirée.

Sur l'hôte de secours, l'archive se place dans un dossier vidé de ce qu'une restauration
précédente y a laissé, depuis la copie (la copie de l'archive en pause le temps de le
faire) ou un instantané. Propriétaire et droits sont rendus à la base, quoi qu'en dise
la source :

```bash
flock /run/copie-archive.lock rsync -a --delete --chown=999:999 --chmod=D700,F600 \
  /srv/mimorya-copie/wal-archive/ /srv/mimorya/wal-archive/
```

Avec `MIMORYA_TLS=internal`, la racine de Caddy gardée au § 3, apportée comme l'identité,
se remet dans le volume `caddy-data` de la nouvelle pile, avant son démarrage, sauf après
une intrusion (4.4), puis se retire :

```bash
docker compose run --rm --no-deps -T --entrypoint tar proxy -xzf - -C /data/caddy </root/caddy-pki.tar.gz
rm /root/caddy-pki.tar.gz
```

### 4.1 Erreur dans les données

Des données supprimées ou modifiées par erreur, par une personne, un agent ou un défaut :

1. Noter l'instant de l'erreur, au plus juste, avec son fuseau : `UTC`, `GMT` ou un
   décalage (`+01`, `-05:30`), jamais `Z` ni un nom de fuseau. Le journal d'audit le dit
   souvent.
2. Sur l'hôte de secours, placer l'archive (voir plus haut).
3. Restaurer la plus récente sauvegarde de base terminée avant l'instant, jusqu'à lui :

   ```bash
   infra/backup/restore-base.sh /srv/mimorya-copie/backups/mimorya-….base.tar.gz.age identite.txt '2026-11-20 14:29:00+01'
   ```

4. Démarrer **la seule base** (`docker compose up -d db`) et vérifier que les données
   perdues y sont (`docker compose exec db psql -U mimorya -d mimorya`).
5. Choisir :
   - **réintégrer** les seules données perdues dans la pile en service, par un export
     ciblé depuis la base restaurée : réservé à qui connaît le schéma, à faire relire.
     Puis, **sur l'hôte de secours seulement** (les chemins sont les mêmes sur l'hôte de
     production ; `secours` est ici le nom de l'hôte de secours), jeter la pile restaurée et
     l'archive qu'elle a laissée :
     `[ "$(hostname)" = secours ] && docker compose down -v && rm -rf /srv/mimorya/wal-archive` ;
   - **reprendre sur la pile restaurée** : arrêter l'ancienne (`docker compose stop` sur
     son hôte), remettre la racine de Caddy le cas échéant, démarrer la nouvelle
     (`docker compose up -d`), faire pointer le domaine vers elle, et vérifier (§ 5). Ce
     qui a été fait entre l'instant et la bascule est à refaire : dites-le aux
     entreprises. Au démarrage, une occurrence de tâche planifiée en retard d'une heure
     au plus est lancée (ADR-080), même si l'ancienne pile l'a déjà fait ; ses auteurs
     peuvent la mettre en pause avant. De même, un run en file ou interrompu à l'instant
     repart si la bascule vient moins d'une heure après lui. Un jeton OAuth renouvelé par
     l'ancienne pile depuis l'instant est à autoriser de nouveau.

### 4.2 Perte de l'hôte

L'hôte est détruit, inaccessible ou son disque perdu :

1. S'assurer que l'ancien hôte ne peut plus joindre le réseau : coupé depuis la console
   de l'hébergeur. Un hôte seulement injoignable peut encore tourner, et un hôte
   seulement éteint redémarre sa pile, connectée, quand on le rallume (ses services
   redémarrent d'eux-mêmes). Ne le reconnecter qu'après avoir arrêté sa pile
   (`docker compose stop`), isolé ou démarré en mode de secours.
2. Sur l'hôte de secours : `.env.production` et la clé maître repris de leur copie ;
   l'archive placée (voir plus haut), depuis la copie ou son dernier instantané.
3. Rejouer toute l'archive, sans instant :

   ```bash
   infra/backup/restore-base.sh /srv/mimorya-copie/backups/mimorya-….base.tar.gz.age identite.txt
   ```

4. Remettre la racine de Caddy le cas échéant, démarrer la pile (`docker compose up -d`),
   faire pointer le domaine vers l'hôte de secours, et vérifier (§ 5).

Sans archive, le dernier dump se restaure dans la base démarrée seule, puis la pile
(ADR-166), après l'étape 1 et la racine de Caddy remise :

```bash
docker compose up -d db
infra/backup/restore.sh /srv/mimorya-copie/backups/mimorya-….dump.age identite.txt
docker compose up -d
```

### 4.3 Base abîmée, hôte intact

Le volume de la base est corrompu, ou la base ne démarre plus :

1. Arrêter la pile, puis mettre de côté le volume abîmé, par l'image de la base elle-même,
   sur un disque qui a la place. La copie doit se relire avant que rien ne soit retiré :

   ```bash
   docker compose stop
   mkdir -p /srv/mimorya-incident && df -h /srv/mimorya-incident
   docker compose run --rm --no-deps -T --entrypoint tar db -czf - -C /var/lib/postgresql/data . \
     >/srv/mimorya-incident/db-data.tar.gz && gzip -t /srv/mimorya-incident/db-data.tar.gz
   ```

2. Retirer le seul volume de la base, celui de Caddy restant :

   ```bash
   docker compose down
   docker volume rm mimorya-production_db-data
   ```

3. Rejouer toute l'archive, sur l'hôte même, sur la dernière sauvegarde de base, puis
   démarrer la pile et vérifier (§ 5) :

   ```bash
   infra/backup/restore-base.sh /srv/mimorya/backups/mimorya-….base.tar.gz.age identite.txt
   docker compose up -d
   ```

   Les WAL du volume abîmé qui n'étaient pas encore archivés (cinq minutes au plus)
   restent dans la copie mise de côté, pour l'analyse.

### 4.4 Intrusion

Un accès illégitime à l'hôte ou à l'application est suspecté :

1. Isoler l'hôte (réseau), sans l'éteindre si une analyse est prévue ; garder ses
   journaux. Sur l'hôte de secours, suspendre les tâches de la copie et des instantanés
   jusqu'à ce que la reprise soit vérifiée : sinon la copie suivrait l'hôte compromis, et
   le nettoyage des instantanés pourrait retirer celui dont la reprise a besoin.
2. Reprendre sur un hôte neuf, comme en 4.1, à un instant antérieur à l'intrusion, depuis
   un instantané fait avant elle : la copie elle-même suit l'hôte compromis, et l'intrus a
   pu y faire entrer des fichiers ou en retirer. Jamais sur l'hôte compromis.
3. Changer tout ce que l'intrus a pu lire :
   - les mots de passe de la base (`ALTER ROLE` sur la base restaurée, puis
     `.env.production`) ;
   - les secrets de l'exploitant dans `.env.production` : clés des fournisseurs de modèle,
     identifiants du relais SMTP ;
   - les identités `age` et leurs destinataires (4.5), puis une sauvegarde de base
     aussitôt ;
   - les clés TLS : avec un certificat public, révoquer l'ancien auprès de son autorité,
     la pile neuve en obtenant un autre ; avec `internal`, une racine nouvelle, l'ancienne
     retirée des postes, et la copie du § 3 jamais remise ;
   - chez leurs fournisseurs, les secrets déclarés par les entreprises (applications,
     jetons des passerelles, clés de modèle) ;
   - la clé maître ne se change pas sans re-sceller les secrets : la tenir pour exposée.
     Les secrets de la base et les sceaux du journal d'audit (ADR-178) sont alors
     falsifiables par qui la détient.
4. Prévenir les entreprises et, s'il y a lieu, l'autorité de contrôle (violation de
   données personnelles, 72 heures).

### 4.5 Identité `age` perdue

1. Les sauvegardes faites pour l'autre destinataire restent lisibles avec son identité.
2. Créer une identité nouvelle. Remplacer la clé publique perdue dans `.env.production`
   (`MIMORYA_BACKUP_RECIPIENTS`) et dans `/srv/mimorya/destinataires.txt`. Recréer le
   conteneur de la base (`docker compose up -d db`), qui chiffre dès lors les WAL suivants
   pour les nouveaux destinataires. Faire aussitôt une sauvegarde de base et un dump.
3. Les sauvegardes et WAL antérieurs chiffrés pour la seule identité perdue sont
   illisibles : la période restaurable commence à la nouvelle sauvegarde de base.

### 4.6 Archivage en échec

`archive-status.sh` sort en erreur :

1. Lire sa raison, puis le journal de la base (`docker compose logs db`) : dossier plein,
   en lecture seule, monté sans liens physiques, destinataire refusé.
2. Corriger. PostgreSQL garde les WAL non archivés et les archive une fois l'archivage
   reparti : l'archive n'aura pas de trou. Mais tant qu'il échoue, la copie hors site
   n'avance plus et le disque de la base se remplit : une perte de l'hôte dans
   l'intervalle perdrait tout ce qui n'est pas archivé.
3. Vérifier que l'état revient à zéro.

## 5. Après une reprise

- [ ] `infra/backup/archive-status.sh` sort à zéro : la base restaurée archive, sur sa
      nouvelle ligne temporelle.
- [ ] Une sauvegarde de base est faite aussitôt (`backup.sh --base`), et un dump.
- [ ] Les tâches du § 3 tournent pour la pile reprise : sauvegardes, conservation, état
      de l'archive, et la copie tirée, sur l'hôte qui sert désormais de secours.
- [ ] Le numéro de ligne temporelle dit par la restauration est noté : une restauration
      ultérieure qui partirait d'une sauvegarde de base antérieure le nomme
      (`MIMORYA_RESTORE_TIMELINE`).
- [ ] Le journal d'audit se vérifie (`docker compose exec api mimorya-admin verify-audit`,
      ADR-178) ; ses entrées postérieures à l'instant sont parties avec les données.
- [ ] Un administrateur de chaque entreprise se connecte et retrouve ses données.
- [ ] L'ancienne installation est gardée jusqu'à cette vérification, puis retirée.
- [ ] Les entreprises sont informées de l'heure de reprise et de ce qui est à refaire.

## 6. Exercices

Chaque trimestre, et après tout changement de l'installation, sur l'hôte de secours, sans
rien démarrer d'autre que la base restaurée :

1. Placer l'archive depuis le dernier instantané de la copie (§ 4).
2. Choisir un instant de la veille et restaurer la plus récente sauvegarde de base
   terminée avant lui (4.1 étape 3), puis démarrer la seule base.
3. Vérifier les entreprises et leurs dernières données (`psql`), et le journal d'audit
   (`docker compose run --rm --no-deps api mimorya-admin verify-audit`, une commande qui ne
   lance aucune tâche de fond).
4. Minuter chaque étape du § 2, puis, sur l'hôte de secours, jeter la pile et l'archive
   qu'elle a laissée
   (`[ "$(hostname)" = secours ] && docker compose down -v && rm -rf /srv/mimorya/wal-archive`),
   et retirer l'identité.
5. Noter le résultat ci-dessous et corriger l'objectif ou la préparation.

| Date | Taille de la base | Durée totale | Écarts constatés | Suite donnée |
| ---- | ----------------- | ------------ | ---------------- | ------------ |
|      |                   |              |                  |              |

La CI joue, à chaque changement, les sauvegardes et les restaurations d'une pile qu'elle
installe (`infra/docker/backup-test.sh`, `point-in-time-test.sh`,
`archive-retention-test.sh`). Elle ne joue ni les tâches planifiées, ni la copie hors site
et ses instantanés, ni la bascule : l'exercice les vérifie, avec les identités et les
accès de l'exploitant.

## 7. Registre de l'installation

À tenir par l'exploitant, hors du serveur :

| Élément                                                              | Valeur |
| -------------------------------------------------------------------- | ------ |
| Version en service (`MIMORYA_VERSION`)                               |        |
| Hôte de secours, et accès à la copie                                 |        |
| Garde des identités `age`, de la clé maître et de la racine de Caddy |        |
| Personnes à prévenir, et comment                                     |        |
| Période gardée sur l'hôte (`prune.sh`)                               |        |
| Durée des instantanés de la copie                                    |        |
