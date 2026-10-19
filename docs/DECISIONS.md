# Journal des décisions d'architecture (ADR)

Format court : contexte, décision, conséquences. Une décision n'est jamais modifiée :
elle est remplacée ou complétée par une nouvelle entrée qui la référence (« remplace
ADR-00X », « complète ADR-00X »). Seules des corrections rédactionnelles (libellé
cité, renvoi) y sont faites sur place.

---

## ADR-001 — Monolithe modulaire TypeScript

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : produit ambitieux mais équipe réduite ; domaine encore mouvant ;
  besoin de transactions traversant autorisation, audit et exécution.
- **Décision** : un dépôt pnpm, TypeScript strict de bout en bout ; paquets `core`,
  `db`, `agent`, `connectors` ; applications `api` (Fastify) et `web` (Next.js). Le
  futur worker réutilisera le code de l'API avec un autre point d'entrée. Python n'est
  introduit que pour un besoin spécifique (OCR, ML) sous forme de service annexe.
- **Conséquences** : types partagés, un seul outillage, déploiement simple pour une PME.
  Les frontières de paquets doivent être respectées pour pouvoir extraire des services.

## ADR-002 — Hermes Agent : reprendre les concepts et le format des compétences, pas le code

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : Hermes Agent (MIT) est un agent personnel Python d'environ 1 M de
  lignes, mono-utilisateur, à état sur disque, dont la sécurité repose sur l'isolation
  du système d'exploitation. Analyse détaillée : [ARCHITECTURE.md](ARCHITECTURE.md) §12.
- **Décision** : réimplémenter en TypeScript la boucle, la mémoire, la délégation, le
  cron et les passerelles en reprenant leurs principes ; adopter le format `SKILL.md`
  compatible agentskills.io (extensions sous `metadata.mimorya.*`) ; ne pas embarquer
  le code Hermes ni exécuter d'instance Hermes par entreprise.
- **Conséquences** : effort de développement plus élevé, mais isolation, permissions et
  audit natifs. Import possible des compétences MIT/Apache avec attribution. Le
  catalogue de plugins Hermes contient du GPL/AGPL : exclu.

## ADR-003 — PostgreSQL unique, schéma partagé, isolation par RLS

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : SaaS multi-entreprises pour TPE/PME, coût d'exploitation à maîtriser,
  exigence forte d'isolation.
- **Décision** : une base, un schéma, `tenant_id` partout, RLS sur toutes les tables
  métier, rôle applicatif sans `BYPASSRLS`, contexte posé par transaction. Fonctions
  `SECURITY DEFINER` limitées pour la connexion et la résolution de session.
- **Alternatives** : base par entreprise (isolation maximale, exploitation lourde) —
  conservée pour les déploiements privés et, plus tard, pour de grands comptes.
- **Conséquences** : chaque accès base passe par `withTenant()` ; tests d'isolation
  obligatoires.

## ADR-004 — Recherche hybride FTS + pgvector, vecteurs de dimension 1024

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : plein texte PostgreSQL (configuration `french`) toujours disponible ;
  embeddings optionnels en `vector(1024)` (index HNSW cosinus), fusion RRF. 1024 est
  supporté par de nombreux modèles multilingues (bge-m3, mistral-embed, voyage,
  text-embedding-3 avec `dimensions=1024`, e5-large).
- **Conséquences** : pas de base vectorielle séparée ; changer de modèle d'embedding
  impose une ré-indexation (colonne `embedding_model`).

## ADR-005 — Pas de Redis dans le MVP

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : files de travaux, verrous et diffusion d'événements reposent sur
  PostgreSQL (`SKIP LOCKED`, baux, puis `LISTEN/NOTIFY`). Redis sera introduit quand la
  charge le justifiera (cache, limitation de débit distribuée, diffusion multi-instances).
- **Conséquences** : un composant de moins à opérer pour les installations privées.

## ADR-006 — API Fastify séparée de Next.js

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : exécutions longues, SSE, webhooks entrants, workers, passerelles.
- **Décision** : Next.js sert l'interface ; Fastify porte l'API. Le navigateur appelle
  `/api/*` sur l'origine web, relayé vers l'API (même origine, pas de CORS).
- **Conséquences** : deux processus en développement ; le reverse proxy de production
  route `/api` directement vers l'API.

## ADR-007 — Couche LLM maison sur SDK officiels

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : interface `LlmProvider` avec adaptateurs Anthropic (`@anthropic-ai/sdk`)
  et OpenAI-compatible (`openai`, couvre OpenAI, OpenRouter, Gemini, Mistral, Ollama,
  vLLM), plus un fournisseur scénarisé pour les tests. Pas de framework d'agent tiers.
- **Conséquences** : maîtrise du format des outils, des coûts et des erreurs ;
  maintenance des adaptateurs à notre charge.

## ADR-008 — Drizzle ORM et migrations SQL versionnées

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : schéma déclaré en TypeScript, migrations générées par drizzle-kit puis
  relues ; migrations manuelles pour RLS, rôles, fonctions et index spécifiques.
- **Conséquences** : SQL lisible et revu ; exécution des migrations par le rôle
  propriétaire uniquement.

## ADR-009 — Sessions opaques en base, SSO OIDC en phase 2

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : jetons opaques aléatoires (empreinte stockée), cookie `HttpOnly`,
  révocables instantanément ; pas de JWT de session. Mots de passe Argon2id. OIDC
  ajouté en phase 2 sur la même table de sessions.

## ADR-010 — Moteur d'autorisation maison (RBAC à périmètre + ABAC JSON)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : besoin de périmètres hiérarchiques, de plafonds agent et d'un
  fonctionnement hors ligne en installation privée.
- **Alternatives** : OpenFGA / SpiceDB (ReBAC, service supplémentaire), Cerbos (service
  supplémentaire), Casbin (modèle générique peu adapté aux plafonds agent).
- **Décision** : moteur pur en TypeScript dans `core`, synchrone, testé ; conditions
  ABAC dans un langage JSON minimal et auditable.
- **Conséquences** : à réévaluer si les besoins relationnels se complexifient
  (partages fins document par document).

## ADR-011 — Un compte = une entreprise (MVP)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : l'e-mail est unique globalement et rattaché à une seule entreprise.
- **Conséquences** : simplifie connexion et isolation ; une table `identities` sera
  nécessaire pour les consultants multi-entreprises (phase 3).

