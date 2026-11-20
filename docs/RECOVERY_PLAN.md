# Plan de reprise

Ce plan dit comment une installation de production (`compose.production.yml`) protège ses
données, combien elle peut en perdre et en combien de temps elle repart, et quoi faire,
dans l'ordre, selon ce qui est arrivé (ADR-216). Il s'appuie sur les sauvegardes chiffrées
(ADR-166), l'archive continue des WAL (ADR-206), la restauration à un instant précis
(ADR-214) et la conservation de l'archive (ADR-215). Les commandes supposent la pile
lancée depuis la racine du dépôt, à la version de l'installation, avec
`COMPOSE_ENV_FILES=.env.production` ; le README en donne le détail.

## 1. Ce qui est protégé, et où

Tout ce que la plateforme garde (entreprises, comptes, conversations, mémoires,
documents déposés, journal d'audit, secrets scellés) est dans la base PostgreSQL. Le
reste, sur l'hôte, se refait ou se garde à part :

| Élément                                              | Protection                                                                                              | Où le garder                              |
| ---------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ----------------------------------------- |
| Base                                                 | dump chaque nuit, sauvegarde de base chaque nuit, archive continue des WAL, chiffrés par `age`          | hors site, copiés depuis l'hôte           |
| Clé maître (`MIMORYA_MASTER_KEY`)                    | aucune dans les sauvegardes : sans elle, les secrets de la base (jetons, mots de passe) sont illisibles | hors du serveur, avec les identités `age` |
| Identités `age` (clés privées)                       | deux destinataires au moins, l'un gardé à part : une identité perdue rend ses sauvegardes illisibles    | hors du serveur, jamais sur l'hôte        |
| `.env.production` (mots de passe, domaine, réglages) | les mots de passe de la base font partie d'une sauvegarde de base : la pile restaurée garde les mêmes   | hors du serveur, avec la clé maître       |
| Version de l'installation (`MIMORYA_VERSION`)        | images publiées par version (ADR-051) ; le dépôt extrait à la même version                              | noter la version en service               |
| Certificats TLS (Caddy)                              | refaits au démarrage                                                                                    | rien à garder                             |

## 2. Objectifs

### Perte de données (RPO)

- **Erreur dans les données** (suppression par mégarde, entreprise supprimée à tort) : la
  base revient à l'instant choisi, juste avant l'erreur ; ce qui a été fait ensuite se
  reprend à la main depuis l'ancienne pile, gardée.
- **Perte de l'hôte** : au plus `archive_timeout` (cinq minutes, ADR-206) plus
  l'intervalle de la copie hors site de l'archive. Avec une copie toutes les cinq minutes,
  **dix minutes au plus**.
- **Base abîmée, hôte intact** : l'archive est sur l'hôte, à jour à cinq minutes près.
- **Sans archive** (destinataires non donnés à la base) : le dernier dump, jusqu'à **24
  heures**.

La période gardée par la conservation (`prune.sh <jours>`, ADR-215) borne jusqu'où la base
peut remonter : 30 jours dans l'exemple du § 3.

### Durée de reprise (RTO)

La durée d'une reprise est la somme de ses étapes, à mesurer par un exercice (§ 6) :

| Étape                                                     | Ordre de grandeur                                            |
| --------------------------------------------------------- | ------------------------------------------------------------ |
| Hôte prêt (Docker, dépôt à la version, `.env.production`) | 30 min, moins si l'hôte de secours est déjà préparé          |
| Sauvegarde de base et archive ramenées de hors site       | taille ÷ débit du lien                                       |
| Sauvegarde de base déchiffrée et déballée                 | quelques minutes par dizaine de Go                           |
| WAL rejoués depuis la sauvegarde de base                  | borné par une journée de WAL avec une sauvegarde chaque nuit |
| Pile démarrée et vérifiée (§ 5)                           | 15 min                                                       |

Objectif proposé : **quatre heures** pour une installation de quelques dizaines de Go, à
confirmer ou corriger par l'exercice. Une sauvegarde de base plus fréquente raccourcit
le rejeu.

## 3. Préparation

À faire une fois l'installation en service, et à revoir à chaque exercice :

- [ ] Destinataires des sauvegardes donnés à la base (`MIMORYA_BACKUP_RECIPIENTS`, deux au
      moins), identités gardées hors du serveur, avec la clé maître et `.env.production`.
- [ ] Tâches planifiées sur l'hôte, en `root` (l'archive appartient à la base, uid 999) :

  ```cron
  # Chaque nuit : dump, puis sauvegarde de base, puis conservation sur 30 jours.
  15 2 * * *  cd /srv/mimorya && COMPOSE_ENV_FILES=.env.production infra/backup/backup.sh && COMPOSE_ENV_FILES=.env.production infra/backup/backup.sh --base && COMPOSE_ENV_FILES=.env.production infra/backup/prune.sh 30
  # Toutes les cinq minutes : copie hors site de l'archive et des sauvegardes, sans
  # suppression, heures des fichiers gardées (-a).
  */5 * * * * rsync -a /srv/mimorya/wal-archive/ sauvegarde@secours:/srv/mimorya-copie/wal-archive/
  */5 * * * * rsync -a /srv/mimorya/backups/ sauvegarde@secours:/srv/mimorya-copie/backups/
  # Toutes les dix minutes : état de l'archive ; votre commande d'alerte sur toute sortie
  # non nulle.
  */10 * * * * cd /srv/mimorya && COMPOSE_ENV_FILES=.env.production infra/backup/archive-status.sh --base-within 26 || votre-alerte
  ```