## ADR-012 — Exécutions longues par baux PostgreSQL

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : un run en file est réservé par `FOR UPDATE SKIP LOCKED` avec un bail ;
  l'état est persisté à chaque étape ; un bail expiré permet la reprise par un autre
  worker. pg-boss sera évalué pour les tâches planifiées (phase 2).

## ADR-013 — Documentation en français, code en anglais

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : documentation produit et technique, libellés d'interface et messages
  utilisateur en français ; identifiants, commentaires de code et messages de commit en
  anglais. Internationalisation de l'interface en phase 3.

## ADR-014 — Agent : permissions non délégables et plafond de run

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : les permissions d'administration (comptes, rôles, politiques,
  connecteurs, compétences, paramètres, audit, validation) ne peuvent jamais être
  exercées par un agent, quels que soient les droits de l'utilisateur. Chaque run peut
  en outre être restreint à une liste de permissions ; un sous-agent hérite d'un
  sous-ensemble.
- **Conséquences** : l'administration reste une action humaine explicite ; aucune
  injection ni compétence ne peut élever des privilèges.

## ADR-015 — Exécution de l'API via `tsx` en MVP

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les paquets internes exposent leur source TypeScript ; un bundle
  esbuild complet de Fastify et des SDK est fragile, et une compilation par paquet
  ajoute de l'outillage avant que le besoin de performance au démarrage existe.
- **Décision** : l'API est lancée avec `tsx` en développement comme en production
  (MVP). La compilation (tsc par paquet ou bundle) sera introduite avec les images
  Docker de déploiement privé.
- **Conséquences** : démarrage un peu plus lent et transpilation à la volée ; aucun
  impact fonctionnel.

## ADR-016 — Exécution des runs dans le processus de l'API (MVP)

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les tours de conversation sont courts (secondes à minutes) et l'état
  est persisté à chaque étape.
- **Décision** : l'API lance les runs en arrière-plan dans son processus et diffuse les
  événements en mémoire (SSE). Un balayage périodique marque en échec les runs sans
  progression depuis 15 minutes (`system_fail_stale_runs`). La reprise automatique
  après arrêt et le worker séparé arrivent en phase 2 (ADR-012).
- **Conséquences** : une seule instance d'API en MVP ; le passage à plusieurs instances
  impose la diffusion via PostgreSQL `LISTEN/NOTIFY` et le worker.

## ADR-017 — Repli serveur et cache de prompt chez Anthropic

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Décision** : l'adaptateur Anthropic active le cache de prompt sur le préfixe stable
  (outils + consignes) et, sur les modèles qui le permettent, le repli côté serveur
  (`fallbacks: "default"`) : une demande refusée pour raison de politique est rejouée
  sur le modèle de repli défini par Anthropic au lieu d'échouer. Les tours de
  l'assistant sont rejoués tels quels (blocs de raisonnement compris).
- **Conséquences** : le modèle ayant réellement servi la réponse est celui enregistré
  dans la consommation ; option désactivable (`serverSideFallback: false`).

## ADR-018 — Serveur en Rust, interface web en Next.js

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Remplace** : ADR-001 (langage du serveur), ADR-006 (Fastify), ADR-007 (SDK
  TypeScript), ADR-008 (Drizzle) et ADR-015 (`tsx`). ADR-010 reste valable : le moteur
  d'autorisation est désormais en Rust dans `crates/core`.
- **Contexte** : le porteur du produit a demandé un développement en Rust ; une première
  version complète existait en TypeScript (référence : commit `63defdf`). Choix retenu :
  « option A » — toute la logique serveur en Rust, l'interface reste en Next.js.
- **Décision** : le serveur est un espace de travail Cargo (`crates/core`, `db`,
  `agent`, `connectors`, `api`) ; l'interface `apps/web` ne communique qu'en HTTP via
  `/api/*`. Bibliothèques : axum + tokio (HTTP, SSE), sqlx (SQL paramétré, migrations
  SQL versionnées dans `crates/db/migrations`, appliquées par le rôle propriétaire),
  serde + schemars + validator (une structure donne le JSON Schema montré au modèle et
  la validation stricte), reqwest (adaptateurs Anthropic et OpenAI-compatible écrits
  sur l'API HTTP), rmcp (MCP), argon2 (mots de passe). Le contrat JSON de l'API est
  conservé à l'identique ; les migrations existantes sont reprises telles quelles.
- **Conséquences** :
  - les migrations SQL deviennent la source de vérité du schéma (plus de schéma
    TypeScript ni de contrôle de dérive) ; le test d'isolation vérifie que chaque table
    portant `tenant_id` a la RLS active et une politique ;
  - deux binaires compilés (`mimorya-api`, `mimorya-seed`) ; tests unitaires par
    `cargo test`, tests PostgreSQL par `cargo test-int` (marqués `#[ignore]`) ;
  - les bases de développement créées par drizzle-kit doivent être recréées (table de
    suivi des migrations différente : `_sqlx_migrations`) ;
  - les types de réponse de l'API sont maintenus à la main côté web
    (`apps/web/lib/api/types.ts`) ; leur génération depuis le serveur est à prévoir ;
  - compilation plus longue qu'en TypeScript, compensée par la vérification statique.

## ADR-019 — Administration : périmètre entreprise, pas d'auto-administration, mots de passe provisoires

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : l'administration passe en écriture (comptes, organisation,
  habilitations). Une permission d'administration détenue sur une seule unité
  donnait déjà accès à des écrans couvrant toute l'entreprise (audit, connecteurs) ;
  aucun service d'envoi d'e-mails n'existe encore.
- **Décision** :
  - les permissions d'administration (`TENANT_WIDE_PERMISSIONS`, `crates/core`) ne
    sont honorées qu'avec un périmètre entreprise, et un rôle qui en contient ne peut
    être attribué qu'à ce périmètre ;
  - un administrateur ne modifie ni son propre compte ni ses propres habilitations ;
  - un compte est créé (ou réinitialisé) avec un mot de passe provisoire aléatoire,
    affiché une fois à l'administrateur, que la personne remplace depuis « Mon compte »
    (ses autres sessions sont alors fermées) ;
  - une unité ne peut être supprimée que si plus rien n'en dépend (sous-unités,
    membres, habilitations, mémoire) ; les comptes ne sont jamais supprimés par
    l'application (désactivation).
- **Conséquences** : le dernier administrateur ne peut pas se retirer ses droits ;
  la transmission du mot de passe provisoire repose sur l'administrateur jusqu'à
  l'invitation par e-mail ; la séparation des pouvoirs complète (double validation
  des actions d'administration) reste à faire (phase 3).

## ADR-020 — Changements d'administration sérialisés par entreprise

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : chaque requête charge une vue des droits et de l'organigramme
  (organigramme mis en cache 30 s). Deux administrateurs agissant au même instant
  pouvaient valider leurs changements sur des vues périmées : deux déplacements croisés
  créaient un cycle (l'organigramme ne se chargeait plus et toute l'entreprise
  devenait inaccessible), deux retraits croisés laissaient l'entreprise sans
  administrateur, une habilitation créée pendant une suppression d'unité disparaissait
  par cascade.
- **Décision** :
  - tout changement d'administration (comptes, habilitations, organisation) s'exécute
    dans une transaction qui prend d'abord un verrou consultatif PostgreSQL propre à
    l'entreprise (`pg_advisory_xact_lock`, `TenantTx::lock_administration`) : les
    changements d'une même entreprise passent l'un après l'autre, sans nouvelle
    infrastructure ;
  - sous ce verrou, le changement revérifie sur les données en base que
    l'administrateur est toujours actif et détient la permission sur toute
    l'entreprise, et applique les règles de l'organigramme à l'arbre relu en base
    (pas au cache) ;
  - la suppression d'une unité verrouille sa ligne (`FOR UPDATE`) avant de compter ce
    qui en dépend : ce qui la référencerait entre-temps (mémoire écrite par un run,
    par exemple) attend puis échoue ;
  - le cache des autorisations ne réenregistre pas des données chargées pendant une
    invalidation (génération incrémentée à chaque invalidation) ;
  - le changement de son propre mot de passe, hors verrou d'administration, n'écrit
    que si l'empreinte lue n'a pas changé et que la session appelante est toujours
    ouverte : une réinitialisation simultanée par un administrateur l'emporte ;
  - une connexion relit la ligne du compte, verrouillée (`FOR NO KEY UPDATE`), avant
    d'ouvrir sa session : elle est refusée si le mot de passe vérifié a changé ou si
    le compte n'est plus actif, et une révocation en attente passe après elle ;
  - ordre des verrous commun à ces opérations : ligne du compte, puis sessions.
- **Conséquences** : les changements d'administration d'une entreprise ne sont pas
  parallèles (sans effet pratique à cette échelle) ; ces courses sont testées de façon
  déterministe : le test tient les lignes concernées et attend que les requêtes soient
  en file dans `pg_locks` (`crates/api/tests/support/locks.rs`, `admin_concurrency.rs`,
  `account_password.rs`, `login_races.rs`) ; comme personne ne retire ses
  propres droits et que chaque retrait revérifie les droits de son auteur, il reste
  toujours au moins un administrateur actif (hors échéance d'habilitation choisie).

## ADR-021 — Rôles personnalisés et mot de passe provisoire à remplacer

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : une entreprise doit pouvoir définir ses propres rôles métier, et un
  mot de passe provisoire transmis par un administrateur ne doit pas rester en usage.
- **Décision** :
  - un rôle personnalisé n'accorde que des permissions du catalogue (plateforme et
    connecteurs), écrites en toutes lettres, sans joker : il ne grandit pas avec le
    catalogue ; sa clé (`custom_…`) est dérivée de son nom à la création, avec un
    suffixe (`_2`, `_3`…) si elle est déjà prise (un rôle renommé garde sa clé) ;
  - les rôles de la plateforme ne se modifient ni ne se suppriment ; personne ne
    modifie ni ne supprime un rôle qu'il détient (même règle que pour ses propres
    habilitations) ; un rôle qui contient une permission d'administration reste
    attribué sur toute l'entreprise (une modification qui laisserait une habilitation
    limitée à une unité est refusée) ; un rôle encore attribué ne se supprime pas ;
  - ces changements passent sous le verrou d'administration (ADR-020) ; la
    revérification des droits de l'auteur relit les rôles et les politiques en base,
    et une attribution relit le rôle attribué ;
  - un compte créé ou réinitialisé porte `must_change_password` ; la résolution de
    session le renvoie, et l'extracteur d'identité `Auth` refuse alors toute route
    authentifiée ; seules `/auth/me` et `/account/password` acceptent cette identité
    (`AuthPendingPasswordChange`), la déconnexion reste possible et les routes sans
    identité (connexion, santé) ne sont pas concernées ; le changement de mot de
    passe lève l'obligation, la session courante continue.
- **Conséquences** : migration `0007` (colonne et fonction `auth_resolve_session`
  recréée) ; l'interface montre un écran dédié tant que le mot de passe est
  provisoire ; l'invitation par e-mail reste à faire ; sur une instance déjà en
  service, les comptes créés ou réinitialisés avant la migration ne sont pas
  concernés (à réinitialiser au besoin).

## ADR-022 — Politiques d'accès : l'administration n'écrit que des refus

- **Date** : 2026-10-18 · **Statut** : acceptée
- **Contexte** : les politiques ABAC (`access_policies`) complètent les rôles. Une
  politique d'**autorisation** accorde l'accès à une ressource hors du périmètre des
  rôles ; écrite par un administrateur, elle peut viser ce même administrateur (une
  condition sur son identifiant, ou une condition large) et lui ouvrir les données
  métier, contournant la règle « pas d'administration de soi-même » (ADR-019) que
  seule une double validation (phase 3) peut garantir. Une politique illisible par le
  moteur empêcherait en outre de charger les autorisations de toute l'entreprise.