- [ ] Hôte de secours identifié, Docker installé, accès à la copie hors site.
- [ ] Premier exercice de reprise fait et minuté (§ 6).

## 4. Procédures

Chaque procédure commence par la même règle : **ne rien jeter de l'installation en
panne** avant que la reprise soit vérifiée. Son volume de base, son archive et ses
journaux peuvent encore servir.

### 4.1 Erreur dans les données

Des données supprimées ou modifiées par erreur, par une personne, un agent ou un défaut :

1. Noter l'instant de l'erreur, au plus juste, avec son fuseau. Le journal d'audit le dit
   souvent.
2. Préparer une pile nouvelle sur un autre hôte : même version, même clé maître, mêmes
   mots de passe, `MIMORYA_WAL_ARCHIVE` vers une copie de l'archive (`rsync -a` ou
   `cp -a`). Pour seulement relire des données, elle peut tourner sur le même hôte, sous
   un autre nom de projet (`COMPOSE_PROJECT_NAME=mimorya-reprise`), sa seule base
   démarrée : le service `proxy` de la pile en service tient déjà les ports 80 et 443.
3. Restaurer la plus récente sauvegarde de base terminée avant l'instant, jusqu'à lui :

   ```bash
   COMPOSE_ENV_FILES=.env.production infra/backup/restore-base.sh \
     backups/mimorya-….base.tar.gz.age identite.txt '2026-11-20 14:29:00+01'
   ```

4. Démarrer la pile restaurée et vérifier (§ 5) que les données perdues y sont.
5. Choisir :
   - **reprendre sur la pile restaurée** : arrêter l'ancienne, faire pointer le domaine
     vers la nouvelle, dire aux entreprises ce qui a été fait entre l'instant et la
     bascule, à refaire ;
   - **réintégrer** les seules données perdues dans la pile en service, par un export
     ciblé depuis la base restaurée : réservé à qui connaît le schéma, à faire relire.

### 4.2 Perte de l'hôte

L'hôte est détruit, inaccessible ou son disque perdu :

1. Préparer l'hôte de secours : Docker, dépôt extrait à la version de l'installation,
   `.env.production` et clé maître repris de leur copie.
2. Ramener de la copie hors site la dernière sauvegarde de base et l'archive (dans le
   dossier `MIMORYA_WAL_ARCHIVE`, en gardant les heures des fichiers : `rsync -a`).
3. Rejouer toute l'archive, sans instant :

   ```bash
   COMPOSE_ENV_FILES=.env.production infra/backup/restore-base.sh \
     backups/mimorya-….base.tar.gz.age identite.txt
   ```

4. Démarrer la pile, faire pointer le domaine vers l'hôte de secours, vérifier (§ 5).

Sans archive, le dernier dump se restaure dans la base démarrée seule, puis la pile
(ADR-166) :

```bash
docker compose --env-file .env.production -f compose.production.yml up -d db
COMPOSE_ENV_FILES=.env.production infra/backup/restore.sh backups/mimorya-….dump.age identite.txt
docker compose --env-file .env.production -f compose.production.yml up -d
```

### 4.3 Base abîmée, hôte intact

Le volume de la base est corrompu, ou la base ne démarre plus :

1. Arrêter la pile, sans `-v`. Mettre de côté le volume abîmé, par l'image de la base
   elle-même, pour l'analyse :

   ```bash
   docker compose --env-file .env.production -f compose.production.yml stop
   docker compose --env-file .env.production -f compose.production.yml run --rm --no-deps -T \
     --entrypoint tar db -czf - -C /var/lib/postgresql/data . >/srv/mimorya-incident/db-data.tar.gz
   ```