- **Décision** :
  - depuis l'administration (`policy:manage`, sur toute l'entreprise), on crée,
    modifie, active, désactive et supprime des politiques de **refus** uniquement ;
    les politiques d'autorisation existantes sont listées en lecture seule ;
  - une politique porte sur des permissions du catalogue ou sur `domaine:*`, jamais
    sur une permission d'administration (`TENANT_WIDE_PERMISSIONS`) ni sur celles que
    lisent ses écrans (`org:read`, `user:read`) : un refus ne peut pas bloquer les
    administrateurs ;
  - chaque permission déclare où la plateforme la vérifie (`Enforcement` : pas encore,
    sur la personne, ressource par ressource). Une politique ne vise que des
    permissions vérifiées (un refus sur une permission jamais vérifiée n'aurait aucun
    effet). Le moteur applique dans `data_scope` (donc routes, outils de l'agent,
    périmètres de liste, `/auth/me`) les refus dont la condition ne lit que le
    collaborateur et le contexte ; une condition sur la ressource ne s'évalue que
    là où chaque ressource est vérifiée (`check`) et n'est acceptée que pour ces
    permissions ;
  - la condition est désérialisée strictement puis vérifiée (`core::policy_rules`) :
    attributs sous `subject.`, `resource.` ou `context.`, au moins un attribut par
    comparaison, valeur requise sauf pour « est renseigné », listes pour « fait partie
    de », nombres pour les comparaisons d'ordre, pas de groupe vide, profondeur et
    longueur bornées, valeurs courtes (texte, nombre ou liste), sans caractère nul ;
    c'est la forme sérialisée de la condition typée qui est stockée. Une politique
    active, ou qu'on active, ou dont les permissions ou la condition changent, est
    revérifiée en entier avec ce qui est stocké (permissions et condition peuvent
    précéder une règle, ou avoir été écrites hors de l'API) ; une politique inactive
    se renomme ou reste désactivée quoi qu'elle contienne ;
  - les nombres se comparent par valeur (`2` et `2.0` sont égaux), comme l'éditeur
    web les affiche ;
  - là où la plateforme montre qu'une ressource existe sans la lire, un refus de
    lecture s'applique aussi : le créneau d'une intervention illisible n'apparaît que
    comme « occupé » (disponibilités, chevauchements d'une proposition) ; l'index des
    compétences du prompt suit `skill:read` de l'agent ; valider une action en
    attente reprend l'agent et exige donc `agent:use` ;
  - les changements passent sous le verrou d'administration (ADR-020), sont audités
    (avec l'état précédent) et s'appliquent aussitôt (cache invalidé).
- **Conséquences** : les entreprises ne peuvent pas encore élargir l'accès par
  politique (elles le font par rôles) ; un nouveau connecteur déclare l'`Enforcement`
  de chacune de ses permissions ; les attributs des collaborateurs utilisés par les
  conditions (`subject.attributes.*`) se modifient depuis l'administration
  (ADR-023).

## ADR-023 — Attributs des collaborateurs : posés par l'administration, typés et courts

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : les conditions des politiques lisent `subject.attributes.<nom>`
  (service, site, type de contrat…). Jusqu'ici, seul le seed posait ces attributs.
  Changer un attribut change les personnes que vise une politique, comme changer
  l'habilitation.
- **Décision** :
  - un administrateur des comptes (`user:manage`, sur toute l'entreprise) remplace
    l'ensemble des attributs d'un compte (`PATCH /admin/users/{id}`,
    `attributes`), jamais sur son propre compte : il pourrait lever un refus qui le
    vise ;
  - règles (`core::collaborator_attributes`) :
    - 20 attributs au plus ;
    - nom : une minuscule puis des lettres ou des chiffres, 40 caractères au plus,
      lisible dans un chemin à points ;
    - valeur : un texte d'une ligne visible (200 caractères au plus, sans caractère
      de contrôle, de mise en forme invisible comme l'espace sans largeur ou le
      forçage de sens, ni séparateur de ligne), un nombre (entier exact dans un
      navigateur, ±2^53−1), oui/non, ou une liste de 1 à 20 textes ;
  - le changement passe sous le verrou d'administration (ADR-020). L'audit nomme
    les attributs ajoutés, retirés ou modifiés (`attributesChanged`), **jamais leurs
    valeurs** : ce sont des données personnelles, et le journal, en ajout seul et
    conservé longtemps, garde la trace sans le contenu ;
  - le changement s'applique à la requête suivante : le sujet est relu à chaque
    requête. Un run déjà lancé garde le sujet calculé à son démarrage (SECURITY.md
    §5) ;
  - les valeurs ne sont montrées qu'aux administrateurs des comptes (`user:manage`
    sur toute l'entreprise). L'éditeur de politiques propose les noms en usage dans
    l'entreprise, sans les valeurs ;
  - l'interface type chaque valeur (texte, nombre, oui/non, liste, un élément par
    ligne). Des attributs écrits hors de l'interface sous une autre forme ne s'y
    modifient pas, pour ne pas les altérer.
- **Conséquences** : les attributs font partie du compte. Ils suivent sa rétention,
  son export et sa suppression. Le **nom** d'un attribut, lui, reste dans l'audit et
  dans l'éditeur de politiques : l'interface rappelle de ne placer de donnée sensible
  ni dans la valeur ni dans le nom (un nom comme « enArretMaladie » en est une). Les
  textes sont enregistrés en forme canonique (NFC) et sans caractère ignoré à
  l'affichage (`Default_Ignorable_Code_Point`, dont les jointeurs sans chasse). Les
  valeurs texte des politiques suivent la même règle (`core::plain_text`) : une valeur
  collée en forme décomposée correspond toujours à l'attribut.

## ADR-024 — Modèle par entreprise et magasin de secrets chiffrés

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : jusqu'ici, toutes les entreprises utilisaient le fournisseur de modèle
  du serveur (`LLM_PROVIDER`). Or une entreprise doit pouvoir choisir son fournisseur,
  avec sa propre clé : c'est elle qui décide où partent ses données (sous-traitant,
  région) et qui paie sa consommation. Une clé d'API ne peut pas être stockée en
  clair.
- **Décision** :
  - **magasin de secrets** (`crates/api/src/secrets`) :
    - chiffrement AES-256-GCM (`ring`) sous une clé maîtresse fournie par
      l'environnement (`MIMORYA_MASTER_KEY`, 32 octets en base64), avec un nonce
      aléatoire par écriture ;
    - l'entreprise et le nom du secret servent de données associées : une valeur
      copiée vers une autre entreprise ou un autre nom ne s'ouvre pas ;
    - la table `secrets` (déjà prévue) ne reçoit que le texte chiffré, le nonce, le
      tag et la version de clé ;
    - sans clé maîtresse, aucun secret ne peut être enregistré (503 explicite) ;
  - **choix du fournisseur** (`tenant_llm_settings`, migration 0008, sous RLS) :
    fournisseur, modèle et adresse. La clé est le secret `llm.api_key`. La
    modification exige `settings:manage` sur toute l'entreprise et passe sous le
    verrou d'administration ;
  - **adresses** :
    - Anthropic, OpenAI, OpenRouter et Gemini s'utilisent à leur adresse officielle,
      sans adresse à saisir ;
    - Ollama et « compatible OpenAI » ne s'utilisent qu'à une adresse listée par
      l'exploitant (`MIMORYA_TENANT_LLM_ENDPOINTS`). La liste est commune : chaque
      adresse est visible et utilisable par toutes les entreprises (une liste par
      entreprise viendra si un serveur doit être réservé à l'une d'elles) ;
    - une entreprise ne fait donc jamais appeler par le serveur une adresse de son
      choix (réseau interne de l'exploitant) ;
    - l'adresse officielle est écrite dans la configuration construite, et la clé
      est toujours celle de l'entreprise : rien ne vient de l'environnement du
      serveur (`ANTHROPIC_BASE_URL`, clé de la plateforme) ;
  - **clé** :
    - jamais renvoyée par l'API ni écrite dans l'audit ou les journaux ;
    - elle ne sert qu'au fournisseur et à l'adresse pour lesquels elle a été saisie :
      changer de fournisseur ou d'adresse exige de la saisir de nouveau, sinon
      l'ancienne est effacée ;
  - **résolution** (`LlmService`, mise en cache 60 s par entreprise et invalidée à
    chaque changement) :
    - sans configuration, les runs utilisent le fournisseur du serveur ;
    - avec une configuration, ils n'utilisent qu'elle. Elle est revérifiée à chaque
      chargement avec les règles du moment : une adresse que l'exploitant retire de
      la liste n'est plus jamais appelée ;
    - si elle devient inutilisable (adresse retirée, clé illisible après un
      changement de clé maîtresse, clé absente), les runs s'arrêtent : **pas de
      repli vers la plateforme**, qui enverrait les données ailleurs que là où
      l'entreprise l'a décidé. La raison est montrée aux administrateurs, et le refus
      est mis en cache comme le reste ; une panne passagère du serveur n'est pas
      présentée comme un refus ;
  - un **test de connexion** envoie une courte requête au fournisseur résolu ; sa
    consommation est enregistrée ;
  - **audit** : `llm.configured` (fournisseur, modèle, adresse, clé remplacée ou non,
    état précédent), `llm.reset` et `llm.tested`, sans clé.
- **Conséquences** :
  - perdre la clé maîtresse rend illisibles les clés enregistrées : les entreprises
    concernées doivent les saisir de nouveau ;
  - la rotation de la clé maîtresse (`key_version`) et un KMS restent à faire
    (phase 3) ;
  - avec plusieurs instances de l'API, un changement met jusqu'à 60 s à atteindre
    les autres ;
  - un run déjà lancé garde le fournisseur résolu à son démarrage (ou à sa reprise
    après une validation) jusqu'à sa fin ou sa prochaine pause, dans la limite de son
    budget ;
  - la liste d'adresses de l'exploitant est lue au démarrage : en retirer une prend
    effet au redémarrage suivant.

## ADR-025 — Serveurs MCP ajoutés par les entreprises : HTTPS public, jeton chiffré

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : jusqu'ici, un serveur MCP ne s'ajoutait qu'en écrivant en base. Une
  entreprise doit pouvoir brancher ses propres serveurs (documentation, outils
  métier) et décider de ce que l'agent peut en faire. Or une adresse choisie par une
  entreprise est appelée par le serveur de la plateforme : sans précaution, elle mène
  au réseau interne de l'exploitant (SSRF). Le transport stdio, lui, lance un
  processus sur ce serveur. Enfin, le client HTTP de `rmcp` n'avait pas de TLS dans
  notre configuration.
- **Décision** :
  - depuis l'administration (`connector:manage`, sur toute l'entreprise, sous le
    verrou d'administration), une entreprise ajoute, modifie, active, désactive,
    teste et supprime des serveurs MCP **Streamable HTTP**. Le connecteur de
    démonstration et les serveurs stdio de l'exploitant ne changent ici que de nom et
    d'activation, et ne se suppriment pas d'ici ;
  - **adresses** : `https://` vers une adresse publique, sans identifiants, paramètres
    ni fragment dans l'URL. Le client HTTP des connecteurs (`connectors::mcp::network`) :
    - résout les noms en adresses publiques seulement, au moment de la connexion (un
      nom ne peut pas être redirigé vers le réseau interne après la vérification) ;
    - refuse les adresses privées écrites en clair ;
    - ne suit aucune redirection et n'utilise pas le mandataire de l'environnement ;
    - porte une configuration TLS explicite (rustls, `ring`, racines webpki) ;
  - un déploiement privé peut autoriser son réseau et le `http` simple
    (`MIMORYA_MCP_ALLOW_PRIVATE_NETWORK`). Le transport stdio n'est démarré que si
    l'exploitant l'autorise (`MIMORYA_MCP_ALLOW_STDIO`) ;
  - **jeton** : envoyé en `Authorization: Bearer`, scellé dans le magasin de secrets
    (`connector.<id>.token`, ADR-024), jamais renvoyé ni audité. Il ne suit pas le
    connecteur vers une autre adresse, et une configuration est toujours lue avec
    son jeton dans un même instantané. Un serveur peut recopier l'en-tête dans ses
    erreurs : le connecteur masque le jeton dans tout ce que le serveur renvoie
    (déclarations d'outils, erreurs de démarrage et d'appel, résultats d'outils),
    avant toute troncature et avant que cela n'atteigne
    les journaux, l'administrateur ou le modèle. Les journaux de la bibliothèque
    `rmcp`, qui recopient les réponses des serveurs, sont coupés par défaut
    (`rmcp=off`, sauf si `LOG_LEVEL` la nomme) ;
  - **politiques par outil** : risque, activation et permissions requises (permissions
    du catalogue délégables à l'agent). `connector:use` est toujours exigé. Les
    politiques d'un connecteur ne portent que sur ses propres outils
    (`mcp_<préfixe>_…`), et le préfixe est définitif ; deux serveurs n'ont jamais le
    même espace de noms ni l'un inclus dans l'autre (serveurs de l'exploitant
    compris) ;
  - **robustesse** : une panne d'un connecteur ne le rend indisponible que lui, et ne
    fait jamais échouer les runs de l'entreprise :
    - connexion et découverte des outils bornées à 20 s
      (`MIMORYA_MCP_START_TIMEOUT_SECS`) ; déclarations lues page par page jusqu'à
      200 outils au plus, noms de 128 caractères au plus, schémas d'entrée de
      32 Kio au plus et 256 Kio de déclarations par serveur, titres et
      descriptions bornés ;
    - les connecteurs démarrent en parallèle, et un échec est mémorisé 60 s pour la
      configuration en cours : un serveur muet ne retarde que les runs lancés
      pendant une tentative de démarrage (une par minute), du délai de démarrage
      au plus ; entre deux tentatives, les runs s'en passent sans attendre. Un
      changement de configuration ou un test réussi fait réessayer aussitôt ;
    - deux outils d'un même serveur qui donnent le même identifiant, un nom d'outil
      qui recopie le jeton, des politiques illisibles, un jeton illisible : ce
      connecteur est écarté ;
    - deux connecteurs qui nomment le même outil : le plus ancien le garde, l'autre
      est écarté ;
  - **état affiché** : `untested` (« Non testé ») tant qu'aucun démarrage n'a eu lieu
    depuis la création, un changement d'adresse ou de jeton, ou la réactivation ;
    puis `ok` ou `error` avec la dernière erreur, en français (cause technique
    abrégée, jeton masqué). Un état calculé sur une configuration antérieure est
    ignoré ;
  - **test de connexion** : il démarre une instance neuve, liste les outils avec la
    politique effective de chacun (celle que les runs appliquent) et enregistre
    l'état.
- **Conséquences** :
  - les serveurs MCP d'une entreprise doivent être joignables publiquement en HTTPS
    (ou sur le réseau d'un déploiement privé qui l'autorise) ;
  - la taille des réponses d'un serveur n'est pas bornée par la bibliothèque MCP
    (corps lus en entier, dans la limite du délai de démarrage ou d'appel) ;
  - la vivacité d'une session MCP gardée en cache n'est pas vérifiée : la plateforme
    ne la reconstruit qu'au changement de configuration suivant ou au redémarrage ;
  - restent à faire : OAuth des serveurs MCP, test avant enregistrement, mandataire
    sortant configurable, descriptions remplaçables par l'administrateur.

## ADR-026 — Jeton MCP : alphabet RFC 6750 et graphies masquées (complète ADR-025)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : ADR-025 masque le jeton d'un serveur MCP dans tout ce que le serveur
  renvoie, mais seulement écrit tel quel. Or un serveur peut recopier l'en-tête
  `Authorization` dans un corps JSON où un encodeur réécrit certains caractères
  (`\/` en PHP, `+` avec System.Text.Json) : le jeton passait alors presque en
  clair vers la dernière erreur, les journaux et le contexte du modèle. Un jeton peut
  aussi contenir `"` ou `\`, que JSON échappe toujours. Enfin, un serveur qui renvoie
  des pages vides sans fin faisait tourner la lecture des outils jusqu'au délai.
- **Décision** :
  - **alphabet** : un jeton suit le `b64token` de la RFC 6750 (lettres sans accent,
    chiffres, `- . _ ~ + /`, puis éventuellement des `=`). La règle
    (`connectors::mcp::token_problem`) vaut à l'enregistrement et à chaque démarrage :
    un jeton enregistré avant elle et hors alphabet rend le connecteur indisponible,
    avec le message « saisissez un jeton conforme » ;
  - **graphies masquées** : le jeton est masqué tel quel, échappé en JSON (`\/`,
    `\"`, `\\`, `\uXXXX` en majuscules ou minuscules) ou encodé dans une URL (`%XX`),
    pour `/ + = " \`, chaque caractère écrit d'une même façon dans tout le jeton. Les
    entités HTML, le double échappement et les graphies qui mêlent plusieurs formes
    d'un même caractère ne sont pas couverts ;
  - **lecture des outils** : 50 pages au plus, pages vides comprises, en plus de la
    limite de 200 outils.
- **Conséquences** :
  - le masquage parcourt chaque texte une fois tant que le plus long segment du jeton
    sans caractère échappé n'y figure pas, puis une fois par graphie (150 au plus pour
    un jeton de l'alphabet) ;
  - un jeton hors alphabet doit être saisi de nouveau.

## ADR-027 — Invitation des nouveaux comptes par lien à usage unique (complète ADR-021)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : un nouveau compte recevait un mot de passe provisoire que
  l'administrateur devait transmettre lui-même, puis que la personne devait remplacer.
  Ce mot de passe transitait par un canal choisi au hasard (messagerie, papier) et
  restait utilisable tant qu'il n'était pas remplacé.
- **Décision** :
  - un compte créé par l'administration est `invited`, sans mot de passe : il ne peut
    pas se connecter. Il reçoit un **lien d'invitation** à usage unique
    (`<WEB_ORIGIN>/invitation#<jeton>`, jeton aléatoire de 256 bits dont seule
    l'empreinte SHA-256 est conservée, table `user_invitations`), valable 7 jours ;
  - le jeton voyage dans le fragment de l'adresse (jamais envoyé avec la page ni dans
    `Referer`) puis dans le corps des requêtes `POST /auth/invitation` (ce que le lien
    ouvre : nom, adresse, entreprise, échéance) et `POST /auth/invitation/accept`
    (mot de passe choisi, 12 caractères au moins) ; ces routes sans identité sont
    limitées par client comme la connexion. La lecture avant connexion passe par la
    seule fonction `auth_find_invitation` (SECURITY DEFINER) ;
  - l'acceptation supprime l'invitation (une acceptation concurrente ne trouve plus
    rien), active le compte avec son mot de passe et ouvre une session ; un lien
    utilisé, expiré, remplacé, inconnu, ou celui d'un compte désactivé ou d'une
    entreprise suspendue reçoit la même réponse ;
  - l'administrateur (`user:manage` sur toute l'entreprise) peut renvoyer une
    invitation à un compte qui n'a pas encore choisi son mot de passe : le nouveau lien
    remplace l'ancien. Désactiver le compte supprime son invitation ; réactivé, il
    redevient `invited`. Un compte invité ne se réinitialise pas (on lui renvoie
    l'invitation), et la réinitialisation d'un compte actif reste un mot de passe
    provisoire (ADR-021) ;
  - **envoi** : si l'exploitant configure un relais (`SMTP_URL`, `MAIL_FROM` ; TLS par
    rustls, identifiants seulement sur `smtps://` ou `?tls=required`, jamais
    affichés), le lien part par e-mail en texte brut et n'est pas montré à
    l'administrateur. Sans relais, ou si l'envoi échoue, le lien lui est montré une
    seule fois (`Cache-Control: no-store`) avec la raison, pour qu'il le transmette ;
  - l'audit trace `user.invited` (échéance, mode d'envoi), `user.invitation_not_sent`
    et `user.invitation_accepted`, jamais le lien ni le jeton ; le journal
    d'exploitation ne retient pas la réponse du relais (elle peut nommer le
    destinataire).
- **Conséquences** :
  - migration `0010` ; dépendance `lettre` (SMTP) ;
  - un compte créé avant cette migration garde le parcours du mot de passe provisoire ;
  - la réinitialisation d'un mot de passe par e-mail (lien plutôt que mot de passe
    provisoire) et l'invitation dans la langue du destinataire restent à faire.

## ADR-028 — Réinitialisation du mot de passe par lien envoyé par e-mail (complète ADR-021 et ADR-027)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : un compte actif qui avait oublié son mot de passe dépendait d'un
  administrateur, qui lui transmettait un mot de passe provisoire par le canal de son
  choix et pouvait, ce faisant, se connecter à sa place.
- **Décision** :
  - depuis la page de connexion (« Mot de passe oublié ? »), une personne demande un
    **lien de réinitialisation** pour son adresse (`POST /auth/password-reset`). La
    réponse est toujours la même (202) et le lien est préparé en arrière-plan : ni le
    contenu ni la durée de la réponse ne révèlent si l'adresse correspond à un compte.
    Seul un compte `active` qui a déjà un mot de passe, dans une entreprise active,
    reçoit un lien ; un compte invité se sert de son invitation ;
  - lien à usage unique `<WEB_ORIGIN>/reset-password#<jeton>` (jeton aléatoire de
    256 bits dont seule l'empreinte SHA-256 est conservée, table `password_resets`, un
    lien en cours au plus par compte : une nouvelle demande remplace l'ancien), valable
    **une heure**. Il part **seulement par e-mail** : sans relais SMTP, la demande
    répond 503 et renvoie vers l'administrateur. Le lien n'est jamais montré,
    journalisé ni audité ;
  - le jeton voyage dans le fragment de l'adresse puis dans le corps de
    `POST /auth/password-reset/link` (adresse du compte, échéance) et
    `POST /auth/password-reset/complete` (nouveau mot de passe, 12 caractères au
    moins). L'usage verrouille la ligne du compte, supprime le lien (avec ce jeton, non
    expiré à l'instant de l'instruction), pose le mot de passe (plus d'obligation de le
    changer), **ferme toutes les sessions** du compte et en ouvre une. La lecture avant
    connexion passe par la seule fonction `auth_find_password_reset` (SECURITY
    DEFINER) ; un lien utilisé, expiré, remplacé, inconnu, celui d'un compte qui n'est
    plus actif ou d'une entreprise suspendue reçoit la même réponse ;
  - un lien cesse de fonctionner dès que le mot de passe change par un autre chemin
    (changement par la personne, réinitialisation par un administrateur) ou que le
    compte n'est plus actif : un déclencheur sur `users` le supprime, quel que soit le
    code à l'origine du changement ; une modification du profil ne le touche pas ;
  - limites : les trois routes sont limitées par client comme la connexion, et la
    demande à **3 par heure et par adresse** e-mail, connue ou non (une boîte ne peut
    pas être inondée de liens) ; les liens expirés sont purgés chaque minute par la
    maintenance périodique (`system_purge_expired_password_resets`) ;
  - audit : `user.password_reset_requested` (échéance), `user.password_reset_not_sent`
    et `user.password_reset_completed`, jamais le jeton ; le journal d'exploitation ne
    retient ni l'adresse ni la réponse du relais.