2. Vider la base (`docker compose down -v`) : l'archive, sur l'hôte, reste.
3. Rejouer toute l'archive sur la dernière sauvegarde de base, comme en 4.2 étape 3.
4. Démarrer la pile et vérifier (§ 5).

### 4.4 Intrusion

Un accès illégitime à l'hôte ou à l'application est suspecté :

1. Isoler l'hôte (réseau), sans l'éteindre si une analyse est prévue ; garder ses
   journaux.
2. Reprendre sur un hôte neuf (4.1, à un instant antérieur à l'intrusion), jamais sur
   l'hôte compromis.
3. Changer tout ce que l'intrus a pu lire : mots de passe de la base (`ALTER ROLE` sur la
   base restaurée, puis `.env.production`), identités `age` et destinataires (une
   sauvegarde de base aussitôt après), et, chez leurs fournisseurs, les secrets déclarés
   par les entreprises (applications, jetons des passerelles, clés de modèle). La clé
   maître ne se change pas sans re-sceller les secrets : la traiter comme exposée.
4. Prévenir les entreprises et, s'il y a lieu, l'autorité de contrôle (violation de
   données personnelles, 72 heures).

### 4.5 Identité `age` perdue

1. Les sauvegardes faites pour l'autre destinataire restent lisibles avec son identité.
2. Créer une identité nouvelle, remplacer la clé publique perdue dans
   `MIMORYA_BACKUP_RECIPIENTS`, redémarrer la base (les WAL suivants sont chiffrés pour
   les nouveaux destinataires), puis faire aussitôt une sauvegarde de base et un dump.
3. Les sauvegardes et WAL antérieurs chiffrés pour la seule identité perdue sont
   illisibles : la période restaurable commence à la nouvelle sauvegarde de base.

### 4.6 Archivage en échec

`archive-status.sh` sort en erreur :

1. Lire sa raison, puis le journal de la base (`docker compose logs db`) : dossier plein,
   en lecture seule, monté sans liens physiques, destinataire refusé.
2. Corriger. PostgreSQL réessaie seul ; tant qu'il échoue, il garde ses WAL, au prix de
   son disque, à surveiller.
3. Vérifier que l'état revient à zéro, puis faire une sauvegarde de base si l'échec a
   duré : la période sans archive n'est restaurable qu'à ses extrémités.

## 5. Après une reprise

- [ ] `infra/backup/archive-status.sh` sort à zéro : la base restaurée archive, sur sa
      nouvelle ligne temporelle.
- [ ] Une sauvegarde de base est faite aussitôt (`backup.sh --base`), et un dump.
- [ ] Le numéro de ligne temporelle dit par la restauration est noté : une restauration
      ultérieure qui partirait d'une sauvegarde de base antérieure le nomme
      (`MIMORYA_RESTORE_TIMELINE`).
- [ ] Le journal d'audit se vérifie (`docker compose exec api mimorya-admin verify-audit`,
      ADR-178) ; ses entrées postérieures à l'instant sont parties avec les données.
- [ ] Un administrateur de chaque entreprise se connecte et retrouve ses données.
- [ ] L'ancienne installation est gardée jusqu'à cette vérification, puis retirée ; son
      archive reste dans le même dossier, et la conservation (ADR-215) la retire avec la
      période.
- [ ] Les entreprises sont informées de l'heure de reprise et de ce qui est à refaire.

## 6. Exercices

Chaque trimestre, et après tout changement de l'installation :

1. Ramener de la copie hors site la sauvegarde de base de la nuit et l'archive.
2. Les restaurer dans une pile jetable, sur un autre hôte, à un instant de la veille.
3. Minuter chaque étape du § 2, vérifier comme au § 5, puis jeter la pile (`down -v`).
4. Noter le résultat ci-dessous et corriger l'objectif ou la préparation.

| Date | Taille de la base | Durée totale | Écarts constatés | Suite donnée |
| ---- | ----------------- | ------------ | ---------------- | ------------ |
|      |                   |              |                  |              |

La CI joue déjà, à chaque changement, les restaurations d'une pile qu'elle installe
(`infra/docker/backup-test.sh`, `point-in-time-test.sh`, `archive-retention-test.sh`) :
l'exercice vérifie en plus les copies hors site, les identités et les accès de
l'exploitant.

## 7. Registre de l'installation

À tenir par l'exploitant, hors du serveur :

| Élément                                       | Valeur |
| --------------------------------------------- | ------ |
| Version en service (`MIMORYA_VERSION`)        |        |
| Hôte de secours                               |        |
| Copie hors site (emplacement, accès)          |        |
| Garde des identités `age` et de la clé maître |        |
| Personnes à prévenir, et comment              |        |
| Période gardée (`prune.sh`)                   |        |