- **Conséquences** :
  - migration `0011` ; le travail laissé en arrière-plan par une requête (liens,
    audit des connexions refusées) est attendu à l'arrêt du serveur ;
  - qui contrôle la boîte aux lettres d'un compte peut en prendre la main : c'est le
    modèle habituel de la réinitialisation par e-mail, que la MFA (phase 2) renforcera ;
  - la réinitialisation par un administrateur reste un mot de passe provisoire
    (ADR-021) ; lui faire envoyer un lien à la place reste à faire.

## ADR-029 — Lien de réinitialisation envoyé par l'administrateur (complète ADR-021 et ADR-028)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : pour réinitialiser le mot de passe d'un compte, l'administrateur ne
  disposait que du mot de passe provisoire (ADR-021) : il le voit, le transmet par le
  canal de son choix et peut s'en servir pour se connecter à la place de la personne.
  Le lien de réinitialisation (ADR-028) n'était accessible que depuis la page de
  connexion.
- **Décision** :
  - quand un relais SMTP est configuré, l'administrateur (`user:manage` sur toute
    l'entreprise, jamais pour son propre compte) peut **envoyer un lien de
    réinitialisation** à un compte actif qui a un mot de passe
    (`POST /admin/users/{id}/password-link`). C'est le même lien qu'à l'ADR-028 (même
    table, même usage unique, même fermeture des sessions à l'usage), valable
    **24 heures** pour laisser à la personne le temps de lire son courrier. Le message
    nomme l'administrateur. Le lien n'est **jamais montré à l'administrateur** ;
  - le mot de passe actuel et les sessions restent valables jusqu'à l'usage du lien :
    pour couper l'accès tout de suite, l'administrateur désactive le compte ou donne
    un mot de passe provisoire, qui reste disponible ;
  - sans relais, la route répond 503 et l'interface ne propose que le mot de passe
    provisoire. Si l'envoi échoue, le lien est supprimé aussitôt (personne ne l'a
    reçu) et l'administrateur reçoit la raison. Un compte invité se voit renvoyer son
    invitation ; un compte désactivé est refusé (409) ;
  - audit : `user.password_reset_link_sent` (échéance) au nom de l'administrateur,
    `user.password_reset_not_sent`, puis `user.password_reset_completed` au nom du
    compte ;
  - compléments à l'ADR-028 :
    - une demande faite depuis la page de connexion est anonyme : son audit est inscrit
      au nom de la plateforme (`system`), pas du titulaire du compte ;
    - le statut de l'entreprise est relu dans la même lecture que le compte, que celle-ci
      verrouille ;
    - la page de connexion ne propose « Mot de passe oublié ? » que si les e-mails
      partent (`GET /auth/options`, sans identité) ;
    - **contrepartie assumée** de la limite par adresse : n'importe qui peut épuiser
      les 3 demandes par heure d'une adresse, et la vraie demande de la personne est
      alors refusée (429) jusqu'à la fin de la fenêtre. C'est le prix d'une limite qui
      ne distingue pas les adresses connues ; l'administrateur peut toujours envoyer un
      lien.
- **Conséquences** : pas de migration. Le mot de passe provisoire reste la voie sans
  relais, ou quand la personne n'a pas accès à sa messagerie.

## ADR-030 — En-têtes de sécurité : CSP à nonce par page, réponses de l'API durcies

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : l'interface affiche des contenus venus d'ailleurs (réponses du modèle
  en Markdown, résultats d'outils, données des connecteurs). React échappe le texte,
  mais une politique de sécurité du contenu (CSP) borne ce qu'une injection pourrait
  faire, y compris faire fuiter des données par l'adresse d'une image. Les réponses de
  l'API, qui portent des données personnelles, n'avaient pas d'en-têtes de cache ni de
  durcissement communs.
- **Décision** :
  - **pages web** : un `proxy.ts` (Next.js 16) donne à chaque page une CSP avec un
    **nonce aléatoire de 128 bits, nouveau à chaque requête** ; Next.js le pose sur ses
    propres scripts et styles. Politique : scripts de l'origine portant le nonce
    (`'strict-dynamic'`), styles de l'origine portant le nonce, images de l'origine (et
    `data:`, `blob:`) seulement, connexions vers l'origine seulement, ni `object`, ni
    `base`, ni cadre (`frame-ancestors 'none'`), formulaires vers l'origine. En
    développement s'y ajoutent `'unsafe-eval'` (débogage de React) et les styles en
    ligne (surcouche du serveur de développement). Le nonce imposant un rendu à chaque
    requête, la mise en page racine attend la requête (`connection()`) : plus aucune
    page n'est construite à l'avance. Les pages derrière une session ne changent pas ;
    les pages publiques (connexion, mot de passe oublié, invitation, réinitialisation),
    légères, sont désormais rendues à chaque requête ;
  - les pages d'erreur de Next.js se mettent en forme par des styles en ligne, que la
    CSP refuse : l'application a ses propres pages « introuvable » (`not-found.tsx`)
    et « erreur » (`global-error.tsx`), en français, mises en forme par des classes ;
  - en complément, toutes les pages gardent `X-Content-Type-Options`,
    `Referrer-Policy`, `X-Frame-Options`, `Permissions-Policy` et reçoivent
    `Cross-Origin-Opener-Policy: same-origin` ;
  - **API** : une couche ajoute à **toutes** les réponses, refus compris (CSRF, limite
    de débit, 404) : `Content-Security-Policy: default-src 'none'; frame-ancestors
'none'`, `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`,
    `Referrer-Policy: no-referrer` et `Cache-Control: no-store`, sauf si la route a
    choisi sa propre politique de cache (flux d'événements) ;
  - **HSTS** : posé par le reverse proxy qui termine TLS, pas par l'application (elle
    ne sait pas si elle est servie en HTTPS). SECURITY.md §10 donne la valeur attendue.
- **Conséquences** :
  - une image distante dans une réponse du modèle n'est pas chargée (refus de la CSP) ;
  - un script tiers ou un style en ligne ajouté plus tard devra porter le nonce, ou la
    politique être revue par une nouvelle entrée ;
  - la CSP des pages est vérifiée dans un navigateur (aucune violation sur les pages de
    l'application ni sur une adresse inconnue, image étrangère refusée) mais pas encore
    par un test automatisé : les tests de bout en bout en CI devront vérifier le nonce
    de l'en-tête et des scripts, et l'absence de violation sur la connexion et sur une
    adresse inconnue. Les en-têtes de l'API sont testés.

## ADR-031 — Tests de bout en bout : Playwright contre l'API à modèle scénarisé

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : le parcours web complet (A13) et la CSP des pages (ADR-030) n'étaient
  vérifiés qu'à la main, dans un navigateur piloté au coup par coup ; une régression de
  l'interface ou de sa politique de sécurité pouvait passer la CI.
- **Décision** :
  - les tests de bout en bout vivent dans `apps/web/e2e/` (un fichier par parcours,
    aides dans `support/`) et tournent avec `@playwright/test` (Apache-2.0, dépendance
    de développement de l'unique paquet Node, version alignée sur le navigateur
    installé) dans Chromium ;
  - ils pilotent **la vraie API et la vraie base** : l'exemple Cargo
    `crates/api/examples/e2e_server` démarre le serveur avec le **modèle scénarisé**
    (le scénario de référence : comptes rendus, proposition de planning, validation,
    synthèse) et écrit chaque e-mail dans un dossier (`E2E_MAIL_DIR`) au lieu de
    l'envoyer. C'est un exemple, pas un binaire : il n'entre dans aucune version
    publiée ;
  - la configuration (`playwright.config.ts`) recrée l'entreprise de démonstration
    (`mimorya-seed --reset`), démarre l'API puis l'application web construite pour la
    production (`next start`), et exécute les tests un par un (ils partagent
    l'entreprise et le script du modèle) ;
  - chaque test échoue si une page de l'un de ses contextes viole la CSP ou lève une
    erreur (fixture automatique ; les contextes supplémentaires viennent d'une fixture
    qui les surveille, pour qu'aucun ne soit oublié) ; les requêtes vers un hôte réservé
    (`exemple.invalid`) sont refusées exprès ;
  - parcours couverts : scénario de référence (demande, suivi en direct jusqu'à la
    validation, validation, synthèse, exécution dans les opérations) ; invitation
    reçue par e-mail et activation ; mot de passe oublié et lien à usage unique ; lien
    de réinitialisation envoyé par l'administrateur, sans qu'il le voie ; chargement de
    tous les écrans et onglets d'administration sans message d'erreur ; CSP (nonce de l'en-tête et des
    scripts, renouvelé à chaque page ; page introuvable ; image étrangère refusée ;
    en-têtes de l'API à travers le relais web) ;
  - un job de CI (`End-to-end (Playwright)`) les exécute à chaque PR sur une base
    PostgreSQL neuve et conserve les traces des tests en échec.
- **Conséquences** :
  - en local, `pnpm test:e2e` recrée l'entreprise de démonstration de la base désignée
    par `E2E_DATABASE_URL` (par défaut celle du développement) et occupe les ports 3000
    et 4000 ;
  - une passe par démarrage, sans nouvel essai (`retries: 0`) : le modèle scénarisé
    consomme ses réponses dans l'ordre (un scénario en file pour l'unique test qui
    converse ; un test de conversation de plus devra ajouter le sien dans
    `e2e_server`) et les tests modifient l'entreprise de démonstration, recréée à chaque
    démarrage ; l'API supprime au démarrage les e-mails d'une exécution précédente ;
  - le parcours avec un modèle réel reste à valider (A11).
