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
    (`apps/web/lib/api/types.ts`) ; leur génération depuis le serveur est à prévoir
    (commencée par l'ADR-036) ;
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
    redevient `invited` (s'il n'a jamais été actif : ADR-064). Un compte invité ne se réinitialise pas (on lui renvoie
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
    publiée. Comme l'API, une fois à l'écoute, il prend le verrou d'exécution des runs
    et reprend ceux qu'un arrêt a laissés, dont un qu'il laisse lui-même (ADR-053) ;
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

## ADR-032 — Images Docker et pile de démonstration derrière Caddy

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : l'installation privée (PRODUCT_VISION) suppose des images prêtes à
  lancer et un reverse proxy qui termine TLS et pose HSTS (ADR-030). Le dépôt n'avait que
  la base de développement en conteneur.
- **Décision** :
  - `infra/docker/api.Dockerfile` : compilation en mode release (`--locked`) de
    `mimorya-api` et `mimorya-seed`, image d'exécution Debian minimale avec les seuls
    certificats racine, utilisateur sans privilège ;
  - `infra/docker/web.Dockerfile` : construction de l'interface en sortie autonome de
    Next.js (`NEXT_OUTPUT=standalone`, activée seulement pour l'image), exécutée par Node
    sous un utilisateur sans privilège. Le relais `/api/*` de Next.js est fixé à la
    construction (`API_URL`), comme le mode démonstration (`NEXT_PUBLIC_DEMO_MODE`) ;
  - `compose.demo.yml` (projet distinct de la base de développement) : base **sans
    port publié**, joignable des seuls conteneurs de la pile et sur son propre volume ;
    `seed` (migrations et entreprise fictive, puis arrêt) avec un `DEMO_PASSWORD`
    **obligatoire** : en production, le seed exige un mot de passe et refuse la valeur
    publique de développement (`demo-mimorya`), que `.env.example` ne fixe plus, pour
    qu'un `.env` copié du modèle ne la fournisse pas en silence ; API (`MIMORYA_ENV=production`, cookies `Secure`,
    `TRUST_PROXY=true`, fournisseur de modèle et relais SMTP repris de l'environnement),
    interface et **Caddy** (`infra/caddy/Caddyfile`) : HTTPS (certificat local par
    défaut, public pour un vrai domaine), `Strict-Transport-Security: max-age=63072000;
includeSubDomains`, `/api/*` envoyé directement à l'API (qui lit l'adresse du client
    en dernière entrée de `X-Forwarded-For`), le reste à l'interface : une seule origine
    pour le navigateur ;
  - seul Caddy publie des ports ; la base de développement (`docker-compose.yml`)
    n'écoute plus que sur `127.0.0.1` ;
  - images : code de l'application à root, exécution sous un utilisateur sans
    privilège qui n'écrit que dans le cache de Next.js ; aucun fichier `.env` dans le
    contexte de construction, à aucune profondeur ;
  - un job de CI construit les deux images, vérifie que seul le proxy publie des ports,
    démarre la pile et la vérifie à travers le proxy (`infra/docker/smoke-test.sh` :
    HSTS, CSP à nonce, API sous `/api`, connexion d'un compte de démonstration avec le
    `DEMO_PASSWORD` de la pile et cookie `Secure`).
- **Conséquences** :
  - c'est une **pile de démonstration** : elle ne s'installe qu'avec les données
    fictives, dont des comptes d'administration qui se connectent tous avec
    `DEMO_PASSWORD` et que la page de connexion liste ; exposée publiquement, elle donne
    la main à qui connaît ce mot de passe. Les migrations sans données de démonstration
    et la création d'une entreprise réelle sont venues ensuite (ADR-033) ;
  - les images ne sont pas publiées dans un registre ; la CI les construit sans cache à
    chaque PR (plusieurs minutes) ;
  - le relais `/api/*` intégré à Next.js reste celui du développement ; derrière Caddy,
    il n'est pas utilisé.

## ADR-033 — Commandes d'exploitation : migrations seules, création d'une entreprise réelle

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : jusqu'ici, seule la commande du jeu de démonstration (`mimorya-seed`)
  appliquait les migrations, et aucune entreprise réelle ne pouvait naître : pas
  d'inscription (phase 3), et l'administration n'agit que dans sa propre entreprise. Une
  installation privée (PRODUCT_VISION) a besoin des deux, sans données fictives.
- **Décision** : un binaire d'exploitation `mimorya-admin` (`crates/api/src/bin/admin/`),
  livré dans l'image de l'API, lancé avec l'environnement de l'API :
  - `migrate` applique les migrations avec `DATABASE_URL` (rôle propriétaire, comme le
    seed), sans aucune donnée ; relancée, elle ne change rien ;
  - `create-company --name --slug --admin-email --admin-name` crée, en une seule
    transaction (tout ou rien), l'entreprise (sigle unique, de 2 à 63 minuscules sans
    accent, chiffres et tirets : `mimorya_core::provisioning`), son unité racine (de type
    entreprise), les rôles de plateforme (`PLATFORM_ROLE_TEMPLATES`) et son premier
    administrateur : rôle de base sur ses propres données et administrateur technique sur
    toute l'entreprise, sans aucun accès métier. Rien d'autre (ni rôle métier, ni
    connecteur, ni mémoire, ni compétence) : l'administrateur construit la suite depuis
    l'administration ;
  - ce premier administrateur est **invité** (ADR-027) : aucun mot de passe ne passe par
    l'exploitant. Le lien, valable 7 jours, est affiché une seule fois sur la sortie de
    la commande, pour être transmis par un canal sûr ; il n'est ni conservé en clair,
    ni journalisé, ni audité. La commande se lance dans un terminal, pas comme une tâche
    dont un collecteur garde la sortie ;
  - `invite --email` donne un nouveau lien (le précédent cesse de fonctionner) à un
    compte qui n'a pas encore choisi son mot de passe : lien expiré ou perdu, sans autre
    administrateur pour le renvoyer. Elle refuse un compte actif ou désactivé, et un
    compte d'une entreprise suspendue ;
  - l'émission d'un lien (verrou du compte, statut « invité », jeton, échéance) est la
    même que celle des administrateurs (`services::invitations::renew_invitation`), et
    les limites d'un compte (adresse, nom affiché) sont partagées avec l'administration
    (`mimorya_core::accounts`) ;
  - `create-company` et `invite` écrivent avec `DATABASE_APP_URL` (rôle `mimorya_app`,
    RLS appliquée) : la transaction est ouverte sur la nouvelle entreprise, que la
    politique de `tenants` laisse écrire, et elle seule (`crates/db/tests/isolation.rs`
    tente d'en écrire une autre, ou hors de toute entreprise). La seule lecture entre
    entreprises est celle de la connexion (`auth_find_login`), pour trouver le compte
    d'une adresse ;
  - chaque création est auditée dans l'entreprise par l'acteur `system`
    (`tenant.created`, `user.created`, `role.assigned`, `user.invited` avec
    `delivery: "operator"`) ; l'invitation n'a pas d'auteur (`created_by_user_id` nul) ;
  - un job de CI exécute ces commandes dans l'image jusqu'à l'ouverture du lien à travers
    le proxy (sur la pile de production depuis l'ADR-034 :
    `infra/docker/production-test.sh`, qui relance aussi les migrations sur une base
    déjà migrée).
- **Conséquences** :
  - une installation privée démarre sans données fictives : migrations, puis entreprise
    et administrateur, puis l'API et l'interface (`NEXT_PUBLIC_DEMO_MODE` non activé) ;
  - aucun envoi d'e-mail par ces commandes : l'exploitant transmet le lien. Un
    administrateur qui a déjà choisi son mot de passe et l'oublie passe par « mot de
    passe oublié » (relais SMTP) ou par un autre administrateur de son entreprise ;
  - l'exploitant, qui détient les accès à la base, crée des entreprises sans autre
    contrôle : c'est le rôle de la plateforme, pas une fonction offerte aux entreprises.
    L'inscription en libre-service reste en phase 3 ;
  - la pile conteneurisée de production est venue ensuite (ADR-034) ; la pile de
    démonstration (ADR-032) garde ses rôles de base aux mots de passe de développement,
    acceptables seulement parce que la base n'y est jamais publiée.

## ADR-034 — Pile conteneurisée de production

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : la pile de démonstration (ADR-032) n'existe qu'avec les données fictives
  et des rôles de base aux mots de passe de développement ; les commandes d'exploitation
  (ADR-033) permettent désormais de démarrer sans données fictives. Il manquait la pile
  qui les assemble pour une installation privée.
- **Décision** : `compose.production.yml`, projet `mimorya-production` (distinct de
  celui de la base de développement, dont le volume et le conteneur portent sinon le
  même nom dans un clone nommé `mimorya`) :
  - `db` : PostgreSQL 16 + pgvector, sans port publié, volume propre. Les mots de passe
    du propriétaire du schéma (`POSTGRES_PASSWORD`) et du rôle applicatif
    (`MIMORYA_APP_DB_PASSWORD`) sont **obligatoires** et choisis à l'installation.
    Comme ils passent dans des URL de connexion, `infra/postgres/entrypoint-production.sh`
    exige des lettres et des chiffres seulement, 16 au moins (`openssl rand -hex 32`),
    **avant** le point d'entrée de l'image : un mot de passe refusé ne crée aucun
    répertoire de données (sinon, au redémarrage automatique, l'image sauterait
    l'initialisation et démarrerait sans rôle applicatif). Puis
    `infra/postgres/init-production.sh` crée `mimorya_app` (connexion, sans
    `BYPASSRLS`) et les extensions ; psql lit le mot de passe dans l'environnement
    (`\getenv`), jamais en argument visible dans la liste des processus ;
  - `migrate` : `mimorya-admin migrate` (rôle propriétaire), puis arrêt ; l'API ne
    démarre qu'après son succès. Chaque `up -d` la relance sur la base existante : c'est
    le chemin des mises à jour ;
  - `api` (production, `TRUST_PROXY`, rôle applicatif), `web` construite **sans** mode
    démonstration (image `mimorya-web:production` ; celle de la démonstration devient
    `mimorya-web:demo`) et Caddy ; `restart: unless-stopped` sur les services durables ;
  - le Caddyfile, commun aux deux piles, prend le mode TLS dans `MIMORYA_TLS` :
    `internal` (autorité locale de Caddy, défaut de la démonstration) ou une adresse
    e-mail (certificat public ACME). La pile de production l'exige, ainsi que
    `MIMORYA_DOMAIN` ;
  - la CI construit les images des deux piles et vérifie que seul le proxy publie des
    ports dans chacune. Elle teste la démonstration (dont la liste des comptes de
    démonstration sur la page de connexion, témoin de la vérification inverse) et
    l'arrête. Elle lit le Caddyfile en mode ACME (`caddy adapt`), puis prouve qu'un mot
    de passe refusé fait échouer la base sans rien laisser dans son volume. Enfin, elle
    installe la production avec des mots de passe tirés au hasard (masqués dans les
    journaux) et la vérifie à travers le proxy (`infra/docker/production-test.sh`) :
    en-têtes et API ; conteneurs `api` et `web` hors root ; migrations relancées sans
    erreur ; aucune entreprise de démonstration en base et aucune liste de comptes de
    démonstration sur la page ; entreprise créée par `mimorya-admin` ; invitation
    acceptée avec un mot de passe tiré au hasard ; session `Secure` dans une entreprise
    qui n'est pas une démonstration. Ce test laisse son entreprise derrière lui : il
    exige `DISPOSABLE_STACK=yes`, réservé à une pile jetable. Les vérifications communes
    aux deux piles sont dans `infra/docker/checks.sh`.
- **Conséquences** :
  - une installation privée se fait en deux commandes (démarrage, création de la
    première entreprise), sans aucun identifiant connu d'avance ;
  - la base ne lit ses mots de passe qu'à sa création (les migrations et l'API s'en
    servent à chaque démarrage) : en changer ensuite demande de modifier le rôle
    (`\password` dans psql) avant la variable (README) ;
  - restent à faire : publication des images dans un registre et cache de leur
    construction en CI, sauvegardes chiffrées et restauration testée (phase 3) ;
  - les images de base ne sont pas épinglées par empreinte (`rust:1-bookworm`,
    `node:22-bookworm-slim`, `caddy:2.10-alpine`, `pgvector/pgvector:pg16`) : une
    reconstruction peut prendre une version corrective plus récente.

## ADR-035 — Droits d'un run relus à chaque étape

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : un run calculait les droits de son utilisateur à son démarrage et les
  gardait jusqu'à sa fin. Un compte désactivé, ou privé d'une habilitation, pendant un
  run pouvait donc encore agir par l'agent jusqu'à la fin du budget de durée (relevé
  par les audits, SECURITY.md §5).
- **Décision** :
  - nouveau port du moteur, `AccessSource` (`crates/agent/src/runtime/ports.rs`) : à
    chaque lecture, `StepAccess::Granted` (règles de l'entreprise et sujet actuels) ou
    `StepAccess::Ended(motif)`. `RuntimeDeps` le porte à la place du moteur
    d'autorisation figé, et `RunConfig` ne porte plus de sujet ;
  - la boucle le relit **avant chaque appel au modèle** (outils proposés), **avant les
    appels d'outils** que renvoie la réponse (elle a pu prendre du temps), **avant de
    conclure une réponse** (et donc avant l'appel éventuel au juge) et **à la reprise**
    après validation (les appels approuvés s'exécutent avec les droits de ce moment).
    Un run qui ne peut plus agir se termine en échec avec le motif, sans autre appel
    au modèle ni action ; à la reprise, chaque appel validé reçoit son issue (annonce
    de la décision, journal « non exécuté » avec le motif). Une lecture impossible
    (base injoignable) arrête aussi le run ;
  - l'API l'implémente avec le service d'accès des requêtes (`RunAccess`,
    `AccessService::active_context`) : règles de l'entreprise depuis son cache
    (invalidé par chaque changement d'administration), compte, habilitations et
    attributs depuis la base, à chaque lecture. Le run continue tant que le compte est
    actif **et** détient `agent:use`, comme au démarrage et à la reprise (« Arrêt : le
    compte de l'utilisateur n'est plus actif », « Arrêt : l'utilisateur n'a plus le
    droit d'utiliser l'assistant ») ;
  - les outils mémoire passent par le contexte d'autorisation lié de l'appel
    (`ctx.authz.memory_read_filter()`, `ctx.authz.can_write_memory()`), plus par une
    copie du moteur prise à la construction du registre ; le moteur reste privé à ce
    contexte (sujet et acteur liés) ;
  - tests : moteur (`crates/agent/tests/runtime_access.rs`, droits successifs simulés
    par `testing::TestAccess`) et API (`crates/api/tests/run_access.rs` : compte
    désactivé, droit `agent:use` retiré, rôles métier retirés, entreprise suspendue,
    pendant la réponse du modèle).
- **Conséquences** :
  - une à trois lectures en base par étape : quelques requêtes, négligeables devant un
    appel au modèle ;
  - ce que le run détient déjà reste dans sa conversation : consignes système (rôles,
    index des compétences) et mémoires rappelées au démarrage, résultats d'outils déjà
    obtenus ; un outil déjà en cours d'exécution n'est pas interrompu (délai borné) ;
  - restent figés pour la durée d'une exécution : le registre d'outils de l'entreprise
    (outils activés, permissions et risques des connecteurs) jusqu'à la pause ou la
    fin, et le niveau d'autonomie du run ;
  - les règles de l'entreprise modifiées directement en base, hors administration, ne
    comptent qu'à l'expiration du cache (30 s), comme pour les requêtes. Depuis, la
    suspension d'une entreprise arrête aussi ses runs en cours (« Arrêt : l'entreprise
    est suspendue ») : elle se fait en base (aucune route ni commande ne la fait encore),
    donc dans ce même délai de 30 s au plus.

## ADR-036 — Contrat de l'API typé, types de l'interface générés (complète ADR-018)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : les routes construisaient leurs réponses avec `json!` et l'interface
  en décrivait la forme à la main (`apps/web/lib/api/types.ts`, ADR-018). Rien ne
  garantissait que les deux concordent : un champ renommé ou retiré côté serveur
  cassait l'interface sans aucune erreur de compilation.
- **Décision** :
  - module `crates/api/src/contract/`, un fichier par ressource : chaque réponse est une
    structure typée (`Serialize`, champs en `camelCase`), construite depuis les lignes
    des dépôts ; les routes renvoient `Json<Type>` ;
  - les colonnes JSON écrites par la plateforme (consommation, budget et vérification
    d'un run, métadonnées d'un message) sont relues dans leur type du contrat, qui
    refuse les clés inconnues (`deny_unknown_fields`) : un écart, champ manquant ou en
    trop, est une erreur interne dont le message nomme la colonne sans citer la valeur
    (elle peut contenir un message). Les métadonnées d'un message sont écrites avec ce
    même type (`MessageMetadata`) ; des tests vérifient que les types du moteur
    (`RunUsageTotals`, `BudgetLimits`, `VerificationReport`, `Citation`) s'y relisent
    sans perte ;
  - `ts-rs` (licence MIT), dans la seule crate `api`, décrit ces types en TypeScript.
    `contract::typescript()` assemble un module unique, chaque type une fois, trié par
    nom : `apps/web/lib/api/contract.ts`, commité et exclu de Prettier. `pnpm contract`
    le régénère ; un test unitaire (`cargo test`, donc `pnpm check` et la CI) échoue
    s'il ne correspond plus au contrat ;
  - l'interface importe ces types depuis `@/lib/api` ; ce qui est propre au navigateur
    (formulaires, état local) reste déclaré côté web ;
  - entiers en `number` (valeurs très en deçà de 2^53), dates en chaînes ISO 8601,
    contenus libres (paramètres d'une validation, données d'un événement) en `unknown`.
- **Conséquences** :
  - première étape : session (`/auth/me`, `/auth/options`), conversations et messages,
    runs et leur trace, validations. Restent décrits à la main jusqu'à leur passage au
    contrat : administration, mémoire, compte, invitations et mot de passe oublié,
    événements du flux SSE, corps des requêtes ;
  - modifier une réponse impose de régénérer le fichier, et le typage de l'interface
    signale chaque usage à adapter ;
  - une valeur stockée hors contrat (jamais écrite par la plateforme) fait répondre la
    route en erreur 500 plutôt que par une réponse incomplète.

## ADR-037 — Contrat de l'API : colonnes texte et flux en direct (complète ADR-036)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : la première étape du contrat (ADR-036) laissait décrits à la main la
  mémoire, le compte, les pages de lien et les événements du flux en direct ; ceux-ci,
  écrits par le moteur (`RunEvent`), n'avaient aucun type côté interface.
- **Décision** :
  - les colonnes texte qui portent une valeur du contrat (rôle d'un message, portée,
    nature et statut d'une mémoire, décision sur un appel d'outil, risque d'une
    validation) sont relues par `stored_text` dans leur énumération : une valeur
    inconnue est une erreur interne qui ne la cite pas. Les contraintes `CHECK` de la
    base fixent déjà ces valeurs ; des tests parcourent leurs sources (`ALL` des
    énumérations du core, `ToolCallEvaluation::KINDS`, dont le moteur écrit aussi le
    journal des appels d'outils) ;
  - `RunStreamEvent` déclare chaque événement du flux (nom et données, union
    discriminée sur `type` en TypeScript). Un test vérifie que chaque événement écrit
    par le moteur, et le texte diffusé (`assistant_delta`, construit avec ce type), se
    relit dans son type sans perte ni clé inconnue ; ajouter une sorte d'événement au
    moteur ne compile plus tant que le test (`_covered`) ne la nomme pas, et son
    échantillon est à ajouter à `every_engine_event` ;
  - l'interface reçoit, pour chaque gestionnaire du flux, le type de données de son
    événement.
- **Conséquences** :
  - restent décrits à la main : l'administration, les corps des requêtes et
    l'enveloppe des erreurs ;
  - les données d'un événement relu depuis le journal d'un run (`RunDetail.events`)
    restent en `unknown` : leur forme est celle de `RunStreamEvent`.

## ADR-038 — Modifications d'administration refusées tracées avec leur raison

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : l'audit gardait les modifications d'administration faites, pas celles
  refusées : un administrateur qui tente de modifier son propre compte, un
  administrateur limité à une équipe, un membre sans rôle d'administration ou une
  tentative sur un rôle de la plateforme ne laissaient aucune trace. Les refus naissent
  à plusieurs endroits (vérification de la permission, permission sur toute
  l'entreprise, verrou de l'administration, règles propres à une ressource).
- **Décision** :
  - un refus (`DomainError` de code `Forbidden`) porte, dans les extensions de sa
    réponse, son message (`Refusal`) ; il n'est jamais envoyé, le message figurant déjà
    dans le corps ;
  - une couche posée sur les routes d'administration (`plugins/refusal_audit.rs`,
    `route_layer`, dernier appel du routeur) trace, pour une requête modifiante d'un
    utilisateur connecté dont la réponse porte un refus, une entrée `admin.refused`
    (`denied`) : l'utilisateur, la raison (le message du refus), la méthode et la route
    (`MatchedPath`, sans les valeurs), la ressource désignée par l'identifiant de la
    route, l'IP et l'identifiant de requête. La ressource est nommée par la collection
    de la route (`user`, `role`, `org_unit`, `policy`, `connector` ; une révocation
    refusée nomme l'habilitation, `assignment`, là où l'audit d'une révocation faite
    nomme le compte) ; seul un UUID est retenu, sans vérification : quand le refus
    précède toute lecture (permission absente, mot de passe temporaire à remplacer, compte
    inactif), il peut désigner une ressource absente
    ou d'une autre entreprise, et la réponse reste la même. L'entrée est écrite dans sa
    propre transaction, après celle de la requête, annulée, et avant l'envoi de la
    réponse ;
  - les messages de refus sont écrits par la plateforme (au plus le nom d'une
    permission) : hormis l'identifiant du chemin, rien de ce que la requête a envoyé
    n'atteint l'audit. Le refus d'un compte qui doit remplacer son mot de passe
    temporaire (extracteur `Auth`) est tracé comme les autres ;
  - un échec d'écriture de la trace est journalisé ; la réponse reste le refus.
- **Conséquences** :
  - ne sont pas tracés : les lectures refusées, les requêtes invalides ou anonymes, les
    refus anti-CSRF (en-tête `X-Mimorya-Request` absent, origine non autorisée), que
    prononce une couche placée avant la résolution de la session, et les ressources
    qu'un administrateur habilité trouve absentes ou hors de son périmètre, qui
    répondent 404 sans distinguer l'une de l'autre ; tracer les secondes demanderait de
    les distinguer côté serveur (feuille de route) ;
  - une route d'administration nouvelle est couverte d'office ; une collection nouvelle
    à identifiant est à nommer dans `resource_of`, sans quoi l'entrée n'a pas de
    ressource (la route reste dans les métadonnées) ;
  - le volume de ces entrées est borné par la limitation de débit globale.

## ADR-039 — Corps des requêtes au contrat, stricts (complète ADR-036)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : les corps des requêtes étaient décrits deux fois, par une structure
  privée de chaque route et par l'objet littéral que l'interface envoie. Un champ mal
  nommé côté interface (`job_title` pour `jobTitle`, `titre` pour `title`) était ignoré
  en silence par le serveur : la requête réussissait sans rien changer.
- **Décision** :
  - chaque corps de requête est un type de `crates/api/src/contract/`, dans le fichier
    de sa ressource (`Deserialize`, `Validate`, champs publics), décrit en TypeScript
    comme les réponses ; un champ facultatif (`Option`, `#[serde(default)]`) l'est aussi
    en TypeScript (`#[ts(optional)]`) ;
  - ces types refusent un champ inconnu (`deny_unknown_fields`) : la route répond 400
    au lieu d'ignorer le champ. Un test unitaire (`contract/tests/requests.rs`) lit
    chaque corps sous sa forme minimale et complète, et vérifie qu'il refuse un champ
    de plus ; un test d'intégration (`crates/api/tests/strict_bodies.rs`) vérifie la
    réponse 400, qui nomme le champ, sans rien changer ;
  - l'interface envoie ses corps avec `satisfies <Type>` : un champ inconnu, manquant
    ou mal typé est une erreur de compilation ;
  - `ts-rs` est compilé avec `no-serde-warnings` : les attributs propres à la lecture
    (`deserialize_with`) ne le concernent pas. La coupure vaut pour tout le contrat,
    réponses comprises : ts-rs ne signale plus un attribut qu'il ne lit pas. Un
    attribut qui change la forme d'une réponse (`serialize_with`, `into`,
    `transparent`) s'accompagne donc de `#[ts(type = …)]` ou `#[ts(as = …)]`, comme
    `#[serde(with)]`, que ts-rs refuse sans eux ; aucune réponse n'en utilise
    aujourd'hui ;
  - les paramètres de requête (`?status=`, `?limit=`) restent déclarés par leur route.
- **Conséquences** :
  - première étape : connexion, compte, liens d'invitation et de réinitialisation,
    conversations, validations, mémoire. Les corps des requêtes de l'administration
    suivent ;
  - un client qui envoyait un champ en trop reçoit désormais une erreur 400 ; les tests
    d'intégration et de bout en bout n'en envoient aucun.

## ADR-040 — Corps des requêtes de l'administration au contrat (complète ADR-039)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : après ADR-039, les corps des requêtes de l'administration restaient des
  structures privées des routes ; ceux des comptes, de l'organisation, des
  habilitations et des rôles ignoraient un champ inconnu. L'audit d'ADR-039 a aussi
  relevé que `satisfies` ne vérifie pas les clés d'une décomposition conditionnelle
  (`...(c ? { comment } : {})`) et que la liste des corps testés était tenue à part.
- **Décision** :
  - chaque corps de l'administration est un type strict du contrat, dans le fichier de
    sa ressource : `NewAccount`, `AccountUpdate`, `RoleGrant` (avec la portée,
    déplacée dans `contract/admin/assignments.rs`), `NewOrgUnit`, `OrgUnitUpdate`,
    `NewRole`, `RoleUpdate`, `NewPolicy`, `PolicyUpdate`, `NewConnector`,
    `ConnectorUpdate` (et `ToolPolicyChange` pour la politique d'un outil),
    `LlmChoice` ;
  - un corps n'emploie que des types du contrat (sensibilité, risque, portée), convertis
    vers ceux du core par la route ; une valeur que la route vérifie elle-même, avec son
    propre message, garde son type texte et est décrite en TypeScript par celui du
    contrat (`type` d'une unité : `OrgUnitType`) ; une condition ABAC reste
    `Record<string, unknown>`, comme dans `PolicyView` ;
  - une valeur par défaut autre que l'absence (compte `internal`, politique ou
    connecteur activé, rôle sans description) devient un champ facultatif que la route
    complète : le champ est facultatif en TypeScript aussi. `null` y vaut l'absence
    (il était refusé par 400 avant) ; l'interface n'en envoie pas, son type ne
    l'autorisant pas ;
  - la portée d'une habilitation est stricte pour chaque variante : celles sans données
    (`tenant`, `self`) s'écrivent `Tenant {}` et `Own {}`, car serde ignore ce qui
    accompagne l'étiquette d'une variante unitaire (`{"type": "tenant", "orgUnitId": …}`
    était accepté, l'unité ignorée) ; le JSON écrit et le type TypeScript sont
    inchangés ;
  - une seule liste, `request_bodies!` (`contract/mod.rs`), déclare chaque corps dans le
    module TypeScript et dans le test qui vérifie qu'il refuse un champ inconnu ;
  - l'interface écrit ses corps champ par champ (`champ: valeur || undefined`, omis par
    `JSON.stringify`), sans décomposition conditionnelle.
- **Conséquences** :
  - tous les corps de requêtes de l'API sont au contrat ; seuls les paramètres de
    requête restent déclarés par leur route ;
  - un champ mal nommé dans un corps de l'administration (comptes, organisation,
    habilitations, rôles) reçoit désormais une erreur 400 au lieu d'être ignoré
    (`crates/api/tests/strict_bodies.rs`) ;
  - un corps ajouté hors de `request_bodies!` n'est ni décrit en TypeScript, ni testé :
    l'interface ne peut pas l'importer, ce qui le signale.

## ADR-041 — Formulaires de modification : verrouillés pendant le rechargement de leur liste

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : un formulaire de modification copie la ressource dans son état à son
  ouverture. `useApiData` garde l'ancienne liste pendant un rechargement, sans le
  signaler. Une fiche de compte rouverte juste après un enregistrement partait donc de
  l'ancien compte, et l'enregistrer réécrivait les anciennes valeurs, jusqu'à tous les
  attributs (le PATCH les remplace). Reconstruire le formulaire par une clé React ne
  suffisait pas : il restait enregistrable avant l'arrivée de la liste, et la
  reconstruction perdait la requête en cours et le focus (audit de la PR #29).
- **Décision** :
  - `useApiData` expose `loading` : une requête est en cours, et `data`, s'il existe,
    est l'ancienne ;
  - pendant ce temps, le formulaire est verrouillé (`<fieldset disabled>`) : ni saisie
    ni enregistrement à partir d'une ressource peut-être périmée ;
  - chaque champ prend son état par `useSyncedState(valeur de la liste)`, repris quand
    cette valeur change, sans reconstruire le composant (requête en cours et message
    conservés) ;
  - appliqué d'abord à la fiche d'un compte (profil et attributs).
- **Conséquences** :
  - après un enregistrement, la fiche est brièvement désactivée, le temps du
    rechargement, et le contrôle qui avait le focus le perd (rendu par ADR-043) ;
  - les autres formulaires de modification (rôle, politique, unité, serveur MCP,
    politique d'outil, modèle) restent à traiter de la même façon (`docs/ROADMAP.md`) ;
  - test : `apps/web/e2e/reloads.spec.ts` (d'abord dans `administration.spec.ts`)
    retient l'annuaire, vérifie que la fiche rouverte n'est pas enregistrable, puis
    qu'elle montre ce que l'API a enregistré.

## ADR-042 — Tout écran chargé par l'API est verrouillé pendant son rechargement (complète ADR-041 et en remplace le verrou par formulaire)

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : ADR-041 verrouillait la seule fiche d'un compte. Les autres formulaires
  de modification (rôle, politique, unité, serveur MCP, modèle) se ferment à
  l'enregistrement, et la liste se recharge ensuite. Rouverts avant son arrivée, ils
  partaient de l'ancienne ressource et l'enregistraient. De même, un bouton
  d'activation ou de suppression agissait d'après l'ancienne liste. Enfin, `loading`
  ne passait à vrai qu'à l'effet suivant un changement d'adresse (audit de la PR #29).
- **Décision** :
  - `Loaded`, qui affiche les données de `useApiData`, place son contenu dans un
    `<fieldset disabled>` (`display: contents`, sans effet sur la mise en page)
    tant qu'une requête est en cours. Tous ses contrôles sont désactivés :
    formulaires, boutons d'ouverture, d'activation et de suppression. Cela vaut pour
    tous les écrans qui l'utilisent (administration, validations, mémoire) ;
  - `useApiData` compare la requête dont la réponse est affichée à la dernière
    demandée (version et adresse) : `loading` est vrai dès le rendu qui la demande ;
  - le verrou par formulaire d'ADR-041 et sa propriété `reloading` sont retirés ; un
    formulaire qui reste ouvert après un enregistrement (fiche d'un compte) suit
    toujours la liste par `useSyncedState` ;
  - « Annuler » est désactivé dans un formulaire de modification tant que sa
    modification n'a pas reçu de réponse : refermé puis rouvert pendant ce temps, il
    serait reparti de l'ancienne ressource ; le verrou prend le relais au
    rechargement ;
  - les règles des outils d'un serveur MCP repartent de chaque test de connexion ;
  - le verrou est signalé (`aria-busy`) ; un contrôle qu'il désactive garde son aspect
    150 ms avant de prendre son aspect désactivé, pour qu'un rechargement rapide ne
    fasse pas clignoter l'écran. C'est une animation CSS (`app/globals.css`), qui vaut
    aussi pour un contrôle affiché pendant le verrou, et ne touche pas un contrôle
    désactivé pour une autre raison (son propre attribut `disabled`). Exception : une
    case à cocher native prend aussitôt l'aspect désactivé du navigateur (levée par
    ADR-045).
- **Conséquences** :
  - après une modification, l'écran est brièvement désactivé, le temps du rechargement,
    et le contrôle qui avait le focus le perd (rendu par ADR-043) ;
  - dans les onglets où un seul formulaire est ouvert à la fois (rôles, politiques,
    organisation), « Modifier » sur une autre carte ferme encore le formulaire dont la
    modification attend sa réponse ; rouvert avant celle-ci, il repart de l'ancienne
    ressource. La fenêtre est celle de la requête : sa réponse referme le formulaire
    rouvert ;
  - une lecture qui ne répond pas laisse l'écran verrouillé jusqu'à son délai
    (ADR-044) ;
  - un écran nouveau qui affiche ses données par `Loaded` est protégé sans rien
    ajouter ;
  - test : `apps/web/e2e/reloads.spec.ts` retient la liste. Il vérifie que la fiche
    d'un compte ne se modifie ni ne se ferme, et qu'un rôle ne se rouvre pas, d'après
    l'ancienne liste, puis que la liste et le formulaire rouvert montrent ce que l'API
    a enregistré. Il vérifie aussi qu'une fiche restée ouverte reprend, au
    rechargement suivant, la modification faite entre-temps hors de la page ; que
    « Annuler » est désactivé pendant l'envoi ; que l'animation du verrou s'applique à
    une carte affichée pendant celui-ci et pas à un bouton désactivé par son propre
    attribut ; et que les règles des outils montrent le dernier test de connexion.

## ADR-043 — Le focus est rendu au contrôle qui l'a perdu en étant désactivé

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : un contrôle désactivé perd le focus, que le navigateur donne à la page
  (vérifié sous Chromium : par son propre attribut comme par un `fieldset`). C'est le
  cas d'un bouton pendant son action (`loading`) et de tout l'écran pendant le
  verrou d'un rechargement (ADR-042) : après chaque modification, plus aucun
  contrôle n'avait le focus. « Entrée » ou « Espace » n'agissaient plus et un
  lecteur d'écran perdait sa position ; Tab repartait du contrôle suivant, Chromium
  gardant le point de départ de la navigation au clavier.
- **Décision** :
  - un module unique, `apps/web/lib/focus-keeper.ts`, installé une fois pour toutes
    les pages (`FocusKeeper` dans la mise en page racine), retient le contrôle qui
    perd le focus sans destination (`focusout` sans `relatedTarget`) alors qu'il est
    désactivé ;
  - dès qu'il est réactivé (observation de l'attribut `disabled`, le sien ou celui
    d'un `fieldset`), il reprend le focus, sans faire défiler la page, si le focus est
    resté sur la page ;
  - un focus donné entre-temps à un autre élément (tabulation, clic sur un contrôle)
    annule ce rendu ; un clic sur le fond de la page, qui ne donne le focus à rien,
    ne l'annule pas ; un contrôle retiré de la page (formulaire fermé) n'est pas
    remplacé.
- **Conséquences** :
  - après « Entrée » dans un champ ou un clic sur un bouton suivi d'un rechargement,
    le focus revient au même contrôle s'il est toujours affiché ;
  - le formulaire qui se ferme à l'enregistrement laisse encore le focus à la page ;
  - le module vaut pour tout contrôle désactivé : la zone de saisie du chat,
    désactivée pendant tout un run, reprend le focus à sa fin si le focus est resté
    sur la page. Une validation décidée depuis la carte du chat donne le focus à son
    bouton, ce qui annule ce rendu ; décidée depuis la page des validations ouverte
    dans un autre onglet, elle le laisse revenir à la zone de saisie ;
  - vérifié sous Chromium seulement (seul navigateur des tests) ; un navigateur qui
    garderait le focus ou n'émettrait pas de `focusout` ne déclenche rien ;
  - test : `apps/web/e2e/reloads.spec.ts` vérifie que le champ « Fonction », validé
    par « Entrée », et le bouton « Tester la connexion » perdent le focus pendant le
    verrou puis le retrouvent, et qu'un focus donné entre-temps à un onglet, puis
    rendu à la page, annule ce rendu (« Enregistrer les attributs » ne le reprend
    pas). Ils échouent sans
    le module, et le dernier sans annulation (le `focusout` de l'élément quitté et
    `focusin` annulent chacun : retirer l'un seul ne le fait pas échouer).

## ADR-044 — Une lecture de l'API a un délai ; un échec de chargement se réessaie

- **Date** : 2026-10-19 · **Statut** : acceptée
- **Contexte** : depuis ADR-042, un écran est verrouillé tant que sa lecture n'a pas
  répondu. Le client n'avait aucun délai : une lecture sans réponse (réseau coupé,
  serveur bloqué) laissait l'écran verrouillé indéfiniment, sans message. Avant le
  verrou, l'écran restait au moins utilisable.
- **Décision** :
  - une lecture (`GET`) qui n'a pas répondu en 30 s (`READ_TIMEOUT_MS`,
    `apps/web/lib/api/client.ts`) est abandonnée et échoue avec « Le serveur ne
    répond pas. Réessayez dans un instant. » ; le délai est une minuterie de la page
    (`setTimeout`), que les tests de bout en bout avancent par l'horloge de
    Playwright ;
  - une modification n'a pas de délai : abandonnée côté client, elle pourrait avoir
    été appliquée par le serveur sans qu'on le sache ;
  - `Loaded` affiche l'échec d'un chargement avec un bouton « Réessayer » qui relance
    la lecture (`Failure`) ; les lectures faites hors de `Loaded` le font aussi : la
    liste des conversations (qui affichait « Aucune conversation » sur un échec) et
    la session (`/auth/me`, dont l'échec laissait un indicateur de chargement sans
    fin) ; le détail d'une conversation donne le message de l'échec ; la validation
    qu'attend un run, si elle n'a pu être chargée, montre l'échec et « Réessayer » au
    lieu de « Chargement de la demande de validation… ».
- **Conséquences** :
  - un écran n'est jamais verrouillé plus de 30 s : passé ce délai, il montre l'échec
    et propose de réessayer ;
  - une lecture légitime plus longue que 30 s échouerait : aucune ne l'est
    aujourd'hui (le flux d'un run passe par `EventSource`, hors de ce client) ;
  - « Réessayer » est retiré de la page quand la lecture reprend : le focus qu'il
    avait n'est pas rendu (ADR-043 ne remplace pas un contrôle retiré) ;
  - une modification sans réponse laisse son bouton en cours, et « Annuler » d'un
    formulaire de modification désactivé (ADR-042), tant qu'elle n'a pas répondu ;
  - en développement et dans les tests de bout en bout, `/api/*` passe par le relais
    de Next.js, qui coupe lui-même une requête sans activité au bout de 30 s (réponse 500) : le premier des deux délais l'emporte. En démonstration et en production,
    Caddy relaie `/api/*` directement à l'API : le délai du client est le seul ;
  - test : `apps/web/e2e/timeouts.spec.ts` retient une lecture et avance l'horloge
    de la page de 30 s : le rechargement des rôles, la liste des conversations et la
    session montrent le message, puis « Réessayer » charge l'écran. Chacun échoue
    sans ce qu'il vérifie (délai, liste donnée pour vide, session muette). Il fait
    aussi échouer le chargement de la validation qu'attend un run (second scénario du
    modèle scénarisé), puis la charge par « Réessayer ».

## ADR-045 — Des cases à cocher dessinées par l'interface

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : ADR-042 retarde de 150 ms l'aspect désactivé des contrôles verrouillés
  par un rechargement, pour qu'un rechargement rapide ne fasse pas clignoter l'écran.
  Une case à cocher native y échappait : le navigateur la grise aussitôt, ce qu'aucune
  règle CSS ne retarde.
- **Décision** :
  - `Checkbox` (`apps/web/components/ui/checkbox.tsx`) garde un vrai
    `<input type="checkbox">` (étiquette, clavier, formulaire), sans l'apparence du
    navigateur (`appearance: none`), et dessine la coche par-dessus ; son aspect
    désactivé est une opacité, de la case comme de la coche, que la règle d'ADR-042
    retarde (étendue à la coche) ; son contour (`--muted`) contraste avec le fond
    (5,4:1 en clair, 6,4:1 en sombre, WCAG 1.4.11) ; pas d'état indéterminé ;
  - toutes les cases à cocher de l'interface l'utilisent (rôles, politiques, règles
    des outils d'un serveur MCP) ; une règle ESLint refuse `type="checkbox"` ailleurs
    que dans `Checkbox`.
- **Conséquences** :
  - plus aucune exception au délai d'ADR-042 parmi les contrôles de l'interface ;
  - une case à cocher nouvelle passe par `Checkbox` ;
  - test : `apps/web/e2e/reloads.spec.ts` vérifie, pendant le verrou, qu'une case des
    règles d'outils est désactivée, sans apparence native, animée par le verrou, et
    qu'elle et sa coche finissent atténuées.

## ADR-046 — Le suivi en direct d'un run qui s'interrompt se signale et se rouvre

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : le chat suit un run par un flux (`EventSource`, `/api/runs/{id}/events`).
  Le navigateur reprend seul une connexion coupée, mais abandonne un flux auquel le
  serveur répond par une erreur (429 de la limite de débit, 500, 502 pendant un
  redémarrage), sans rien en dire. Depuis que les validations qu'attend un run ne se
  chargent plus qu'à l'annonce de son flux (PR #34), un flux abandonné laissait le chat
  sur « Je travaille sur votre demande… », sans la validation attendue.
- **Décision** :
  - `useRunStream` écoute l'erreur du flux : quand le navigateur l'a abandonné
    (`readyState` fermé), l'activité est marquée `lost` et le chat affiche « Le suivi
    en direct de la demande est interrompu. » avec « Réessayer » ;
  - « Réessayer » rouvre le flux, qui rejoue les événements du run depuis le début
    (validations attendues comprises) ;
  - un run qui a attendu plusieurs fois rejoue chaque attente : les chargements de
    ses validations peuvent se chevaucher, seul le dernier est affiché.
- **Conséquences** :
  - un flux abandonné par le navigateur n'est plus silencieux et se rouvre sans
    recharger la page ; une coupure que le navigateur répare seul (il se reconnecte)
    reste sans message ;
  - test : `apps/web/e2e/timeouts.spec.ts` répond par une erreur au flux d'un run,
    vérifie le message, puis « Réessayer » amène la validation (troisième scénario du
    modèle scénarisé, suivi à 9 h) ; il échoue si l'abandon n'est pas signalé. Un
    autre termine le premier flux pour que le navigateur se reconnecte seul, et
    vérifie qu'aucun message n'apparaît (quatrième scénario, suivi à 11 h).
    Aucun scénario n'attend deux fois : le dernier point n'a pas de test de bout en
    bout.

## ADR-047 — Modifications d'administration sur une ressource introuvable tracées (complète ADR-038)

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : ADR-038 trace les modifications d'administration refusées (403), pas
  celles d'un administrateur habilité sur une ressource que la plateforme répond
  introuvable (404). Toute l'administration s'exerce sur l'entreprise entière : pour
  un administrateur habilité, une ressource hors de son périmètre est celle d'une autre
  entreprise, que la RLS rend absente. Un identifiant d'une autre entreprise, deviné
  ou divulgué, essayé depuis l'administration ne laissait aucune trace.
- **Décision** :
  - le marqueur des réponses (`Refusal`, `http/error.rs`) porte aussi le message d'une
    réponse d'absence (`Refusal::NotFound`, code `NotFound`), jamais envoyé non plus ;
  - la couche d'audit de l'administration (`plugins/refusal_audit.rs`) trace, pour une
    requête modifiante d'un utilisateur connecté sur une route qui désigne une
    ressource par son identifiant (`{id}`, nommée par `resource_of`) et répondue
    introuvable, une entrée `admin.not_found` (`denied`) de même contenu qu'un refus :
    l'utilisateur, la raison (le message de la réponse : « Compte introuvable »…), la
    méthode et la route, l'identifiant du chemin, l'IP et l'identifiant de requête.
    Elle est écrite dans l'entreprise de l'administrateur seulement ;
  - l'entrée ne distingue pas une ressource absente d'une ressource d'une autre
    entreprise, pas plus que la réponse : les distinguer demanderait une lecture
    inter-entreprises à la portée de l'application, et une entrée qui les distinguerait
    donnerait aux lecteurs de l'audit de l'entreprise l'oracle d'existence que la
    réponse 404 refuse (un administrateur qui lit l'audit pourrait sonder des
    identifiants). L'exploitant, avec le rôle propriétaire, peut rapprocher ces
    identifiants des ressources des autres entreprises ;
  - seul un administrateur qui détient la permission sur toute l'entreprise atteint
    cette réponse : chaque opération vérifie la permission avant toute lecture, les
    autres sont refusés, et tracés comme refus (ADR-038) ;
  - une réponse d'absence qui ne désigne aucune ressource (rétablir le modèle de la
    plateforme quand l'entreprise l'utilise déjà) n'est pas une tentative sur une
    ressource : elle n'est pas tracée ;
  - sur une route à identifiant, une réponse d'absence est rapportée à la ressource du
    chemin : par convention, un identifiant inconnu envoyé dans le corps répond comme
    une requête invalide (400 : « Rôle inconnu », « Unité inconnue »…), jamais 404.
- **Conséquences** :
  - ne sont pas tracés : un identifiant inconnu envoyé dans le corps d'une requête (un
    rôle, une unité, un responsable…), qui répond 400 comme une requête invalide
    (feuille de route) ; les lectures ; les ressources hors périmètre des autres routes
    (conversation, run, mémoire d'un autre membre, qui répondent 404), hors de
    l'administration ;
  - une suppression concurrente (deux administrateurs suppriment la même ressource)
    laisse aussi une entrée, pour le second ;
  - une route d'administration nouvelle à identifiant est couverte si sa collection est
    nommée dans `resource_of` ; sinon, un refus y est tracé sans ressource et une
    absence ne l'est pas. Un test unitaire relève les chemins à identifiant du routeur
    de l'administration (`routes/admin/mod.rs`) et échoue sur une collection non
    nommée ;
  - le volume est borné par la limitation de débit globale et par la permission
    requise ;
  - test : `crates/api/tests/admin_absent_resources.rs` modifie un compte et supprime
    un rôle d'une autre entreprise, puis d'autres qui n'existent nulle part : mêmes
    réponses, mêmes entrées hormis l'identifiant, rien d'écrit dans l'audit de l'autre
    entreprise, rien de ce que la requête envoie hormis l'identifiant ; chaque
    collection de l'administration est nommée ; un administrateur limité à une équipe
    et un membre sont refusés avant, ni le rétablissement du modèle ni un rôle inconnu
    envoyé dans le corps ne sont tracés ; la réponse reste 404 quand la trace ne
    s'écrit pas. Retirer le marqueur ou la condition sur la ressource désignée fait
    échouer ces tests.

## ADR-048 — Les connecteurs indisponibles sont signalés aux runs

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : un connecteur qui ne démarre pas laisse le run sans ses outils (ADR-025)
  ; seuls ses administrateurs le savaient (statut et dernière erreur de l'instance). Le
  modèle, ne voyant pas les outils, pouvait conclure qu'une information n'existait pas,
  et l'utilisateur ignorait pourquoi la réponse était incomplète. `TenantTools.issues`
  n'était lu que par les tests.
- **Décision** :
  - au démarrage d'un run (pas à sa reprise après validation, dont l'historique est
    figé), les connecteurs indisponibles **à la portée de l'utilisateur** sont passés au
    moteur (`StartInput.unavailable_connectors`) ;
  - la portée se juge sur ce que chaque outil du type exige, faute de connaître les
    outils d'un connecteur qui ne répond pas : `connector:use` pour un serveur MCP (tout
    outil MCP l'exige), l'une des permissions métier qu'il introduit pour un autre type
    (`services/connectors/reach.rs` ; un test vérifie, pour chaque type autre que MCP,
    que chacun de ses outils exige l'une d'elles). La permission est évaluée avec le
    plafond de l'agent ;
  - le modèle l'apprend par une note `<connecteurs_indisponibles>` en tête du tour
    utilisateur, qui nomme les connecteurs et lui demande de signaler ce qui manque
    plutôt que de conclure que l'information n'existe pas. Le prompt système reste stable
    ; les noms, choisis par un administrateur, tiennent sur une ligne (contrôles et
    séparateurs remplacés par une espace) et ne peuvent fermer ni la note (chevrons
    remplacés) ni leurs guillemets (« » remplacés) ;
  - l'utilisateur l'apprend par l'événement `connectors_unavailable` du suivi en
    direct, émis juste après `run_started` et rejoué comme les autres ; le panneau
    d'activité affiche « Le connecteur « … » est indisponible : la réponse peut être
    incomplète. ».
- **Conséquences** :
  - la portée est plus large que les outils qu'un run se verrait proposer : un membre
    qui détient `connector:use` voit le nom d'un serveur MCP indisponible même si les
    politiques de ses outils les lui auraient tous retirés (la plateforme ne connaît pas
    les outils d'un serveur qui ne répond pas) ; pour un autre type, une seule des
    permissions qu'il introduit suffit, alors qu'un outil peut en exiger plusieurs
    (`planning_propose` : `planning:propose` et `intervention:read`) ou aucune ne
    l'exiger seule (`intervention:write`). Le nom d'un connecteur est une donnée de
    configuration de l'entreprise, pas une donnée métier ;
  - un connecteur modifié, désactivé ou supprimé pendant son démarrage est signalé au
    run qui l'attendait : ce run est bien privé de ses outils ; le suivant ne le signale
    plus ;
  - un run repris après validation n'est pas averti d'un connecteur tombé entre-temps :
    l'appel à l'un de ses outils échoue et le modèle en reçoit l'erreur ;
  - le message montré à l'utilisateur disparaît avec le panneau d'activité à la fin du
    run ; l'événement reste dans l'historique du run ;
  - tests : `crates/agent/src/runtime/unavailable.rs` (note, noms neutralisés),
    `crates/agent/tests/runtime_tools.rs` (note et événement, ou rien quand tout
    démarre), `crates/api/src/services/connectors/reach.rs` (portée),
    `crates/api/tests/connector_notices.rs` (un serveur MCP et le connecteur de
    démonstration indisponibles : le responsable technique est averti des deux,
    l'administrateur technique, sans permission métier, du seul serveur MCP),
    `apps/web/e2e/unavailable-connectors.spec.ts` (le message dans le panneau, puis le
    run continue avec les autres connecteurs). Laisser passer tous les connecteurs dans
    le filtre de portée fait échouer les tests.

## ADR-049 — Une tentative de démarrage d'un connecteur est partagée par les runs qui l'attendent

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : le cache des connecteurs (`services/connectors/cache/`) garde les
  instances démarrées et se souvient une minute d'un échec. Mais les runs lancés pendant
  qu'une instance démarrait (après un changement de configuration, au premier run après
  un redémarrage, ou une minute après un échec) faisaient chacun leur propre tentative :
  autant de connexions au serveur MCP, chacune attendue jusqu'au délai de démarrage, et
  autant d'instances aussitôt refermées sauf la première terminée.
- **Décision** :
  - une tentative en cours occupe l'emplacement de l'instance (`Slot::Starting`, avec sa
    version) ; un run qui a besoin de la même version attend son issue (un canal
    `watch` dont l'émetteur appartient au run qui démarre) au lieu d'en faire une
    autre, puis relit l'emplacement : l'instance démarrée, ou l'échec, qu'il reçoit
    comme déjà signalé (`Remembered` : journalisé et enregistré une seule fois) ;
  - un run qui abandonne sa tentative (sa tâche annulée) lâche l'émetteur : les runs qui
    attendaient se réveillent, trouvent la tentative orpheline et l'un d'eux la reprend ;
  - un run qui a lu une instance modifiée depuis (son ticket précède l'invalidation)
    reçoit l'instance vivante de la version qu'il a lue (même configuration, même
    jeton) ; il n'en attend ni n'en démarre aucune autre : il répond aussitôt « changé
    pendant son démarrage », ce que la tentative aurait de toute façon donné ;
  - la règle de version est inchangée : une tentative d'une autre version remplace
    l'emplacement, l'instance d'une version plus récente l'emporte, une tentative
    invalidée n'est jamais gardée.
- **Conséquences** :
  - un serveur qui ne répond pas retarde les runs d'une seule tentative (jusqu'au délai
    de démarrage, que tout type de connecteur doit borner : `Connector::instantiate`), puis, l'échec retenu, un run par minute ; les runs attendent toujours
    la tentative en cours (la démarrer en arrière-plan sans les faire attendre reste à la
    feuille de route) ;
  - tests (`cache/tests.rs`) : deux runs simultanés obtiennent la même instance d'un seul
    démarrage ; un échec en cours est reçu une fois comme nouveau, puis comme retenu ; une
    tentative abandonnée est reprise par le run qui l'attendait ; un run au ticket
    périmé ne démarre rien mais reçoit l'instance vivante de sa version. Les tests où un
    run attend sont bornés : un run qui attendrait toujours les fait échouer.
    Retirer l'attente, la reprise d'une tentative orpheline ou le refus d'un ticket
    périmé fait échouer ces tests.

## ADR-050 — Un serveur MCP s'essaie avant d'être enregistré

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : un administrateur ne pouvait tester un serveur MCP qu'après l'avoir
  ajouté (`POST /admin/connectors/{id}/test`). Une adresse ou un jeton erroné laissait
  donc un connecteur en erreur, actif pour les runs (qui l'attendaient au démarrage),
  jusqu'à sa correction ou sa suppression.
- **Décision** :
  - `POST /admin/connectors/trial` (`connector:manage` sur toute l'entreprise, corps
    strict `ConnectorTrial` : préfixe, adresse, jeton facultatif) démarre une instance
    neuve comme un run le ferait, avec l'adresse et le jeton saisis, liste ses outils tels
    qu'ils se déclarent (risque et permissions par défaut, aucune politique n'existant
    encore), puis la libère. La réponse est celle du test d'un connecteur enregistré
    (`ConnectorTest`) ;
  - les contrôles sont ceux de l'ajout (préfixe, adresse joignable par cette
    plateforme, format du jeton) et précèdent toute connexion ; la résolution DNS filtrée
    et l'absence de redirection s'appliquent comme à un run ;
  - rien n'est enregistré, ni connecteur ni secret ; l'audit garde une entrée
    `connector.tried` (adresse, préfixe, jeton envoyé ou non, issue, nombre d'outils),
    jamais le jeton, que l'échec renvoyé masque aussi. L'essai s'exécute à part de sa
    requête (tâche d'arrière-plan dont la requête attend la réponse) : un client qui
    s'en va avant la fin ne l'interrompt pas avant que son entrée soit écrite ;
  - les contrôles d'ajout (`mcp_addition_problem`) et le démarrage d'essai
    (`start_and_list`, que le test d'un connecteur enregistré emploie aussi avec ses
    politiques) sont partagés ;
  - le formulaire offre « Tester avant d'enregistrer » à l'ajout, et à la modification
    quand l'adresse change ou qu'un jeton est saisi : un essai n'envoie jamais le jeton
    enregistré, que le test de la fiche du connecteur emploie. Le résultat (serveur joint ou
    non, ou valeurs refusées) n'est affiché que tant que les valeurs essayées n'ont pas
    changé.
- **Conséquences** :
  - un essai fait joindre au serveur une adresse publique choisie par l'administrateur,
    comme l'ajout puis le test le faisaient déjà. Seules la permission requise et la
    limitation de débit globale (par adresse cliente) en bornent le volume : rien ne
    limite encore le nombre d'essais ou de tests simultanés d'une entreprise, dont
    chacun tient une connexion jusqu'au délai de démarrage (feuille de route) ;
  - les tests : `crates/api/tests/connector_trials.rs` (essai réussi avec jeton, rien
    d'enregistré, audit sans jeton ; échec d'un serveur qui cite le jeton, masqué ;
    adresses privée ou en http, préfixe et jeton invalides, champ inconnu refusés sans
    essai ; membre refusé et refus tracé ; essai abandonné par son client, audité
    quand même) ; `apps/web/e2e/integrations.spec.ts` (un nom du réseau privé refusé à la
    connexion, une adresse privée refusée avant, chaque résultat écarté quand l'adresse
    change, une réponse réussie affichée, rien d'ajouté). Exécuter l'essai dans sa
    requête, ou afficher un résultat ou un refus pour d'autres valeurs que celles
    essayées, fait échouer ces tests.

## ADR-051 — Images publiées sur le registre de GitHub (complète ADR-032 et ADR-034)

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : la CI construisait et vérifiait les images de l'API et de l'interface,
  puis les jetait. Installer la pile de démonstration ou de production demandait donc de
  les construire sur le serveur (Rust et Next.js : temps, mémoire, outillage), et rien
  ne garantissait qu'une installation tournait le code vérifié par la CI.
- **Décision** :
  - le workflow `.github/workflows/images.yml` publie trois images (amd64) sur le
    registre de GitHub (`ghcr.io/orizonlab/`) : `mimorya-api` (API, migrations,
    commandes d'exploitation, données de démonstration), `mimorya-web` (interface sans
    comptes de démonstration) et `mimorya-web-demo` (interface qui les liste). Le relais
    `/api/*` étant fixé à la construction, les deux interfaces visent `http://api:4000`,
    nom du service des deux piles ;
  - **publication** : après une CI **verte** sur un commit poussé sur `main` du dépôt
    même (`workflow_run` ; la branche `main` d'un dépôt dérivé, dont la CI tourne aussi,
    n'est jamais publiée), les images du commit que la CI a vérifié (`head_sha`) sont
    construites et publiées sous `sha-<12 caractères>`, **une seule fois** : une CI
    relancée ne remplace jamais ce qu'une installation fait peut-être déjà tourner sous
    ce nom. Seule une réponse « introuvable » du registre compte comme une absence
    (`infra/docker/image-digest.sh`) : un registre qui ne répond pas, limite le débit ou
    refuse l'accès arrête la publication au lieu de la relancer ;
  - **promotion** : une fois les trois publiées, `main` et `latest` les désignent
    ensemble (`imagetools create`), si le commit est encore la tête de `main` ; une
    promotion à la fois, celle qui arrive remplaçant celle qui attend. Ces noms ne
    reviennent donc jamais à un commit plus ancien (une CI relancée, une promotion en
    retard). Les CI de `main` tournant chacune jusqu'au bout, en parallèle, elles
    peuvent finir dans le désordre : si la promotion d'un commit plus ancien remplace
    celle, en attente, de la tête, aucune ne nomme rien, et `main` et `latest` restent
    sur un commit antérieur jusqu'à la fusion suivante, la tête restant publiée sous
    `sha-<12>` ;
  - **version** : une étiquette `vX.Y.Z` (éventuellement suivie d'un suffixe) sur un
    commit de `main` nomme `X.Y.Z` les images déjà publiées pour ce commit, sans rien
    reconstruire. Un commit hors de `main`, ou jamais publié (CI en échec), n'a pas de
    version : le travail échoue. Une version déjà donnée ne change jamais d'images : si
    `X.Y.Z` en désigne d'autres, le travail échoue sans rien renommer ;
  - la liste des images (nom, Dockerfile, arguments de construction, description) est
    écrite une seule fois, dans `infra/docker/images.json`, que lisent tous les travaux
    du workflow. `compose.demo.build.yml` et `compose.production.build.yml` en répètent
    le Dockerfile et les arguments (aujourd'hui identiques) : les modifier ensemble ;
  - le jeton de la CI (`GITHUB_TOKEN`, permission `packages: write` sur les seuls travaux
    qui publient, pas sur la construction des PR) suffit : ni compte ni secret
    supplémentaire. Les images portent la source du dépôt
    (`org.opencontainers.image.source`), sa révision, leur version, leur date et une
    description ;
  - une PR qui modifie le workflow ou `infra/docker/` construit les trois images sans
    rien publier ; chaque image garde son cache de construction (`type=gha`, une portée
    par image) ;
  - les piles (`compose.demo.yml`, `compose.production.yml`) ne font que nommer les
    images publiées (`ghcr.io/orizonlab/…:${MIMORYA_VERSION:-latest}`), sans les
    construire : une image introuvable (version mal écrite, paquet privé, autre
    architecture) arrête la pile au lieu d'être remplacée par une construction locale
    sous le nom publié. `compose.demo.build.yml` et `compose.production.build.yml` les
    construisent depuis le dépôt sous des noms locaux (`mimorya-api:local`…) ; la CI
    vérifie les piles ainsi.
- **Conséquences** :
  - une installation fixe `MIMORYA_VERSION` (une version, sinon un commit) pour garder
    la même d'un redémarrage à l'autre, et se fait depuis le dépôt extrait à la même
    version : le fichier compose et les fichiers qu'il monte (Caddy, initialisation de la
    base) vont avec les images ;
  - la CI vérifie les piles construites depuis le dépôt, pas les images téléchargées :
    rien ne démarre encore une image publiée avant qu'une installation le fasse ;
  - le code étant public, publier ses images ne révèle rien de plus. Leur visibilité est
    à constater au premier envoi : un paquet lié à un dépôt public devrait l'être, sinon
    son propriétaire (le compte OrizonLab) le rend public dans les réglages du paquet ;
  - premier envoi constaté le 2026-10-20 (commit `20409bf`) : les trois paquets sont
    publics et téléchargeables sans compte, `latest`, `main` et `sha-20409bfe048a`
    désignant les mêmes images ; pour un paquet qui n'existait pas encore, le registre a
    répondu « introuvable », comme le suppose la publication. Depuis, chaque
    promotion jouée vérifie que les images de son commit se téléchargent sans compte :
    un paquet resté privé fait échouer le travail ;
  - restent à faire : la signature des images ; arm64 ; l'épinglage des actions par
    empreinte ; que toute promotion, même celle d'un commit plus ancien, nomme `main` et
    `latest` la tête de `main` si ses images sont publiées. Le cache du travail de CI
    qui vérifie les piles n'est pas retenu : ce travail dure moins que celui de Rust.
    Une PR qui touche les images en vérifie la construction, pas l'envoi.

## ADR-052 — Les compétences s'écrivent et s'activent depuis l'administration

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : les compétences (procédures versionnées que l'agent charge avec
  `skill_read`, format agentskills.io, ADR-002) n'existaient que par les données de
  démonstration : aucune entreprise ne pouvait en écrire. Par ailleurs, `skill_read`
  rendait la compétence chargée comme tout résultat d'outil, encadrée en « donnée non
  fiable » dont les consignes système interdisent de suivre les instructions : le modèle
  recevait l'ordre de charger une procédure et celui de ne pas la suivre.
- **Décision** :
  - un administrateur détenant `skill:manage` sur toute l'entreprise (permission
    d'administration, non délégable) écrit une compétence et sa première version
    (`POST /admin/skills`), écrit de nouvelles versions (`POST …/versions`), rend une
    version active (`PUT …/activation`, une version plus ancienne comprise : c'est le
    retour arrière, la version active jusque-là étant retirée), retire la compétence
    (`DELETE …/activation` : plus aucune version active, l'agent ne la voit plus),
    modifie sa description et supprime une compétence qui n'est pas active, avec ses
    versions ;
  - une version écrite ne change plus : son contenu, sa note, son auteur restent ; la
    personne qui l'a rendue active en dernier et la date sont gardées. Rien n'atteint un
    run tant qu'aucune version n'est active : une compétence et ses nouvelles versions
    naissent brouillons. Les statuts `candidate` et `rejected` restent réservés aux
    évaluations (phase 2) ;
  - ce qui est écrit est vérifié avant tout enregistrement (`mimorya_core::skill_rules`) :
    un nom que l'agent peut demander (lettres minuscules, chiffres, tirets simples, 2 à
    64 caractères, unique dans l'entreprise) ; une description de 10 à 300 caractères
    sur une seule ligne visible, puisqu'elle est une ligne de l'index des compétences du
    prompt système et ne doit pouvoir ni le casser ni y ouvrir une section ; un contenu
    non vide d'au plus 20 000 caractères, en texte visible, retours à la ligne et
    tabulations seulement (aucun caractère invisible : ce que lit l'administrateur est ce
    que lit le modèle), fins de ligne Windows ramenées à `\n` ; une note de version
    d'une ligne. Au plus 40 compétences actives par entreprise, l'index étant répété dans
    chaque run ;
  - les changements passent sous le verrou d'administration (ADR-020) et sont audités
    (`skill.created`, `skill.version_created`, `skill.activated` avec la version
    précédente, `skill.retired`, `skill.updated`, `skill.deleted`) sans le contenu ; les
    refus et les compétences introuvables (d'une autre entreprise comprise) le sont
    comme les autres (ADR-038, ADR-047, ressource `skill`) ;
  - la compétence chargée par `skill_read` arrive au modèle encadrée par
    `<procedure_entreprise source="skill_read">`, dont le contenu ne peut ni fermer ni
    ouvrir une enveloppe ; les consignes système disent que c'est une procédure validée
    par l'entreprise, à suivre pour la tâche qu'elle couvre, qui ne peut ni étendre les
    droits de l'agent, ni lui éviter une validation, ni lever les règles de sécurité. Le
    nom `skill_read` est réservé : les noms d'outils sont uniques dans un registre (un
    second `skill_read` est refusé, testé), ceux d'un serveur MCP préfixés (`mcp_…`),
    ceux du connecteur de démonstration fixés dans le code. Un refus de `skill_read`
    (compétence introuvable) et tout autre résultat restent des données non fiables ;
  - aucun texte que la plateforme encadre, résultat d'outil, compétence ou mémoire
    rappelée, ne peut ouvrir ni fermer l'une de ses enveloppes (`<donnees_outil>`,
    `<procedure_entreprise>`, `<contexte_memoire>`, `<connecteurs_indisponibles>`),
    quelles que soient la casse, les espaces ou les caractères invisibles qui coupent
    une balise (`neutralise_envelopes`) : une mémoire
    qu'écrit un détenteur de `memory:write` ne peut pas se faire passer pour une
    compétence ;
  - l'écran « Compétences » de l'administration liste les compétences, en écrit,
    montre les versions et leur contenu, écrit une version à partir de la dernière, en
    rend une active (« Revenir à » pour une plus ancienne), retire et supprime.
- **Conséquences** :
  - une compétence ne porte aucune permission : chaque outil qu'elle mentionne reste
    soumis au PEP et aux validations ; la confiance qu'on lui accorde est celle qu'on
    accorde à l'administrateur qui l'a rendue active ;
  - le détenteur de `skill:manage` (le rôle « Administrateur technique », sans accès aux
    données métier) oriente désormais l'agent qui travaille pour les utilisateurs
    métier : les lectures restent dans les droits de chaque utilisateur, les écritures et
    sorties restent soumises à sa validation ;
  - l'index des compétences change avec elles : le prompt système d'un run ne change
    pas pendant ce run, mais deux runs successifs peuvent différer ;
  - non vérifié avec un modèle réel (comme le reste du moteur) : à valider avec le
    scénario de référence ;
  - restent à faire (phase 2) : propositions de version par l'agent, cas d'évaluation et
    non-régression avant activation, compétences propres à une unité (`org_unit_id`),
    import d'un `SKILL.md` avec son en-tête ; le détail d'une compétence renvoie le
    contenu de toutes ses versions, à paginer si elles deviennent nombreuses.

## ADR-053 — Les runs interrompus par un arrêt du service sont repris au démarrage (complète ADR-016)

- **Date** : 2026-10-20 · **Statut** : acceptée · reprise au démarrage et verrou
  d'exécution remplacés par l'ADR-086 (reprise par bail, plusieurs workers)
- **Contexte** : un run en cours quand le service s'arrête (redéploiement, arrêt brutal,
  plantage) restait `running` jusqu'à ce que le balayage le passe en échec 15 minutes
  plus tard (ADR-016) : la demande de l'utilisateur était perdue, même lorsque rien
  n'empêchait de la poursuivre. L'état d'un run n'était enregistré qu'en fin de tour
  d'outils, jamais avant le premier appel au modèle, et rien n'indiquait qu'une action
  avait commencé avant d'en avoir le résultat.
- **Décision** :
  - au démarrage, une fois que l'API écoute, elle reprend chaque run encore `running`,
    dans sa propre entreprise (`system_interrupted_runs`, fonction `SECURITY DEFINER`
    qui ne donne que l'entreprise et l'identifiant). Une seule instance exécute les runs
    (ADR-016) : à ce moment, chacun d'eux a été interrompu. Ce n'est plus seulement
    écrit : l'API tient, pour toute sa vie, un verrou consultatif PostgreSQL de session
    (`pg_try_advisory_lock`, sur une connexion à elle) ; un second processus sur la même
    base ne l'obtient pas et refuse de démarrer, au lieu de reprendre ou de doubler les
    runs que le premier exécute. Le verrou n'est pris, et les runs repris, qu'une fois le
    port d'écoute obtenu. Sa connexion est nommée `mimorya-run-executor` (visible dans
    `pg_stat_activity`, ce que rappelle le message d'erreur) et sondée par le serveur
    (keepalive TCP après une minute d'inactivité) : si l'hôte de l'API disparaît sans la
    fermer, le verrou est libéré en quelques minutes, non en deux heures (sur une
    connexion par socket Unix, sans effet ni besoin : le serveur est sur le même hôte) ;
  - le balayage des runs bloqués n'agit qu'une fois le processus en marche depuis aussi
    longtemps qu'un run met à devenir « bloqué » (15 minutes) : seuls ses propres runs
    peuvent alors l'être, et un run repris garde son ancienne heure d'enregistrement
    jusqu'à sa prochaine étape. La purge des liens expirés reste immédiate ;
  - un run est enregistré dès son démarrage, avant le premier appel au modèle, puis à
    chaque étape comme avant ; avec son état est gardé le numéro du dernier événement de
    son journal (`agent_runs.checkpoint_seq`, repère monotone qui ne dépend d'aucune
    horloge). Il reprend à sa dernière étape enregistrée ; l'appel au modèle et les
    appels d'outils lancés depuis sont perdus ;
  - le moteur ne le reprend que si rien d'autre qu'une lecture n'a été lancé depuis
    cette étape : les appels autorisés commencés depuis sont les événements `tool_call`
    postérieurs au repère, et chacun doit être une lecture : selon le risque enregistré
    dans l'événement au moment de l'appel (surcharge de l'administrateur comprise) et selon
    l'outil s'il est encore présent, le plus élevé des deux l'emportant (un outil reconnu
    depuis comme écrivant n'est pas relancé). Un appel sans risque enregistré d'un outil
    disparu est inconnu. Une
    action qui a pu changer quelque chose (écriture, brouillon, envoi…) n'est jamais
    relancée à l'aveugle : le run échoue en nommant l'action, dont l'effet est à
    vérifier. Il échoue aussi s'il était en train d'exécuter des actions validées (en
    nommant celles qui avaient été validées, ou en disant que rien n'a été exécuté si
    aucune ne l'avait été), s'il a déjà été repris deux fois (`resumptions`, compté dans
    son état), s'il n'avait encore aucune étape enregistrée, s'il est interrompu depuis
    plus d'une heure (sa demande peut avoir perdu son objet), ou si son utilisateur ne
    peut plus agir, ou si le moteur ne peut pas être reconstruit (fournisseur retiré…).
    Chacune de ces fins nomme, par leur titre, les actions qui étaient en cours et dont
    l'effet n'est pas connu (toute action validée, ou qui n'a pas fait que lire, jugée par
    la même règle que le moteur, `InterruptedCall::only_read` : l'API, qui ne démarre
    alors aucun connecteur, prend pour risque actuel celui que l'administrateur a fixé
    dans les politiques d'outils) : l'utilisateur est invité à vérifier, jamais à relancer
    à l'aveugle. Ensuite, chaque
    étape relit les droits comme avant (ADR-035) ;
  - les demandes de validation encore en attente sont annulées et l'annulation est
    auditée (`approval.cancelled`, acteur système) : elles appartenaient à l'étape
    perdue, et le run les redemande s'il en a besoin ;
  - une reprise est annoncée (événement `run_resumed`, qui liste les lectures
    interrompues) ; une fin est enregistrée et annoncée comme celle d'un run qui échoue
    (statut, message dans la conversation, audit, `run_finished`).
- **Conséquences** :
  - un redéploiement ne coûte plus que l'étape en cours d'un run qui ne faisait que lire,
    plus le délai du redémarrage ; l'appel au modèle perdu est facturé (enregistré dans
    la consommation) mais absent du total du run ;
  - le suivi en direct affiche la reprise : à `run_resumed`, les étapes sans résultat
    sont marquées « Interrompue par un arrêt du service » et une ligne dit où le run a
    repris (une par reprise). Tout appel qui se termine reçoit son résultat dans le flux,
    y compris un appel refusé par l'utilisateur, dont la demande a expiré, dont l'outil a
    été retiré, ou abandonné (`tool_result` d'erreur, dont le texte nomme le cas sans
    reprendre le commentaire de l'utilisateur) ; un appel refusé par la plateforme est,
    lui, terminé dès son `tool_call`. Une étape sans résultat est donc toujours une étape
    perdue, à une exception transitoire près : un run dont le journal est antérieur à
    l'envoi d'un `tool_result` pour tout appel terminé (PR #44), repris ensuite,
    montrerait encore un appel refusé, ou dont l'outil a été retiré, comme interrompu
    (affichage seulement). Une validation annulée par la reprise n'a jamais été
    affichée dans le fil (il ne charge les cartes qu'une fois le run en attente, et un
    run en attente n'est pas repris). Le serveur des tests de bout en bout prend le
    verrou et reprend les runs comme l'API ; il en laisse un, interrompu pendant une
    lecture après une validation refusée, que `apps/web/e2e/interrupted-run.spec.ts` suit
    jusqu'à sa nouvelle validation ;
  - le verrou disparaît avec sa connexion (redémarrage de PostgreSQL) sans que l'API le
    reprenne : un second processus démarré ensuite pourrait l'obtenir. Une seule instance
    reste une règle d'exploitation (ADR-016) ;
  - restent à faire (phase 2, ADR-012) : plusieurs instances, avec un bail par run et un
    worker séparé ; la reprise d'une étape dont les actions ont un identifiant
    d'idempotence.

## ADR-054 — Identifiants inconnus envoyés dans le corps d'une requête d'administration tracés (complète ADR-047)

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : ADR-047 trace une modification d'administration sur une ressource que
  désigne le chemin et que la plateforme répond introuvable. Un identifiant envoyé dans
  le corps (le rôle d'une habilitation, l'unité de son périmètre, l'unité parente ou le
  responsable d'une unité, l'unité d'un compte) répond, lui, comme une requête invalide
  (400 : « Rôle inconnu », « Unité inconnue »…) et ne laissait aucune trace : un
  identifiant d'une autre entreprise, deviné ou divulgué, essayé là n'apparaissait nulle
  part.
- **Décision** :
  - une erreur du domaine peut porter la référence inconnue (`DomainError::unknown` :
    le type de ressource attendu et l'identifiant), jamais envoyée : la réponse reste
    celle d'une requête invalide, identique pour une ressource d'une autre entreprise et
    pour une ressource absente ;
  - le marqueur des réponses (`Refusal::UnknownReference`) la remet à la couche d'audit
    de l'administration (`plugins/refusal_audit.rs`), qui trace une entrée
    `admin.unknown_reference` (`denied`) : l'utilisateur, la raison (le message de la
    réponse), la méthode et la route, la ressource que désigne le chemin s'il en désigne
    une, la référence (`metadata.reference` : type et identifiant), l'IP et
    l'identifiant de requête, dans l'entreprise de l'administrateur seulement ;
  - comme pour ADR-047, l'entrée ne distingue pas une ressource absente d'une ressource
    d'une autre entreprise : la distinguer ferait de l'audit un oracle d'existence pour
    ses lecteurs ;
  - sont concernés les six contrôles qui cherchent dans l'entreprise un identifiant du
    corps : rôle et unité du périmètre d'une habilitation, unité parente d'une unité
    créée ou déplacée, responsable d'une unité, unité d'un compte créé ou modifié.
    L'identifiant est contrôlé par un type (UUID) : rien d'autre de ce que la requête a
    envoyé n'atteint l'audit.
- **Conséquences** :
  - ne sont pas tracées les autres requêtes invalides (type d'unité inconnu, date
    d'échéance passée, permission inconnue : des valeurs, pas des identifiants de
    ressource) ;
  - seul un administrateur habilité atteint ces contrôles : les autres sont refusés
    avant, et tracés comme refus (ADR-038) ;
  - un nouveau contrôle d'identifiant envoyé dans le corps doit répondre par
    `DomainError::unknown` pour être tracé ;
  - l'unité du périmètre d'une habilitation est contrôlée par l'API avant la règle du
    cœur (`assignment_problem`), qui garde le même contrôle en défense ; de même pour
    l'unité parente d'un déplacement (`move_problem`) ;
  - test : `crates/api/tests/admin_unknown_references.rs` envoie chaque identifiant
    d'une autre entreprise puis d'une ressource absente : mêmes réponses, mêmes entrées
    hormis l'identifiant, rien dans l'audit de l'autre entreprise, rien de ce que la
    requête envoie hormis les identifiants ; un administrateur limité et un membre sont
    refusés avant, et une requête invalide d'une autre sorte n'est pas tracée.

## ADR-055 — Démarrages de connecteurs demandés par l'administration bornés par entreprise

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : tester un connecteur enregistré ou essayer un serveur MCP avant de
  l'enregistrer (ADR-050) démarre un connecteur, qui peut attendre un serveur lent
  jusqu'au délai de démarrage. Ces démarrages n'étaient bornés que par la limitation de
  débit globale : les administrateurs d'une entreprise pouvaient en garder un grand
  nombre en cours (connexions sortantes, tâches), au détriment des autres entreprises.
- **Décision** :
  - une entreprise a au plus deux démarrages demandés par l'administration en cours à
    la fois (`MAX_ADMIN_STARTS`, `services/connectors/admin_starts.rs`), tests et essais
    confondus ; un troisième est refusé (409, « Deux démarrages de connecteur sont déjà
    en cours dans votre entreprise : réessayez quand l'un d'eux aura fini. »), après la
    vérification de la permission, des paramètres et, pour un test, de l'existence du
    connecteur : un connecteur absent reste répondu introuvable, et tracé (ADR-047) ;
  - chaque démarrage tient un créneau tant qu'il dure : le test le rend une fois son
    résultat enregistré ; l'essai, qui se poursuit hors de sa requête (ADR-050), le rend
    à sa fin, même si son client est parti, et avant de répondre ;
  - les démarrages des runs ne sont pas comptés : un même démarrage sert les runs qui en
    ont besoin, puis il est gardé (ADR-049) ;
  - le compte vit en mémoire du processus, qui est seul à exécuter (ADR-016).
- **Conséquences** :
  - un refus de ce type n'est pas tracé : ce n'est pas une modification refusée ;
  - plusieurs instances (phase 2) demanderaient un compte partagé ;
  - test : `crates/api/tests/connector_admin_starts.rs` retient deux essais sur un
    serveur qui les fait attendre (le client du premier part), puis vérifie qu'un
    troisième essai et un test sont refusés, qu'un connecteur absent est toujours
    répondu introuvable et tracé, un membre refusé et un essai invalide répondu comme
    tel, et que les deux créneaux reviennent une fois les essais finis (deux essais
    sont de nouveau retenus) ; un test unitaire vérifie qu'une autre entreprise n'est
    pas retenue.

## ADR-056 — Session MCP gardée entre les runs : reprise d'un serveur redémarré, redémarrage d'une session fermée (complète ADR-025 et ADR-049)

- **Date** : 2026-10-20 · **Statut** : acceptée
- **Contexte** : une instance de connecteur démarrée est gardée entre les runs
  (ADR-049) et n'était reconstruite qu'au changement de sa configuration ou au
  redémarrage de la plateforme. La feuille de route demandait de vérifier la vivacité
  de cette session : un serveur qui redémarre, ou qui s'arrête, la laissait-il
  inutilisable ?
- **Constat** (vérifié par des tests) :
  - un serveur HTTP qui redémarre ne connaît plus la session : il répond 404, et le
    client MCP (`rmcp` 3.5, `reinit_on_expired_session`, actif par défaut) en ouvre une
    nouvelle, puis rejoue l'appel. Pendant l'arrêt, les appels échouent (résultat
    d'erreur pour le modèle) ; une fois le serveur revenu, la même session le joint ;
  - un serveur local (stdio) qui s'arrête ferme son transport pour de bon : tous les
    appels suivants échouaient jusqu'au redémarrage de la plateforme.
- **Décision** :
  - une instance dit si sa connexion est fermée pour de bon (`ConnectorCloser::is_closed`,
    `ConnectorRuntime::is_closed`) : pour un serveur MCP, quand le transport de sa
    session est fermé ;
  - le cache ne sert pas une instance fermée : il la libère (journal
    d'avertissement, sans secret) et le run qui en a besoin la démarre de nouveau, comme
    au premier démarrage (démarrage partagé, échec retenu, ADR-049) ;
  - rien n'est ajouté pour HTTP, où le client rejoint seul un serveur redémarré.
- **Conséquences** :
  - un run déjà en cours garde l'instance qu'il a reçue : ses appels échouent jusqu'à
    sa fin ; le run suivant démarre une nouvelle session ;
  - `is_closed` est lu sous le verrou du cache, avant chaque run servi : il lit un
    état, sans jamais attendre le serveur ;
  - limites restantes : un serveur stdio vivant mais muet ne se dit jamais fermé (ses
    appels échouent au délai d'appel, jusqu'au redémarrage de la plateforme ou à un
    changement de l'instance) ; un serveur HTTP qui ne répond pas 404 à une session
    inconnue n'est pas rejoint ; un serveur stdio qui démarre puis s'arrête aussitôt est
    démarré de nouveau par chaque run, sans le délai qui suit un démarrage échoué ;
  - tests : `crates/connectors/tests/mcp_sessions.rs` (un serveur HTTP arrêté puis
    relancé sur le même port est rejoint par la même session, qui ne se dit jamais
    fermée ; un serveur stdio arrêté laisse une session qui se dit fermée) et
    `services/connectors/cache/tests.rs` (une instance fermée est libérée et démarrée de
    nouveau, une instance ouverte est gardée).

## ADR-057 — Description d'un outil MCP remplaçable par l'administrateur (complète ADR-025)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : le modèle choisit un outil d'après sa description. Celle d'un outil MCP
  est écrite par le serveur : bornée (1 000 caractères) et sans le jeton (ADR-026), elle
  reste le texte d'un tiers, qui peut être vague, trompeur ou porter une injection.
  ADR-025 laissait à faire la possibilité de la remplacer.
- **Décision** :
  - la politique d'un outil (`ToolOverride`) porte une `description` facultative,
    écrite par l'administrateur (`connector:manage`) avec le risque et les permissions ;
    le registre la donne au modèle à la place de celle du serveur. Le nom, le schéma
    d'entrée, le risque et les permissions de l'outil n'en dépendent pas ;
  - le modèle la lit toujours précédée du nom du serveur (`[Serveur MCP « préfixe »]`,
    `mcp_tool_description`), comme une description déclarée : ce nom est ajouté là où les
    runs appliquent les politiques (`scoped_overrides`). L'administration la garde et la
    montre telle qu'écrite ;
  - elle est bornée comme celle d'un serveur : 1 000 caractères, mesurés après retrait
    des espaces aux extrémités et passage de `\r\n` à `\n`. L'ensemble des
    remplacements d'un serveur est borné comme ses déclarations (256 Kio). Comme le
    contenu d'une compétence (ADR-052), elle ne contient aucun caractère invisible hormis
    le retour à la ligne et la tabulation (`plain_text::shows_all_it_holds`) : ce que lit
    l'administrateur est ce que lit le modèle, sans texte caché (caractères d'étiquette,
    espace sans largeur, forçage de sens…). Vide, c'est la description du serveur qui
    s'applique ;
  - le test de connexion montre les deux : celle du serveur et le remplacement
    (`replacedDescription`), que l'éditeur des règles d'outils reprend. La modification
    est tracée avec les politiques (`connector.updated`).
- **Conséquences** :
  - une description remplacée ne suit pas les changements du serveur : si celui-ci change
    le comportement de l'outil, l'administrateur doit la mettre à jour (le test de
    connexion montre les deux) ;
  - aucune autorisation n'en dépend : une description ne peut ni accorder une permission
    ni changer un risque ;
  - seuls les serveurs MCP ajoutés par l'entreprise sont concernés : les connecteurs
    réglés par l'exploitant n'acceptent pas de politiques depuis l'administration, et
    une description inscrite en base pour un autre type de connecteur est ignorée ;
  - les descriptions déclarées par les serveurs ne sont pas encore filtrées des
    caractères invisibles (dette, feuille de route) ;
  - tests :
    - `crates/api/tests/connector_descriptions.rs` : le modèle lit le remplacement
      précédé du nom du serveur, puis de nouveau la description du serveur une fois le
      remplacement retiré ; un remplacement hors bornes, ou qui cache du texte, est
      refusé et rien n'est enregistré ; une description inscrite pour le connecteur de
      démonstration n'est ni lue par le modèle ni montrée par le test de connexion ;
    - `services/admin/connector_rules.rs` : bornes, ensemble compris, et caractères
      invisibles ;
    - `apps/web/e2e/integrations.spec.ts` : description écrite depuis les règles d'un
      outil, enregistrée sans les espaces aux extrémités, rendue au serveur une fois
      vidée.

## ADR-058 — Caractères invisibles retirés des déclarations d'un serveur MCP (complète ADR-025 et ADR-057)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : ADR-057 refuse tout caractère invisible dans une description écrite par
  l'administrateur, mais laisse en dette celles que déclarent les serveurs, qui sont
  pourtant le texte réellement non fiable. Des caractères d'étiquette (U+E0000–E007F),
  des espaces sans largeur ou des forçages de sens peuvent y porter un texte que
  l'administrateur ne voit pas dans les règles d'outils et que le modèle lit à chaque
  run.
- **Décision** :
  - le titre d'un outil est ramené à une ligne visible ; sa description et la
    documentation de son schéma d'entrée (textes `description` et `title`, à toute
    profondeur) perdent leurs caractères invisibles, hormis le retour à la ligne et la
    tabulation (`plain_text::without_hidden`, la règle de `is_hidden` dans `core`) ;
  - ce retrait précède le masquage du jeton : un jeton coupé de caractères invisibles
    échapperait sinon au masque, puis apparaîtrait une fois les caractères retirés ;
  - ce que le serveur relit ne peut pas changer sans fausser ses appels : le nom d'un
    outil, et dans son schéma un nom de propriété ou une valeur permise, constante, par
    défaut ou d'exemple (y compris sous une clé `title` ou `description` à l'intérieur
    de ces valeurs). Un nom qui cache des caractères (y compris un retour à la ligne)
    fait refuser le serveur sans être cité, puisqu'il pourrait reformer un jeton coupé ;
    un schéma qui en cache ailleurs que dans sa documentation fait refuser le serveur,
    avec un message en français qui nomme l'outil (`mcp/visible.rs`) ;
  - un titre qui ne montre plus rien une fois nettoyé laisse place au titre suivant
    (annotations), puis au nom de l'outil.
- **Conséquences** :
  - le texte d'un serveur légitime peut perdre un trait d'union conditionnel ou la
    jonction d'un émoji composé : rien d'utile au choix d'un outil ;
  - des valeurs légitimes font refuser le serveur entier : une valeur permise contenant
    un retour chariot (`\r`) ou un émoji avec sélecteur de variante (U+FE0F), par
    exemple. Le message nomme l'outil, que l'éditeur du serveur peut corriger ;
  - les résultats des outils ne sont pas filtrés : ce sont des données non fiables,
    encadrées comme telles (`wrap_tool_result`), et les modifier altérerait ce que lit
    l'utilisateur (feuille de route) ;
  - tests :
    - `mcp/declarations.rs` : retrait avant le masquage, un jeton coupé de caractères
      invisibles restant masqué ; refus d'une valeur permise qui cache du texte, et d'un
      nom d'outil, sans le citer ; titre vide remplacé par le nom ;
    - `mcp/visible.rs` : documentation à toute profondeur, une propriété nommée
      `description` ou `default` (ou une définition nommée `enum`) restant un schéma
      dont la documentation est nettoyée ; refus d'un nom de propriété, d'une valeur
      permise, par défaut, constante ou d'exemple qui cache des caractères, même sous
      une clé de documentation ;
    - `crates/connectors/tests/mcp_failures.rs` : un serveur HTTP qui cache du texte
      dans ses déclarations n'en transmet rien.

## ADR-059 — Validation par un responsable (circuit hiérarchique, premier lot)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : jusqu'ici, seule la personne pour qui l'agent agit validait ses
  actions (MVP, `AGENT_ENGINE.md` §5). La phase 2 prévoit qu'un responsable disposant
  de `approval:decide` sur son périmètre les valide. La position hiérarchique
  n'accorde rien seule (`SECURITY.md`) : désigner un responsable ne doit pas lui
  donner de droit.
- **Décision** :
  - une entreprise choisit les risques dont les actions vont à un responsable
    (`approval_settings.manager_risks`, aucun par défaut : le comportement antérieur
    est conservé). Le réglage se fait dans l'administration (onglet « Circuit de
    validation ») ou par l'API (`GET`/`PUT /admin/approval-routing`), pour ceux qui
    gèrent les politiques d'accès (`policy:manage`, toute l'entreprise), et il est
    tracé avec la valeur remplacée (`approval_routing.updated`) ;
  - à la demande de validation, le responsable est le responsable (`lead_user_id`) de
    l'unité principale de la personne, sinon de l'unité la plus proche au-dessus, qui
    n'est pas cette personne, dont le compte est actif et qui détient
    `approval:decide` sur elle (ressource `approval` dont elle est propriétaire, dans
    son unité principale ; `core::approval_routing`). `approval:decide` est désormais
    vérifiée ressource par ressource : une politique de refus peut la viser ;
  - si personne n'est éligible, l'action est refusée aussitôt
    (`ApprovalPlacement::Unroutable`) : résultat d'erreur pour le modèle, appel
    journalisé comme refusé, audit `agent.approval.unroutable`. Aucune demande ne reste
    en attente d'un décideur inexistant ;
  - la demande garde qui la décide (`approvals.routing`, `approver_user_id`). Seul le
    responsable désigné l'approuve, et seulement s'il détient encore `approval:decide`
    sur la personne au moment de décider (403 sinon). Ce droit est vérifié juste avant
    l'enregistrement de la décision, hors de sa transaction : un droit retiré dans
    l'intervalle laisse passer cette décision, mais le run relit à chaque étape les
    droits de la personne, qui seuls permettent l'exécution ;
  - la personne pour qui l'agent agit ne peut pas approuver la demande (403 « attend
    la validation de votre responsable »), mais peut la **retirer**, tant qu'elle peut
    utiliser l'agent (le retrait le fait reprendre) : c'est un
    refus, sans risque puisque l'action n'est pas exécutée, tracé `approval.rejected`
    avec `withdrawn: true`. Ainsi, une demande dont le responsable ne peut plus
    décider (droit retiré, compte désactivé ou supprimé) ne bloque jamais la
    conversation. Toute autre personne reçoit 404 ;
  - le run reprend **avec les droits de la personne pour qui l'agent agit**, relus au
    moment de la décision, jamais avec ceux du décideur. Si son compte n'est plus actif,
    la décision est refusée (409) sans rien enregistrer. L'audit de la décision nomme
    le décideur et la personne (`on_behalf_of`) ;
  - `GET /approvals` liste les demandes faites pour la personne et celles qu'elle
    décide comme responsable. Chaque demande dit pour qui elle est faite
    (`requestedFor`), qui la décide (`approver`), qui l'a décidée (`decidedBy`), son
    échéance, si le lecteur peut la décider (`canDecide`, vérifié sur la personne
    comme à la décision) et s'il peut la retirer (`canWithdraw` : faite pour lui,
    confiée à un responsable, et il peut encore utiliser l'agent ; calculé par le
    serveur comme à la décision). Un responsable qui a perdu le droit ne voit plus les demandes
    en attente qu'il ne peut plus décider. L'interface n'affiche les boutons que si le
    lecteur peut décider, dit sinon qui doit décider, et propose à la personne de
    retirer sa demande. Le compteur de la barre latérale ne compte que ce que le
    lecteur peut décider. Le responsable voit l'action et ses paramètres exacts, pas
    l'exécution de la personne.
- **Conséquences** :
  - dans l'entreprise de démonstration, les actions de Lucas vont à Julien, celles de
    Julien et de Nadia à Marc, et celles de Marc ne trouvent personne (la directrice ne
    détient pas `approval:decide`) : elles sont refusées quand le réglage les vise ;
  - si un relais SMTP est configuré, le responsable est prévenu par e-mail (ADR-060) ;
  - une demande que personne ne décide avant son échéance expire d'elle-même (ADR-061) ;
  - le responsable est relancé la veille de l'échéance (ADR-062) ;
  - restent à faire : délégation en cas d'absence, personnes rattachées à plusieurs unités
    (seule l'unité principale compte) ;
  - tests :
    - `crates/core/src/approval_routing.rs` : responsable le plus proche détenant la
      permission, position seule ignorée ;
    - `crates/agent/tests/runtime_approvals.rs` : refus immédiat d'une action que
      personne ne peut valider ;
    - `crates/api/tests/manager_approvals.rs` : seul le responsable décide (un tiers ne
      peut ni décider ni retirer : 404), reprise pour la personne, responsable privé de
      son droit refusé (et ne voyant plus la demande), retrait par la personne d'une
      demande que plus personne ne peut décider (impossible sans `agent:use`),
      responsable désactivé ignoré, refus quand personne n'est éligible, réglage
      réservé à `policy:manage` ;
    - `apps/web/e2e/manager-approvals.spec.ts`, contre `e2e_server` : réglage choisi
      dans l'administration et relu après rechargement ; demande de Nadia refusée par
      Marc, relue après rechargement ; demande de Nadia retirée par elle alors que Marc
      pouvait encore la décider, relue chez elle et chez Marc. Les états que la
      démonstration n'atteint pas (plus aucun décideur, demande confiée à la
      directrice) sont affichés à partir d'une réponse simulée de l'API, sans clic.

## ADR-060 — Le responsable prévenu par e-mail d'une demande qui l'attend (complète ADR-059)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : depuis ADR-059, une demande confiée à un responsable n'apparaît que dans
  sa page « Validations » et le compteur de la barre latérale. Tant qu'il ne se connecte
  pas, l'agent de la personne reste en pause. Un e-mail le prévient, mais il sort de la
  plateforme : il échappe à ses contrôles d'accès et à sa durée de conservation.
- **Décision** :
  - quand un run se met en pause sur des demandes confiées à un responsable, un e-mail
    lui est envoyé en arrière-plan, une fois la pause enregistrée (les demandes sont
    alors décidables) : **un seul par responsable et par pause**, quel que soit le
    nombre de demandes de l'étape. Le run est en pause, que le message parte ou non.
    Seules comptent les demandes qui attendent encore ce responsable, dont le compte est
    actif, dans un run toujours en pause d'une entreprise active, et qui n'ont pas déjà
    été signalées ; une demande que la personne décide
    elle-même n'est jamais envoyée ;
  - le message dit pour qui sont les actions, combien, leurs risques, le lien vers la
    page des validations et leur échéance (la plus proche ; celles d'une étape sont les mêmes à la minute). Il ne contient **ni les actions ni
    leurs paramètres** : ils ne se lisent qu'après connexion, là où le droit de décider
    est vérifié ;
  - chaque demande signalée est tracée (`approval.notified`, réussi ou échoué, au nom
    de la plateforme pour la personne, avec l'identifiant du responsable et le nombre
    de demandes du message), sans l'adresse ni la réponse du relais, qui peut la citer ;
    un échec est journalisé sans elles, et n'est pas réessayé ;
  - sans relais SMTP configuré, rien n'est envoyé. L'écran « Circuit de validation »
    le dit (`notifiesByEmail`), et le responsable trouve les demandes dans
    « Validations ».
- **Conséquences** :
  - aucune donnée personnelle nouvelle n'est conservée : l'e-mail n'est pas stocké, et
    l'audit ne garde que des identifiants ;
  - un avis que l'arrêt du service interrompt entre l'enregistrement de la pause et
    l'envoi est rattrapé au démarrage suivant (ADR-070) ;
  - pendant une absence, l'avis va au remplaçant que le responsable a désigné
    (ADR-072) ;
  - le responsable est relancé la veille de l'échéance (ADR-062) ; pas de résumé
    périodique. Ce qu'un modèle détourné peut faire envoyer reste borné : au plus deux
    messages par pause et par responsable (l'avis, puis la relance), et autant de
    pauses que son budget d'étapes en permet ;
  - tests :
    - `crates/api/src/services/runs/approval_notice/message.rs` : contenu du message ;
    - `crates/api/tests/manager_notices.rs` : seul le responsable est prévenu, sans
      l'action ni ses paramètres, envoi tracé ; les demandes d'une même pause font un
      seul message, et une demande signalée ne l'est plus ; rien n'est envoyé pour un
      run terminé avant d'avoir enregistré sa pause ; un message refusé par le relais est
      tracé en échec et la demande attend toujours ; l'administration dit si les
      responsables sont prévenus ;
    - `apps/web/e2e/manager-approvals.spec.ts` : Marc ouvre le lien de l'e-mail reçu
      pour décider de la demande de Nadia.

## ADR-061 — Expiration active des demandes de validation

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : une demande de validation expire 7 jours après sa création
  (`expires_at`). Jusqu'ici, l'échéance n'était constatée que lorsque quelqu'un tentait
  de décider : une demande que personne ne décidait laissait le run en pause pour
  toujours, la conversation bloquée, et la demande listée « en attente ».
- **Décision** :
  - chaque minute, la maintenance périodique cherche les demandes en attente dont
    l'échéance est passée. Elle passe par une fonction `SECURITY DEFINER` minimale
    (`system_overdue_approval_runs`) qui ne rend que l'entreprise et le run ; chaque run
    est ensuite traité dans sa propre entreprise (RLS) ;
  - seul le processus qui exécute les runs (celui qui détient le verrou de l'exécuteur,
    ADR-053) le fait. Sous le verrou du run, comme une décision, chaque demande échue
    devient `expired`, tracée `approval.expired` au nom de la plateforme pour la
    personne (`failure`, raison « Délai de validation dépassé », outil, risque et
    circuit) ;
  - quand plus aucune demande du run n'attend, le run reprend comme après une décision :
    avec les droits de la personne relus à cet instant, l'agent apprenant que l'action
    n'a pas été exécutée. Si son compte n'est plus actif, le run se termine en échec
    (« compte inactif »). Tant qu'une autre demande du run peut encore être décidée, il
    reste en pause ;
  - les droits de la personne sont lus avant d'enregistrer quoi que ce soit : un échec
    ne change rien, et le balayage suivant réessaie. Une décision relit la demande sous
    le verrou du run : celle qu'une expiration a réglée entre-temps répond « déjà
    traitée ». La règle de reprise (`settle`, puis `resume`) est la même que pour une
    décision. À l'arrêt du service, la maintenance est attendue avant les runs ;
  - la page des validations affiche l'échéance de chaque demande en attente.
- **Conséquences** :
  - aucune conversation ne reste bloquée plus de 7 jours (et une minute) par une demande
    sans décision ;
  - quand le compte de la personne n'est plus actif, le run se termine sans enregistrer
    l'issue de l'appel qui attendait (comme la reprise au démarrage, ADR-053) ; à
    harmoniser ;
  - la durée de 7 jours n'est pas encore réglable par entreprise ; le responsable est
    relancé la veille (ADR-062), pas la personne qui décide ses propres demandes ;
  - tests :
    - `crates/api/tests/approval_expiry.rs` : une demande échue expire et le run poursuit
      sans l'action, une seule fois ; seul le processus qui exécute les runs expire ; un
      run attend tant qu'une autre de ses demandes peut être décidée ; le run d'un compte
      inactif se termine ; chaque entreprise voit ses demandes expirer chez elle et rien
      de celles de l'autre ; une décision qui croise l'expiration trouve la demande
      réglée (course rendue déterministe par les verrous) ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue, au chemin de recherche fixe, appelable par l'application seule ;
    - `apps/web/e2e/manager-approvals.spec.ts` : l'échéance affichée.

## ADR-062 — Relance du responsable la veille de l'échéance (complète ADR-060 et ADR-061)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : une demande confiée à un responsable expire 7 jours après sa création
  (ADR-061). Le responsable n'est prévenu qu'une fois, quand le run se met en pause
  (ADR-060) : un e-mail oublié laisse la demande expirer, et l'agent poursuivre sans
  l'action, sans que personne ne l'ait regardée.
- **Décision** :
  - chaque minute, le processus qui exécute les runs (verrou de l'exécuteur, ADR-053)
    cherche les demandes en attente, confiées à un responsable, qui expirent dans les
    24 heures et dont il n'a pas encore été relancé, dans un run **en pause** d'une
    entreprise **active** (un run terminé sans avoir enregistré sa pause laisse des
    demandes que personne ne peut plus décider : elles ne sont jamais relancées). Il passe par une fonction
    `SECURITY DEFINER` minimale (`system_approval_reminder_runs`), qui ne rend que
    l'entreprise et le run ; chaque run est ensuite traité dans sa propre entreprise ;
  - le responsable reçoit **une seule relance par demande**, groupée par run comme
    l'avis : « Rappel : … attend votre validation », avec l'échéance, sans l'action ni
    ses paramètres ; seulement si son compte est actif ;
  - chaque demande relancée est tracée (`approval.reminded`,
    réussi ou échoué, au nom de la plateforme pour la personne, avec l'identifiant du
    responsable et le nombre de demandes du message) ; une relance refusée par le relais
    n'est pas réessayée. Si l'écriture de cette trace échoue après l'envoi, la relance
    repart la minute suivante : la garantie est « au moins une fois » dans ce cas ;
  - l'échec d'un run n'empêche pas de relancer les autres ; les envois se font l'un
    après l'autre dans la maintenance, qu'ils peuvent retarder de quelques secondes ;
  - sans relais SMTP, rien n'est envoyé ni tracé. L'écran « Circuit de validation »
    l'indique.
- **Conséquences** :
  - avec la validité actuelle de 7 jours, la relance part au sixième jour, 24 heures
    avant l'échéance (à la minute près, selon le passage de la maintenance) ;
  - la personne qui décide ses propres demandes n'est pas relancée : elle les voit dans
    sa conversation ;
  - tests :
    - `crates/api/src/services/runs/approval_notice/message.rs` : texte de la relance ;
    - `crates/api/tests/approval_reminders.rs` : relance unique la veille, par le seul
      processus qui exécute les runs, pas une semaine avant ; les demandes d'une pause
      relancées ensemble ; pas de relance à un responsable désactivé ; une relance
      refusée par le relais tracée en échec et jamais renvoyée ; ni demande de la
      personne, ni demande échue, ni demande d'un run terminé avant sa pause ; rien sans
      relais ; pas de relance dans une entreprise suspendue ; chaque entreprise relancée
      chez elle et rien de l'autre visible ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue.

## ADR-063 — Connexion par le fournisseur d'identité de l'entreprise (SSO OpenID Connect, Microsoft 365 d'abord)

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : les entreprises clientes veulent que leurs collaborateurs se
  connectent avec le compte qu'ils ont déjà : d'abord Microsoft 365 (Entra ID), aussi
  Google Workspace ou un autre fournisseur d'identité (Okta, Keycloak…). Le mot de
  passe Mimorya reste possible. Plusieurs méthodes doivent coexister, par entreprise.
- **Décision** :
  - protocole unique : **OpenID Connect**, flux par code d'autorisation avec PKCE,
    `state` et `nonce`, côté serveur (client confidentiel). Bibliothèque `openidconnect`
    (découverte, clés de signature, validation du jeton d'identité : signature,
    émetteur, audience, expiration, nonce), branchée sur le client HTTP durci des
    connecteurs (pas de redirection, pas de proxy, noms résolus en adresses publiques
    seulement sauf réseau privé autorisé par l'exploitant, ADR-025), chaque échange
    borné en durée (10 s) et en taille (256 Kio). **Toute adresse appelée** — l'émetteur
    enregistré, mais aussi les clés et le point d'échange que nomme la déclaration du
    fournisseur, et l'adresse d'autorisation où le navigateur est envoyé — est contrôlée
    avant la connexion : `https` (sauf réseau privé autorisé), sans identifiants, et une
    adresse IP écrite en clair doit être publique (le résolveur ne la voit pas) ;
  - chaque entreprise déclare ses **fournisseurs d'identité** (`identity_providers`,
    RLS) : type `microsoft` (identifiant de l'annuaire Entra, un GUID ; émetteur
    `https://login.microsoftonline.com/{annuaire}/v2.0`), `google` (émetteur Google ;
    seuls les comptes Workspace d'un domaine déclaré, revendication `hd`) ou `oidc`
    (adresse d'émetteur quelconque, nom obligatoire), avec identifiant client, secret
    (coffre des secrets, ADR-024, jamais relu), **domaines d'e-mail** couverts (un
    fournisseur par domaine dans l'entreprise) et état actif. Type, émetteur et
    identifiant client ne changent pas : on déclare un autre fournisseur ;
  - un fournisseur ne sert **que les comptes de son entreprise**. La page de connexion
    demande l'e-mail ; le fournisseur proposé est celui qui couvre le domaine du
    compte portant cette adresse, actif, dans une entreprise active
    (`auth_find_identity_provider`, seule lecture inter-entreprises). Une adresse sans
    compte, d'un compte désactivé ou invité, ou d'une entreprise sans fournisseur reçoit
    la même réponse (« mot de passe ») ; une entreprise ne peut donc pas détourner vers son
    fournisseur les comptes d'une autre en déclarant leur domaine. La réponse dit
    qu'une adresse a un compte dans une entreprise qui l'a reliée à son fournisseur.
    La découverte et le départ disent tous deux si une adresse a un compte relié à un
    fournisseur (le départ répond 404 sinon) : les adresses ainsi essayées ont leur
    propre limite par adresse cliente, celle de la connexion, quelle que soit l'étape,
    et redemander une adresse déjà comptée ne coûte rien (la page la redemande, son
    départ suit ; seule son empreinte est gardée, le temps de la fenêtre). Le départ et
    le retour ensemble ont en plus la leur, deux fois celle de la connexion (une
    connexion SSO compte les deux). Aucune ne retire rien à la limite des mots de passe
    (la découverte que la page de connexion fait pour toute adresse non plus), et un
    retour rejoué avec sa tentative scellée, valable dix minutes, est refusé au-delà,
    sans trace ni appel au fournisseur. Au-delà, la page de connexion le dit, le mot de
    passe restant possible. Une première version donnait aux trois étapes une seule
    limite, trois fois celle de la connexion : on pouvait y essayer trois fois plus
    d'adresses que de mots de passe ;
  - le départ (`POST /api/auth/sso/start`) renvoie l'adresse du fournisseur et pose un
    cookie de tentative (HttpOnly, SameSite=Lax, chemin `/api/auth/sso`, 10 minutes) :
    entreprise en clair, fournisseur, `state`, `nonce` et vérificateur PKCE scellés par
    la clé maîtresse (AES-256-GCM, l'entreprise en données associées). Rien n'est écrit
    en base ; le code n'est échangeable qu'une fois chez le fournisseur ;
  - **pas de création de compte** (remplacé par ADR-067 : par le lien de l'entreprise, un
    fournisseur peut créer le compte) : le compte doit exister et être actif (ou invité :
    ADR-064). La première
    connexion le relie à l'identité (`user_identities` : fournisseur, sujet immuable —
    `oid` pour Microsoft, qui survit à un changement d'application, `sub` sinon) par
    l'e-mail du jeton, seulement si le fournisseur garantit cette adresse et qu'elle
    relève d'un domaine déclaré. Pour Microsoft, le nom de connexion (UPN, dont
    l'annuaire a vérifié le domaine), sinon `email` seulement si Microsoft garantit son
    domaine (`xms_edov`) : `email` seul peut ne pas être vérifié ; pour les autres,
    `email` avec `email_verified`. Ensuite,
    seule l'identité reliée compte ; un compte déjà relié à une autre identité du même
    fournisseur est refusé. La personne connectée est celle que le fournisseur prouve,
    pas celle que l'adresse saisie désignait ;
  - le retour du fournisseur (`GET /api/auth/sso/callback`) vérifie d'abord le `state`
    contre le cookie (contre la connexion forcée ; un refus du fournisseur le porte
    aussi), efface le cookie, échange le code, valide
    le jeton, ouvre la session comme une connexion par mot de passe (ligne du compte
    verrouillée, compte actif) et la trace (`auth.login`, méthode `sso`, fournisseur ;
    `auth.identity_linked` à la première liaison). Tout échec renvoie à
    `/login?sso=echec` ; la raison est tracée dans l'entreprise de la tentative quand
    elle est connue, jamais un jeton, un code ni la réponse du fournisseur à l'échange
    (de son refus, seul son code d'erreur, filtré) ;
  - la configuration (`/api/admin/identity-providers`) exige `settings:manage` sur
    toute l'entreprise (non délégable), sous le verrou d'administration, auditée (sans
    le secret) ; le fournisseur est joint avant l'enregistrement pour dire tout de suite
    ce qui ne va pas ; sa suppression emporte domaines, secret et identités reliées.
- **Conséquences** :
  - le serveur doit joindre le fournisseur (`login.microsoftonline.com`, …) ; pour un
    fournisseur du réseau privé, l'exploitant l'autorise comme pour les connecteurs
    (`MIMORYA_MCP_ALLOW_PRIVATE_NETWORK`). La déclaration et les clés du fournisseur,
    d'abord relues au départ et au retour de chaque connexion, sont gardées une heure
    (ADR-065) ;
  - qui administre les fournisseurs peut se connecter comme les comptes de ses domaines
    pas encore reliés ; c'est déjà le pouvoir de qui fixe un mot de passe provisoire
    (même niveau d'administration, tracé) ;
  - un compte invité accepte d'abord son invitation (remplacé par ADR-064 : il peut
    l'accepter par le fournisseur) ; un mot de passe provisoire reste à remplacer, même
    après une connexion par le fournisseur ;
  - la connexion par mot de passe reste ouverte ; la rendre impossible pour les
    domaines couverts (fait : ADR-066), accepter une invitation par le fournisseur (fait :
    ADR-064), la
    création de compte à la première connexion (fait : ADR-067), SAML (fait : ADR-068) et
    la vérification de propriété d'un domaine viendront ensuite ;
  - lots : (1) modèle, connexion, configuration par l'API, tests contre un fournisseur
    simulé ; (2) onglet « Connexion » de l'administration (`settings:manage` : déclarer,
    modifier, désactiver, supprimer ; adresse de retour à enregistrer chez le
    fournisseur) et, sur la page de connexion, le bouton « Se connecter avec … » proposé
    d'après l'adresse quand on quitte le champ, et un message générique au retour d'un
    échec (`?sso=echec`) ; (3) options. Les tests de bout en bout passent par le
    fournisseur OpenID Connect du serveur e2e (127.0.0.1:4100, il connecte la personne
    que la connexion désigne), seul joint sur le réseau privé (`BuildOptions::sso_network`,
    réservé aux tests) : les connecteurs y restent sur le réseau public. Toutes ses
    requêtes passant par le relais web, d'une seule adresse, la limite globale par
    adresse y est relevée (`BuildOptions::global_rate_limit_per_minute`, réservé aux
    tests).
  - Tests :
    - `crates/core/src/identity_providers/tests.rs` : issuer de chaque type, nom,
      client, domaines, preuves d'identité par type ;
    - `crates/api/src/services/sso/attempt.rs` : tentative scellée, ouverte seulement
      pour son entreprise, intacte et à temps ; `address.rs` et `http.rs` : adresses
      hors politique refusées, sans aucune connexion ;
    - `crates/api/tests/sso_sign_in.rs`, `sso_refusals.rs`, `sso_companies.rs`,
      `sso_limits.rs`, `admin_identity_providers.rs` : connexion, liaison unique, limites
      propres aux étapes (adresses essayées par la découverte ou le départ ; départ et
      retour ensemble ; `crates/api/src/plugins/distinct_limit.rs` : une adresse comptée
      une fois par fenêtre), refus (dont un audit sans code ni jeton ; le compte invité,
      d'abord refusé, est couvert par `sso_invitations.rs`, ADR-064), isolation entre
      entreprises, fournisseur
      hors politique réseau jamais joint, configuration ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue ;
    - `apps/web/e2e/single-sign-on.spec.ts` : déclaration dans l'administration (secret
      jamais affiché, relu après rechargement), modification sans ressaisir le secret
      (conservé), connexion de bout en bout par le bouton, fournisseur désactivé proposé à
      personne, suppression, retour en échec signalé, découverte refusée au-delà de sa
      limite signalée puis redemandée une fois la minute passée.

## ADR-064 — Accepter une invitation par le fournisseur d'identité de l'entreprise

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : dans une entreprise qui se connecte par Microsoft 365 (ADR-063), un compte
  créé par l'administration attend que la personne choisisse un mot de passe par le lien
  de son invitation, mot de passe dont elle n'a pas besoin.
- **Décision** :
  - un compte **invité** se voit proposer le fournisseur de son entreprise comme un compte
    actif (`auth_find_identity_provider`, migration 0018), sur la page de connexion et sur
    celle de son invitation (« Activer mon compte avec … ») ;
  - au retour, les règles d'ADR-063 s'appliquent (adresse garantie par le fournisseur, dans
    un domaine déclaré ; liaison au sujet) ; le compte invité devient alors **actif sans
    mot de passe**, son invitation est supprimée (son lien ne vaut plus rien), et
    l'acceptation est tracée (`user.invitation_accepted`, méthode `sso`, fournisseur),
    dans la même transaction que la session ouverte ;
  - le fournisseur garantit l'adresse à laquelle l'invitation a été envoyée : c'est la même
    preuve que l'ouverture du lien, sans l'échéance du lien, qui ne protégeait que contre
    sa fuite ;
  - un compte désactivé n'est proposé à aucun fournisseur ;
  - un compte garde la trace d'avoir été actif (`users.activated`, migration 0018 : son
    invitation acceptée, par le lien ou par le fournisseur, ou un mot de passe provisoire
    reçu, à sa création ou de l'administration, même désactivé). Désactivé puis réactivé, seul un compte qui ne l'a jamais été
    redevient `invited` (ADR-027) : avant, l'absence de mot de passe en tenait lieu ;
  - le lien et le fournisseur verrouillent la même ligne du compte avant toute écriture
    (le fournisseur avant même de relier l'identité) : le premier l'accepte, l'autre
    trouve un compte actif (le lien est alors refusé, le fournisseur ouvre une simple
    session) ; un renvoi d'invitation qui passe après est refusé, celui qui passe avant
    voit son lien supprimé par l'acceptation.
- **Conséquences** :
  - un compte accepté ainsi n'a pas de mot de passe : il ne se connecte que par le
    fournisseur, et « mot de passe oublié » ne lui envoie rien ; l'administration peut lui
    fixer un mot de passe provisoire. « Mon compte » le lui dit au lieu de proposer de
    changer un mot de passe (`hasPassword` dans `/auth/me`) ; un changement demandé quand
    même est refusé (409) sans compter comme un essai de mot de passe ;
  - qui administre les fournisseurs (`settings:manage`, non délégable) peut désormais
    se connecter aussi comme les comptes **invités** de ses domaines, rôles déjà
    attribués compris, alors qu'une invitation envoyée par e-mail ne lui était pas
    montrée. C'est le pouvoir qu'ADR-063 lui reconnaît déjà sur les comptes actifs pas
    encore reliés, au même niveau d'administration ; l'acceptation est tracée
    (`auth.identity_linked`, `user.invitation_accepted`) ;
  - tests : `crates/api/tests/sso_invitations.rs` (acceptation, invitation supprimée, lien
    refusé, sans mot de passe, trace unique ; acceptée après l'échéance du lien ; refusée
    sans adresse garantie ; rien pour un compte désactivé ; réactivé, il reste actif et
    l'administration peut lui fixer un mot de passe provisoire ; « Mon compte » sans
    mot de passe), `sso_invitation_races.rs` (lien contre fournisseur dans les deux
    ordres, renvoi contre fournisseur), `sso_companies.rs` (le compte invité d'une
    autre entreprise reste hors d'atteinte), `invitations.rs` (un mot de passe provisoire
    donné pendant la désactivation rend le compte actif une fois réactivé) ;
    `apps/web/e2e/single-sign-on.spec.ts`
    (invitation reçue par e-mail, compte activé par le fournisseur, « Mon compte » sans
    mot de passe, lien refusé ensuite).

## ADR-065 — Garder en mémoire la déclaration et les clés des fournisseurs d'identité

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : chaque connexion par le fournisseur d'identité (ADR-063) relisait sa
  déclaration (`/.well-known/openid-configuration`) et ses clés publiques au départ, puis
  encore au retour : quatre requêtes vers Microsoft ou Google avant même l'échange du
  code, une latence et une dépendance de plus à chaque connexion. Les fournisseurs
  demandent de garder ces documents et de ne relire les clés qu'à leur renouvellement.
- **Décision** :
  - la déclaration de chaque fournisseur déclaré, clés comprises, est gardée **une heure**
    en mémoire (`crates/api/src/services/sso/metadata.rs`), par instance du serveur,
    comme les limites de débit : le départ et le retour la lisent là. Elle est gardée
    **par fournisseur**, donc par entreprise, même quand deux entreprises déclarent le
    même émetteur (Google, ou l'annuaire Microsoft d'une autre) : ni le contenu, ni la
    présence en mémoire, qui se verrait au temps de réponse, ne passent de l'une à
    l'autre ;
  - un jeton dont la signature échoue avec les clés gardées (le fournisseur les a
    renouvelées, que ses jetons portent un identifiant de clé ou non) fait relire la
    déclaration **une fois**, puis le même jeton est vérifié avec les nouvelles clés,
    sans nouvel échange du code ; s'il échoue encore, il est refusé (« clé de signature
    inconnue », ou la signature refusée). Un refus pour une autre raison (audience,
    nonce, expiration) ne relit rien. Le jeton ne vient que de l'échange de serveur à serveur, après la tentative scellée : au pire deux lectures de la déclaration par connexion, autant qu'avant ;
  - un échec n'est pas gardé : la connexion suivante relit le fournisseur ;
  - la vérification d'un émetteur par l'administration (déclaration d'un fournisseur)
    le relit toujours, et ne garde rien : le fournisseur n'est pas encore déclaré ;
  - au plus 256 fournisseurs gardés : au-delà, les déclarations périmées partent
    d'abord, puis la plus ancienne lue ; une déclaration de plus de 64 Kio, clés
    comprises, n'est pas gardée (relue à chaque connexion, comme avant). Chaque adresse
    qu'elle nomme reste contrôlée avant chaque requête (ADR-025).
- **Conséquences** :
  - une connexion ne joint plus le fournisseur que pour échanger son code ; un
    changement de ses adresses (rare) est vu au plus tard une heure après, ses clés
    renouvelées dès le premier jeton qu'elles signent ; une clé retirée par le
    fournisseur (compromise) reste acceptée au plus une heure, pour des jetons qui ne
    viennent que de son point d'échange, joint en TLS ;
  - une entreprise qui déclare de nombreux fournisseurs et s'y connecte peut faire
    sortir de la mémoire ceux des autres, qui relisent alors leur fournisseur à chaque
    connexion, comme avant : rien n'est refusé ;
  - un serveur redémarré relit chaque fournisseur à sa première connexion ;
  - tests : `crates/api/src/services/sso/metadata.rs` (durée, bornes, périmés d'abord) ;
    `crates/api/tests/sso_provider_cache.rs` (connexions sans relecture, la vérification
    par l'administration ne gardant rien ; clés renouvelées relues une fois, avec ou sans
    identifiant ; refus d'une autre nature sans relecture ; clé inconnue refusée après
    une seule relecture, et sans double relecture d'une déclaration tout juste lue ;
    échec non gardé, fournisseur gardé tant qu'il sert ; déclaration trop lourde relue ;
    deux entreprises au même émetteur gardées à part ; déclaration par l'administration
    toujours relue).

## ADR-066 — Rendre la connexion par le fournisseur d'identité obligatoire pour ses domaines

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : une entreprise qui se connecte par Microsoft 365 (ADR-063) veut que ses
  collaborateurs passent tous par lui (MFA, accès conditionnel, départ géré dans
  l'annuaire) : un mot de passe gardé dans Mimorya contournerait ces règles.
- **Décision** :
  - un fournisseur peut être déclaré **obligatoire** pour ses domaines
    (`identity_providers.required`, migration 0019), à sa déclaration ou ensuite, par
    l'administration (`settings:manage`, onglet « Connexion », tracé avec la valeur
    précédente) ;
  - tant qu'il est actif et obligatoire, les comptes de ses domaines :
    - ne se connectent plus par mot de passe : le refus (403, le fournisseur nommé) n'est
      dit qu'à qui donne le bon mot de passe, ne compte pas comme un échec et est tracé
      (« Fournisseur d'identité obligatoire ») ; un mauvais mot de passe reçoit la réponse
      habituelle ;
    - n'acceptent leur invitation que par le fournisseur (ADR-064), pas en choisissant un
      mot de passe ;
    - ne reçoivent aucun lien de réinitialisation, ni demandé (« mot de passe oublié »,
      réponse habituelle, rien n'est envoyé) ni envoyé par l'administration (409), ne
      peuvent plus utiliser un lien reçu avant (403, aucun mot de passe fixé ni session
      ouverte) et ne reçoivent pas de mot de passe provisoire (409) : aucun chemin ne
      leur rend un mot de passe ;
  - les **administrateurs de la connexion** (`settings:manage` sur toute l'entreprise,
    ceux qui déclarent les fournisseurs) gardent leur mot de passe : un fournisseur en
    panne ou mal configuré ne bloque jamais l'entreprise (comme les super-administrateurs
    de Google Workspace). Un compte invité n'a pas encore de droits : il n'en est pas ;
  - la découverte dit si le fournisseur est obligatoire (`required`) : la page de
    connexion ne propose plus que lui, et un lien « Administrateur de la connexion : se
    connecter par mot de passe » ; la page d'invitation ne propose plus de mot de passe ;
  - la règle se lit dans l'entreprise du compte (RLS) : le fournisseur d'une autre
    entreprise, même obligatoire pour le même domaine, ne touche à aucun de ses comptes.
- **Conséquences** :
  - les sessions ouvertes par mot de passe avant que le fournisseur devienne obligatoire
    restent valables jusqu'à leur échéance ;
  - le mot de passe d'un compte n'est pas effacé : si le fournisseur cesse d'être
    obligatoire ou est désactivé, le compte retrouve celui qu'il avait ;
  - la règle se lit à chaque demande, avant tout verrou, dans sa propre transaction
    (`services/sso/requirement.rs`) ;
  - tests : `crates/api/tests/sso_required.rs` (mot de passe refusé sans compter d'échec,
    tracé ; mauvais mot de passe sans indice ; connexion par le fournisseur ; pas de lien
    de réinitialisation ; administrateur de la connexion exempté ; plus obligatoire ou
    désactivé : mot de passe rendu ; changement tracé avec la valeur précédente ;
    invitation acceptée par le fournisseur seulement ; lien demandé avant inutilisable,
    ni lien ni mot de passe provisoire de l'administration ; fournisseur obligatoire
    d'une autre entreprise sans effet) ;
    `apps/web/e2e/single-sign-on.spec.ts` (case cochée dans « Connexion », relue après
    rechargement ; page de connexion réduite au fournisseur, mot de passe refusé à
    Nadia, accepté à Paul ; invitation sans mot de passe).

## ADR-067 — Créer le compte à la première connexion par le lien de l'entreprise

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : une entreprise qui se connecte par Microsoft 365 (ADR-063) ne veut pas
  inviter un à un ses collaborateurs : l'annuaire les connaît déjà. Mais les domaines
  qu'elle déclare ne sont pas vérifiés (rien ne prouve qu'elle possède
  `entreprise.fr`), et deux entreprises peuvent déclarer le même : créer un compte
  d'après la seule adresse saisie sur la page de connexion enverrait la personne au
  fournisseur de qui a déclaré son domaine, peut-être pas son entreprise.
- **Décision** :
  - un fournisseur peut **créer les comptes** (`identity_providers.create_accounts`,
    migration 0020), à sa déclaration ou ensuite, par l'administration
    (`settings:manage`, onglet « Connexion », tracé avec la valeur précédente) ;
  - la création ne part jamais d'une adresse : chaque fournisseur a un **lien de
    connexion de l'entreprise** (`/login/entreprise#<id du fournisseur>`, affiché et
    copiable dans « Connexion ») que l'entreprise diffuse elle-même. La page lit le
    fournisseur (`POST /api/auth/sso/link`, fonction `auth_find_identity_provider_link` :
    actif, entreprise active ; autant par minute que les connexions) et affiche le nom de
    l'entreprise et du fournisseur ; le départ (`POST /api/auth/sso/link/start`, compté
    avec les connexions par fournisseur) n'indique aucune adresse au fournisseur ;
  - la tentative scellée (ADR-063) garde l'origine de la connexion : au retour, le compte
    est celui relié au sujet, sinon celui de l'entreprise ayant l'adresse garantie
    (ADR-063) ; à défaut, si le fournisseur crée les comptes **et que la connexion est
    partie du lien**, un compte est créé dans son entreprise : **actif,
    sans mot de passe, sans rôle ni unité**, habilitation par défaut (`internal`), relié
    au sujet. Son nom est celui que donne le jeton (`name`, sinon prénom et nom, sinon la
    partie locale de l'adresse), sur une ligne visible : forme canonique, caractères
    cachés retirés, espaces repliés, 120 caractères au plus. La création est tracée
    (`user.created`, méthode `sso`, fournisseur) ;
  - mêmes garde-fous qu'une liaison : adresse garantie par le fournisseur et dans un de
    ses domaines ; une adresse déjà prise, dans cette entreprise ou une autre (unique
    partout, ADR-011), n'est jamais reprise : refus tracé « Adresse déjà utilisée par un
    autre compte », sans rien créer ni relier. Une connexion partie d'une adresse (page
    de connexion, invitation) ne crée jamais de compte, même si la personne se connecte
    chez le fournisseur sous une autre identité que celle suggérée : refus « Aucun compte
    de l'entreprise pour cette adresse » ;
  - le type « OpenID Connect » refuse les émetteurs de Google et de Microsoft : il ne
    prouve une adresse que par `email_verified`, qu'un compte Google grand public ou un
    autre annuaire satisferait ; leurs types propres vérifient le domaine Workspace ou le
    nom de connexion de l'annuaire. Un fournisseur ainsi déclaré auparavant reste en
    place (à redéclarer avec son type) mais ne peut pas créer les comptes ; les clouds
    nationaux de Microsoft (`login.microsoftonline.us`, Chine) sont traités de même ;
- **Conséquences** :
  - un compte créé ne peut rien tant qu'un administrateur ne lui a pas donné de rôle ;
    l'annuaire le montre comme tout autre compte, et il suit leurs règles : désactivation,
    rétention, export et suppression des données personnelles (SECURITY §9, DATA_MODEL
    §5) ; son nom vient de l'annuaire de l'entreprise, qui en est la source ;
  - une connexion qui crée un compte ne le crée qu'une fois : deux connexions simultanées
    de la même personne, la seconde est refusée (adresse prise) et la suivante aboutit ;
  - la découverte par l'adresse ne crée rien : elle ne propose un fournisseur qu'à un
    compte existant. La vérification de propriété d'un domaine (enregistrement DNS)
    permettra plus tard de proposer la création depuis l'adresse ;
  - le lien nomme l'entreprise : qui le reçoit d'un tiers voit chez qui il se connecte
    avant d'aller chez le fournisseur ;
  - tests : `crates/api/tests/sso_accounts.rs` (compte créé actif, sans mot de passe ni
    rôle, nommé par le jeton, tracé, retrouvé à la connexion suivante, par le lien ou
    l'adresse ; rien pour une connexion partie d'une adresse ; rien sans le réglage, puis créé une fois réglé, nommé par prénom et nom ; jamais hors des domaines,
    sans adresse garantie ni sur l'adresse d'une autre entreprise ; lien d'un fournisseur
    inconnu, désactivé ou d'une entreprise suspendue : 404 ; limites) ; règle du nom :
    `crates/core/src/identity_providers/display_name.rs` ;
    `apps/web/e2e/single-sign-on.spec.ts` (case cochée dans « Connexion », relue après
    rechargement ; lien affiché, ouvert par un nouveau venu, compte créé à son nom et
    relu dans l'annuaire).

## ADR-068 — Prendre en charge les fournisseurs d'identité SAML 2.0

- **Date** : 2026-10-21 · **Statut** : acceptée
- **Contexte** : beaucoup d'entreprises connectent leurs applications par SAML 2.0 (AD FS,
  applications SAML d'Entra ID, Okta, Shibboleth…), pas par OpenID Connect (ADR-063). La
  vérification d'une réponse SAML est délicate (signatures XML, attaques par
  enveloppement) ; les bibliothèques Rust courantes (`samael`) s'appuient sur libxml2 et
  xmlsec, en C.
- **Décision** :
  - bibliothèque **`saml-rs` 0.6.1**, en Rust pur (`unsafe` interdit), dont les signatures
    XML passent par `bergshamra` sur RustCrypto (fonctionnalité `crypto-rustcrypto`
    seule : ni PKCS#11, ni algorithmes post-quantiques ou anciens). Elle vérifie la
    signature avec les seuls certificats des métadonnées déclarées, n'extrait que le
    contenu couvert par une référence vérifiée et refuse les enveloppements connus. Elle
    est en version 0.x, sans audit externe : version épinglée exactement (`=0.6.1`), usage
    isolé dans `services/sso/saml/`, politique de validation stricte et refus testés un
    à un, enveloppement compris ;
  - un fournisseur de type **`saml`** se déclare par ses **métadonnées** (XML collé,
    256 Kio au plus, `identity_providers.saml_metadata`, migration 0021) : son
    identifiant d'entité devient l'émetteur ; elles doivent donner une adresse de
    connexion HTTP-Redirect (contrôlée comme toute adresse d'un fournisseur, ADR-063) et
    au moins un certificat de signature. Pas de secret client ; le client est
    l'identifiant d'entité **propre à l'entreprise** (`{origine}/api/auth/saml/{id de
l'entreprise}/metadata`, où ses métadonnées sont publiées), connu avant la
    déclaration et affiché dans « Connexion » avec l'adresse de réception, commune
    (`{origine}/api/auth/saml/acs`). La confiance qu'un fournisseur accorde est ainsi
    liée à une entreprise : une autre qui déclarerait les mêmes métadonnées n'obtient de
    lui aucune assertion qui la nomme comme audience, comme le client et son secret en
    OpenID Connect. Les certificats se renouvellent en redonnant les métadonnées, du même
    identifiant d'entité seulement ;
  - le départ envoie le navigateur au fournisseur avec une demande (`AuthnRequest`,
    HTTP-Redirect, non signée) ; la tentative scellée (ADR-063) garde la demande
    attendue, dans un cookie propre (`mimorya_saml`, chemin `/api/auth/saml`, 10 minutes,
    HttpOnly, `SameSite=None; Secure`) : le fournisseur poste sa réponse depuis son propre
    site. Cette réception est la seule requête modifiante exemptée de l'en-tête
    anti-CSRF : la tentative scellée, la demande attendue et la signature la lient au
    navigateur qui a commencé ;
  - la réponse n'est acceptée que si : l'assertion est **signée** (RSA avec SHA-256/384/512,
    canonicalisation exclusive) par un certificat des métadonnées ; elle nomme
    l'application comme audience et l'adresse de réception comme destinataire ; elle
    répond à la demande attendue ; elle est dans sa fenêtre de validité ; et aucune des
    assertions de la réponse n'a déjà servi (`sso_used_assertions`, par entreprise,
    toutes enregistrées ensemble jusqu'à leur expiration, purgées ensuite) : un rejeu est
    refusé quel que soit le serveur. Ni
    connexion à l'initiative du fournisseur, ni assertion chiffrée, ni déconnexion
    unique ;
  - ce que dit l'assertion suit les règles d'OpenID Connect : le sujet est l'identifiant
    d'objet d'Entra ID s'il est donné, sinon le NameID (un NameID transitoire, qui change
    à chaque connexion, n'est accepté qu'avec cet identifiant) ; l'adresse est le NameID
    s'il en est une, sinon l'attribut e-mail ; le nom vient des attributs usuels
    (Microsoft, LDAP). Domaines, liaison, invitation (ADR-064), fournisseur obligatoire
    (ADR-066) et création par le lien (ADR-067) s'appliquent tels quels.
- **Conséquences** :
  - le cookie `SameSite=None` exige HTTPS (ou `localhost`) : SAML ne fonctionne pas sur
    une installation servie en HTTP sur le réseau ;
  - les métadonnées sont données par l'administrateur, pas relues chez le fournisseur :
    un certificat renouvelé doit être redonné ;
  - lots : (1) API, vérification et tests contre le fournisseur SAML de `saml-rs` avec des
    clés de test (`crates/api/tests/sso_saml.rs`) ; (2) onglet « Connexion » (type SAML,
    métadonnées collées puis redonnées, identifiant de l'entreprise et adresse de
    réception à enregistrer chez le fournisseur) et test de bout en bout, contre un
    fournisseur SAML servi par `e2e_server` (127.0.0.1:4101), dont les pages, sans
    script, renvoient la réponse par un bouton ;
  - tests : `crates/api/tests/sso_saml.rs` et `sso_saml_declaration.rs` (déclaration par les métadonnées, adresses de
    l'application, connexion et liaison par l'identifiant d'objet, rejeu refusé ;
    réponse sans tentative, vide, pour une autre demande, signée par une autre clé,
    altérée, enveloppée (assertion forgée avant celle signée), à l'assertion non
    signée, hors du profil strict (canonicalisation inclusive), pour une autre
    application, hors des domaines, fournisseur désactivé entre-temps, tentative SAML
    portée au retour OpenID Connect : refusées et tracées ; métadonnées illisibles, sans
    adresse de connexion, avec un secret, sur un fournisseur OpenID Connect, d'un autre
    fournisseur, adresse privée : refusées ; certificat renouvelé ; compte créé par le
    lien seulement ; fournisseur d'une entreprise déclaré par une autre : refusé) ;
    `services/sso/saml/statement.rs`, `saml/flow.rs` (toutes les assertions notées),
    `core/src/identity_providers/tests.rs` ; `apps/web/e2e/saml-sign-on.spec.ts`
    (identifiant de l'entreprise et adresse de réception affichés, métadonnées de
    l'entreprise lues à cet identifiant ; fournisseur déclaré par ses métadonnées, sans
    client ni secret ; connexion par sa page ; métadonnées d'un autre fournisseur
    refusées au renouvellement, les siennes gardées, connexion encore possible) ;
    `crates/api/tests/auth.rs` (`only_the_saml_reception_goes_without_the_header` : sans
    l'en-tête et d'un autre site, la réception seule est atteinte ; les chemins qui lui
    ressemblent, les métadonnées, le retour OpenID Connect et la connexion par mot de
    passe sont refusés).

## ADR-069 — Prouver la propriété d'un domaine par un enregistrement DNS

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : une entreprise déclare les domaines de ses fournisseurs d'identité
  (ADR-063) sans prouver qu'elle les possède. Les adresses sont uniques sur toute la
  plateforme (ADR-011) : un compte, où qu'il soit, réserve son adresse. Le lien de
  connexion d'une entreprise peut créer des comptes (ADR-067), et son administration en
  invite. Microsoft et Google ne garantissent que des adresses de domaines qu'ils ont
  vérifiés chez leur client ; un fournisseur OpenID Connect générique ou SAML dit ce que
  veut son propriétaire. Une entreprise peut ainsi créer chez elle des comptes aux
  adresses du domaine d'une autre, par son lien ou par invitation : l'entreprise qui
  possède le domaine ne peut plus inviter ces personnes, l'adresse étant prise.
- **Décision** :
  - une entreprise **prouve un domaine** en publiant dans son DNS un enregistrement TXT
    `_mimorya.<domaine>` de valeur `mimorya-verification=<empreinte>`. L'empreinte (SHA-256
    de l'entreprise et du domaine, 32 caractères hexadécimaux) est propre à l'entreprise :
    l'enregistrement d'une autre ne prouve rien pour elle. Elle n'est pas secrète, car
    seul qui gère le DNS du domaine peut la publier. Le calcul est du domaine pur
    (`core/src/identity_providers/domain_proof.rs`) ;
  - « Connexion » donne, pour chaque domaine couvert par un fournisseur de l'entreprise,
    l'enregistrement à publier et la date de sa dernière preuve
    (`GET /admin/identity-providers`, champ `domains`). L'administrateur de la connexion
    (`settings:manage` sur toute l'entreprise) demande la lecture
    (`POST /admin/identity-providers/domain-proofs`). Le serveur lit l'enregistrement par
    le résolveur du système (`hickory-resolver`, Rust pur, `resolv.conf`, nom absolu, 5
    secondes au plus), hors du verrou d'administration. S'il trouve la valeur attendue,
    il enregistre la preuve sous le verrou et la trace (`identity_provider.domain_verified`).
    Sinon, il refuse en disant ce qui manque : un domaine qu'aucun fournisseur de
    l'entreprise ne couvre, un enregistrement absent ou d'une autre valeur, ou un DNS
    illisible ;
  - **un domaine prouvé réserve ses nouvelles adresses** à l'entreprise qui l'a prouvé :
    aucune autre n'y crée de compte, ni par invitation (refus « Cette adresse relève d'un
    domaine prouvé par une autre entreprise ») ni à la connexion par son lien, quel que
    soit le fournisseur (refus tracé « Adresse d'un domaine prouvé par une autre
    entreprise »), sauf si elle l'a prouvé aussi. La lecture traverse les entreprises par
    la seule fonction `auth_domain_held_elsewhere` (SECURITY DEFINER), qui répond oui ou
    non sans nommer l'autre entreprise ;
  - un fournisseur **OpenID Connect générique ou SAML ne crée un compte** (ADR-067) **que
    dans un domaine prouvé** par son entreprise : refus tracé « Domaine non prouvé par
    l'entreprise : aucun compte n'y est créé ». Microsoft et Google n'ont pas besoin de
    preuve. La liaison d'un compte que l'entreprise possède déjà, l'acceptation d'une
    invitation et le fournisseur obligatoire ne créent pas de compte : ils ne demandent
    pas de preuve ;
  - la preuve est gardée par entreprise (`verified_domains`, migration 0022, RLS) et
    survit au fournisseur qui couvrait le domaine. Une nouvelle lecture en met la date à
    jour. Deux entreprises peuvent prouver le même domaine, si son propriétaire publie
    leurs deux enregistrements.
- **Conséquences** :
  - une entreprise dont le fournisseur générique ou SAML créait déjà des comptes doit
    prouver ses domaines pour continuer ;
  - la réservation commence à la preuve : une adresse déjà prise, avant elle ou par un
    compte existant d'une autre entreprise, le reste (ADR-011). L'exploitant de la
    plateforme, qui crée le premier administrateur d'une entreprise (ADR-033), n'y est pas
    soumis ;
  - la preuve n'est pas relue ensuite, et elle survit à la fin de la déclaration du
    domaine : un domaine de nouveau déclaré retrouve sa preuve ancienne. Un domaine perdu
    puis racheté par un tiers reste donc prouvé pour l'entreprise ; comme toute preuve
    par DNS sans DNSSEC, elle vaut ce que vaut la résolution du serveur ;
  - un refus d'invitation dit à l'administrateur d'une autre entreprise qu'un client de la
    plateforme a prouvé ce domaine, sans le nommer ; l'enregistrement `_mimorya.<domaine>`
    est de toute façon public dans le DNS ;
  - lots : (1) API, règle et tests, avec le DNS simulé des tests
    (`crates/api/tests/support/dns.rs`, servi aussi par `e2e_server` sur 127.0.0.1:4102) ;
    (2) « Domaines de l'entreprise » dans « Connexion » : chaque domaine, son
    enregistrement, son état (« Non prouvé », « Prouvé le … ») et le bouton « Vérifier »,
    avec leur test de bout en bout ;
  - tests : `crates/api/tests/sso_domain_proofs.rs` :
    - l'enregistrement donné ; refus pour un enregistrement absent, celui d'une autre
      entreprise, un DNS illisible ou un domaine non couvert ; refus tracé sans le droit ;
      preuve datée et tracée ;
    - pas de création par un fournisseur générique avant la preuve, alors qu'un compte
      existant se connecte ; création après ;
    - une autre entreprise qui déclare le domaine de Néo ne peut ni le prouver ni y créer
      de compte par son lien, et, Néo l'ayant prouvé, ni par invitation ;
    - Microsoft crée sans preuve, mais plus dans un domaine qu'une autre entreprise a
      prouvé, ni par lien ni par invitation, jusqu'à ce que Néo le prouve aussi ;

    et `core/src/identity_providers/domain_proof.rs` et `tests.rs` (valeur propre à
    l'entreprise et au domaine, seuls Microsoft et Google prouvent leurs domaines),
    `crates/db/tests/definer_functions.rs` (`auth_domain_held_elsewhere`) ;
    `apps/web/e2e/domain-proofs.spec.ts` (domaine listé avec son enregistrement, non prouvé ;
    vérifié avant publication : refus disant ce qui manque ; publié puis vérifié : prouvé et
    daté, relu après rechargement).

## ADR-070 — Rattraper au démarrage l'avis qu'un arrêt du service a perdu (complète ADR-060)

- **Date** : 2026-10-22 · **Statut** : acceptée · rattrapage devenu périodique, dix
  minutes après la demande, par l'ADR-086
- **Contexte** : l'e-mail qui prévient le responsable part en arrière-plan, une fois la
  pause du run enregistrée (ADR-060). Un arrêt du service entre ces deux moments le
  perd : la demande attend le responsable, qui n'en sait rien, jusqu'à la relance de
  la veille (ADR-062), six jours plus tard, ou jusqu'à son expiration. Chaque avis tenté,
  envoyé ou refusé par le relais, est tracé (`approval.notified`) : une demande sans
  cette trace n'a jamais été signalée.
- **Décision** :
  - au démarrage, une fois le verrou de l'exécuteur pris (ADR-053), **avant** de
    reprendre les runs interrompus et de servir la moindre requête, l'API cherche les
    demandes en attente, confiées à un responsable, jamais signalées, dans un run en
    pause d'une entreprise active. Elle passe par une fonction `SECURITY DEFINER`
    minimale (`system_unnotified_approval_runs`), qui ne rend que l'entreprise et le
    run ; chaque run est ensuite lu dans sa propre entreprise (RLS). Ces demandes sont
    listées à cet instant, puis signalées en arrière-plan comme le fait une pause : un
    message par responsable et par run, seulement s'il est actif, tracé de même. Une
    demande qu'un run repris ou une décision crée ensuite n'est pas dans la liste :
    elle n'est jamais signalée deux fois ;
  - une demande dont l'échéance tombe dans les 24 heures n'est pas rattrapée : la
    relance (ADR-062) la signale, avec son échéance. Pour que les deux ne se croisent
    pas au démarrage, la maintenance périodique n'expire les demandes et ne relance les
    responsables qu'à partir de son deuxième passage, une minute après le démarrage (la
    purge des liens de réinitialisation expirés reste immédiate) ;
  - un échec du rattrapage est journalisé et n'empêche pas de reprendre les runs ; il
    n'est pas réessayé avant le démarrage suivant.
- **Conséquences** :
  - un avis perdu part au démarrage suivant. Sont rattrapés de la même façon un avis
    dont la lecture a échoué avant l'envoi, ou dont la trace n'a pas pu être écrite
    après (le responsable peut alors le recevoir deux fois : « au moins une fois »,
    comme la relance), et les demandes faites quand aucun relais SMTP n'était configuré,
    si l'un l'est désormais : leur responsable les apprend au premier démarrage avec
    relais ;
  - le nombre de messages par pause et par responsable reste borné à deux (l'avis, puis
    la relance), hors un avis renvoyé faute de trace ;
  - l'expiration des demandes échues et la relance commencent une minute après le
    démarrage ;
  - tests :
    - `crates/api/tests/approval_notice_catch_up.rs` : une demande dont l'avis a été
      perdu est signalée au démarrage suivant, sans l'action, tracée, une seule fois ;
      d'une pause dont une demande expire dans les 24 heures, seule l'autre est
      signalée, la première étant relancée ; chaque entreprise est rattrapée chez elle
      et rien de l'autre n'est visible ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue.

## ADR-071 — Délégation des validations pendant une absence (premier lot : l'API)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : une demande confiée à un responsable (ADR-059) n'est décidée que par
  lui. S'il est absent, elle attend sa relance (ADR-062), puis expire (ADR-061) :
  l'agent de la personne poursuit sans l'action, que personne n'a regardée. Il faut
  qu'un collègue puisse décider à sa place pendant l'absence, sans que cette
  désignation n'accorde le moindre droit.
- **Décision** :
  - chacun peut déclarer **son** absence : un collègue qui le remplace, et une période
    (début, maintenant par défaut ; fin à venir, au plus 90 jours après le début)
    (`GET`, `PUT`, `DELETE /approval-delegation`). Une seule par personne : déclarée de
    nouveau, elle remplace la précédente ; elle se retire à tout moment. Le remplaçant est
    un compte actif de l'entreprise, jamais soi-même ; un identifiant inconnu, d'une autre
    entreprise ou d'un compte inactif reçoit la même réponse. Chaque déclaration et chaque
    retrait sont tracés (`approval.delegation_set` avec le remplaçant et la période,
    `approval.delegation_ended`) ;
  - **rien n'est réécrit** : une demande garde le responsable désigné à sa création.
    Tant que la période court et que le compte de l'absent est actif, son remplaçant peut
    décider les demandes en attente confiées à l'absent, **à condition de détenir
    lui-même `approval:decide` sur la personne** pour qui chacune est faite, vérifié à la
    liste comme à la décision, comme pour le responsable. Sans ce droit, il ne voit pas la
    demande et reçoit 404 s'il tente de la décider. La délégation n'accorde aucun droit :
    l'administrateur donne, s'il le faut, un rôle sur le périmètre voulu, avec une
    échéance ;
  - l'absent garde la main : il peut toujours décider, la première décision l'emporte
    (verrou du run). La personne pour qui l'agent agit, même désignée remplaçante, ne
    peut que retirer sa demande ;
  - un seul relais : le remplaçant d'un remplaçant ne décide pas pour le premier absent ;
  - la décision est enregistrée au nom du remplaçant (`decidedBy`) ; l'audit
    (`approval.approved` ou `approval.rejected`) nomme en plus l'absent
    (`standingInFor`). Le remplaçant voit les demandes en attente de l'absent qu'il peut
    décider, et celles qu'il a décidées, jamais ce que l'absent a décidé ;
  - une absence terminée ne remplace plus rien ; la maintenance périodique la supprime
    (`system_purge_ended_approval_delegations`, au périmètre minimal). Il n'en reste que
    les entrées d'audit.
- **Conséquences** :
  - donnée personnelle nouvelle : la période d'absence et le remplaçant, conservés
    jusqu'à la fin de l'absence ; supprimés avec le compte de l'un ou de l'autre ;
  - l'avis et la relance vont au remplaçant plutôt qu'à l'absent (ADR-072) ; l'absence se
    déclare dans « Validations » (ADR-073) ; l'administrateur des comptes la déclare
    pour un collaborateur empêché (ADR-078) ;
  - tests :
    - `crates/api/tests/approval_delegation.rs` : déclaration, lecture par l'absent et
      par le remplaçant, remplacement, retrait, audit ; refus (soi-même, inconnu, autre
      entreprise, compte désactivé, fin passée, fin avant le début, plus de 90 jours),
      sans trace ; absence d'un compte désactivé ou terminée sans effet, puis purgée ;
    - `crates/api/tests/approval_delegation_decisions.rs` : le remplaçant ne voit ni ne
      décide sans `approval:decide` sur la personne, puis décide une fois le droit
      donné, l'audit nommant l'absent ; rien avant le début, après le retrait, pour un
      absent désactivé ni par un second relais ; l'absent décide toujours ; la personne
      désignée ne peut qu'abandonner sa demande ; ce que l'absent a décidé reste caché ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue.

## ADR-072 — L'avis et la relance vont au remplaçant pendant une absence (complète ADR-071)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : depuis ADR-071, le remplaçant désigné par un responsable absent peut
  décider à sa place. Mais l'e-mail d'une pause (ADR-060), celui rattrapé au démarrage
  (ADR-070) et la relance de la veille (ADR-062) partaient toujours vers l'absent, qui ne
  les lit pas : le remplaçant ne l'apprenait qu'en ouvrant la page des validations.
- **Décision** :
  - au moment d'envoyer un avis ou une relance, si le responsable que les demandes
    attendent a une absence en cours (commencée, pas terminée), que son compte et celui
    de son remplaçant sont actifs, et que le remplaçant peut décider ces demandes
    (`approval:decide` sur la personne pour qui elles sont faites, vérifié comme à la
    décision), le message va **au remplaçant, et à lui seul**. Il dit en quelle absence :
    sujet suivi de « (en l’absence de …) », texte ouvert par « En l’absence de …, ». Sinon,
    il va au responsable, comme avant : un remplaçant qui ne pourrait pas décider ces
    demandes n'en apprend jamais l'existence ;
  - le message garde ce qu'il disait : ni l'action ni ses paramètres ;
  - l'audit (`approval.notified`, `approval.reminded`) garde le responsable désigné
    (`approverId`) et nomme le remplaçant prévenu (`delegateId`).
- **Conséquences** :
  - une demande reste signalée une fois et relancée une fois : une demande signalée au
    responsable avant son absence ne l'est pas de nouveau au remplaçant, mais sa relance,
    si elle tombe pendant l'absence, va au remplaçant ;
  - pendant son absence, le responsable ne reçoit rien pour les demandes que son
    remplaçant peut décider ; il les voit toujours dans « Validations » ;
  - tests :
    - `crates/api/src/services/runs/approval_notice/message.rs` : texte au remplaçant ;
    - `crates/api/tests/delegate_notices.rs` : avis et relance au remplaçant, en
      l'absence nommée et tracés ; le responsable prévenu quand le remplaçant ne pourrait
      pas décider, ou que l'absence n'a pas commencé.

## ADR-073 — Déclarer son absence dans « Validations » (complète ADR-071)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : l'absence ne se déclarait que par l'API (ADR-071). Pour désigner un
  remplaçant, il faut aussi savoir qui peut l'être ; l'annuaire de l'entreprise n'est
  lisible que par l'administration.
- **Décision** :
  - la page « Validations » ouvre sur une section « Absence », montrée à qui détient
    `approval:decide` quelque part, ou remplace quelqu'un. On y choisit un remplaçant et
    deux jours, du premier (aujourd'hui : dès maintenant) au dernier inclus, dans le fuseau
    du navigateur, au plus 90 jours (l'API tolère l'heure qu'ajoute un changement d'heure) ; l'absence déclarée se relit (« … vous remplace du … au … ») et prend
    fin d'un bouton. Le remplaçant y lit les absences qu'il couvre (« Vous remplacez … du
    … au … ») ; une demande qu'il décide pour un absent porte « En l’absence de … » ;
  - `GET /approval-delegation` donne aussi les remplaçants possibles (`candidates`) : les
    comptes actifs de l'entreprise, autres que le lecteur, qu'un rôle en cours charge
    d'`approval:decide` sur une unité ou toute l'entreprise, nom seulement. Une politique
    de refus n'y est pas lue : c'est une indication, et le droit est vérifié à chaque
    décision (ADR-071). La liste n'est donnée qu'à qui détient lui-même `approval:decide` :
    les autres n'apprennent pas qui valide dans l'entreprise.
- **Conséquences** :
  - aucune donnée nouvelle n'est conservée ; un responsable voit le nom des autres
    collaborateurs habilités à valider ;
  - l'absence d'un collaborateur empêché, déclarée par l'administrateur : ADR-078 ;
  - tests :
    - `crates/api/tests/approval_delegation.rs` : remplaçants possibles, sans le lecteur, ni
      un compte désactivé, ni une habilitation échue ou sur soi seul, donnés seulement à qui
      valide ; 90 jours et une heure acceptés, pas davantage ;
    - `apps/web/e2e/manager-absence.spec.ts` : Marc déclare son absence en désignant
      Julien, relue après rechargement et par l'API (remplaçant, fin du dernier jour) ; Julien, prévenu par e-mail en
      l'absence de Marc, voit qu'il le remplace et refuse la demande de Nadia, relue
      décidée par lui ; Marc met fin à son absence. Le serveur de bout en bout lui
      réserve un scénario du modèle (`SCENARIO_HOURS`).

## ADR-074 — Mémoire sémantique : modèle d'embeddings par entreprise, recherche hybride (premier lot : l'API)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : la mémoire n'était cherchée que par ses mots (plein texte français,
  ADR-004). Une demande qui dit « véhicule » ne retrouvait pas la mémoire qui parle des
  « camionnettes ». La colonne `embedding vector(1024)` attendait un modèle. Calculer un
  vecteur, c'est envoyer le texte de la mémoire à un fournisseur : l'entreprise doit le
  choisir en connaissance de cause.
- **Décision** :
  - **désactivé par défaut** : sans choix de l'entreprise, rien de sa mémoire ne part
    vers un modèle d'embeddings et la recherche reste plein texte, comme avant ;
  - un administrateur des réglages de toute l'entreprise (`settings:manage`) choisit son
    modèle (`GET`, `PUT`, `DELETE /admin/embedding`, `POST /admin/embedding/test`), sur
    les règles du modèle de langage (ADR-024) : Mistral (`mistral-embed`) ou OpenAI
    (`text-embedding-3-small`, demandé en 1024 dimensions) à leur adresse officielle avec
    une clé, ou un serveur compatible OpenAI (Ollama, vLLM, TEI…) à une adresse que liste
    l'exploitant (`MIMORYA_TENANT_LLM_ENDPOINTS`). La clé est scellée (`embedding.api_key`),
    jamais rendue ni tracée, jamais emportée vers une autre adresse ; la configuration est
    revérifiée à chaque chargement. Les vecteurs doivent avoir 1024 dimensions : un modèle
    qui en rend d'autres est refusé, jamais tronqué ; un vecteur nul (sans direction) est
    refusé comme son texte ;
  - une **similarité minimale** (cosinus, strictement entre 0 et 1) dit quand une mémoire
    est proche : les modèles répartissent leurs similarités différemment, la valeur de
    départ dépend du fournisseur (Mistral 0,75, OpenAI 0,35, autres 0,5) et se règle ;
  - **indexation en arrière-plan** : les mémoires actives sans vecteur du modèle choisi
    (écrites ou approuvées depuis, ou indexées par un autre modèle) reçoivent le leur par
    lots de 32, huit lots par entreprise et par passage, un passage à la fois : au
    démarrage, à chaque écriture ou approbation d'une mémoire et à chaque changement de
    modèle, puis chaque minute. Une proposition n'est envoyée qu'une fois approuvée. Le
    choix de l'entreprise est relu avant chaque lot et verrouillé pendant l'écriture de
    ses vecteurs : dès qu'il change ou que l'entreprise revient aux mots, plus rien ne part
    vers l'ancien modèle et ce que rapporte le lot en vol est jeté (seul ce lot, déjà
    parti, a pu sortir). Un texte que le modèle refuse comme trop long (dépassement de
    contexte, 413, 422), ou comme invalide alors que d'autres de son lot reçoivent leur
    vecteur, n'est plus demandé (`embedding_model` posé, sans vecteur) ; un lot dont tous
    les textes sont refusés comme invalides est une panne du modèle ou de son réglage.
    Toute panne est reprise au passage suivant et montrée aux administrateurs jusque-là.
    Seules les entreprises actives qui ont choisi un modèle sont parcourues
    (`system_tenants_awaiting_embeddings`, qui ne rend que l'entreprise) ; chaque lot est
    lu et écrit dans la transaction de l'entreprise (RLS) ;
  - **recherche hybride** : au début de chaque tour et par l'outil de recherche de
    l'agent, la demande (ses 2 000 premiers caractères) reçoit son vecteur, attendu
    5 secondes au plus. Les deux classements — par les mots et par le sens — ne portent
    que sur les mémoires que le sujet peut lire (pré-filtrage SQL, ADR-004), puis sont
    fusionnés par rang réciproque (RRF, constante 60). Les vecteurs sont comparés
    exactement sur ces seules mémoires : un index approché filtrerait après coup et en
    manquerait ; l'index HNSW reste pour des volumes qui le justifieront. Seuls comptent
    les vecteurs du modèle choisi (`embedding_model`) et assez proches. Si le modèle
    échoue ou tarde, les mots trouvent seuls : le tour ne s'arrête pas ;
  - chaque appel est compté (`usage_records`, `kind = 'embedding'`), au nom de
    l'utilisateur et du run pour une demande, sans pour l'indexation ; son coût n'est
    estimé que si le modèle est tarifé (configuration), jamais deviné ;
  - revenir à la recherche par les mots supprime le choix, la clé et **tous les
    vecteurs** de l'entreprise. Audit : `embedding.configured` (fournisseur, modèle,
    adresse, similarité, clé remplacée, état précédent), `embedding.reset` (vecteurs
    effacés), `embedding.tested`, sans clé ni texte.
- **Alternatives écartées** : un modèle d'embeddings offert par la plateforme à toutes
  les entreprises (leur mémoire partirait sans qu'elles l'aient choisi) ; calculer le
  vecteur à l'écriture, dans la requête (une panne du fournisseur bloquerait
  l'enregistrement d'une mémoire) ; un plafond de sensibilité des mémoires envoyées
  (les textes rappelés partent déjà vers le modèle de langage ; à reconsidérer avec les
  contraintes « local uniquement » par sensibilité).
- **Conséquences** :
  - le texte des mémoires actives et celui des demandes partent vers le fournisseur
    choisi ; le vecteur, dérivé de la mémoire, est supprimé avec elle et effacé au retour
    à la recherche par les mots ;
  - une mémoire dont le texte changerait devrait perdre son vecteur (aucune
    modification n'existe aujourd'hui : une mémoire se crée, s'approuve ou se supprime) ;
  - restent l'écran d'administration (lot 2) et la capitalisation automatique des leçons ;
  - tests :
    - `crates/agent/src/llm/embedding/` : réponse remise dans l'ordre, vecteur manquant, nul
      ou d'autres dimensions refusé, texte trop long distingué d'une panne (413, 422,
      dépassement de contexte), dimensions demandées à OpenAI seul, lot trop grand
      refusé ; choix vérifié (fournisseur, modèle, adresse listée, similarité), clé
      requise, configuration revérifiée, clé absente des traces ;
    - `crates/db/tests/memory_search.rs` : trouvé par le sens hors de tout mot commun,
      jamais hors de portée du sujet, ni d'un autre modèle, ni trop loin ; ce que trouvent
      mots et sens en tête ; file des mémoires à indexer (refus, autre modèle) ; seule
      une entreprise active qui a choisi un modèle est parcourue, l'autre entreprise
      comprise, jamais suspendue ;
    - `crates/api/tests/admin_embedding.rs` : clé scellée, jamais montrée ni tracée,
      supprimée avec le choix et les vecteurs ; règles du choix, clé qui ne suit pas ;
      administrateurs de toute l'entreprise seuls ; test compté et tracé ;
    - `crates/api/tests/semantic_memory.rs` : rien n'est envoyé sans modèle ; un tour
      rappelle par le sens ce que son utilisateur peut lire, jamais la mémoire personnelle
      d'un autre ; les mots trouvent pendant une panne, reprise ensuite ; proposition
      envoyée une fois approuvée ;
    - `crates/api/tests/memory_indexing.rs` : texte invalide ou trop long non redemandé,
      même un lot entier de textes trop longs ; lot refusé en entier comme invalide montré
      comme panne, rien de marqué ; autre modèle, nouvelle
      indexation ; retour aux mots, vecteurs effacés, y compris pendant un passage (lot en
      vol jeté, aucun autre envoyé) ; un passage n'envoie rien quand le choix a changé ;
      indexation en arrière-plan ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue.

## ADR-075 — Choisir le modèle d'embeddings dans « Modèle d'IA » (complète ADR-074)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : le modèle d'embeddings d'une entreprise ne se choisissait que par l'API
  (ADR-074).
- **Décision** :
  - l'onglet « Modèle d'IA » de l'administration ajoute, sous le modèle des conversations,
    une section « Recherche dans la mémoire » : « par les mots seulement », ou « par les
    mots et par le sens » avec le fournisseur, le modèle, l'adresse, la similarité
    minimale, la clé (enregistrée ou non), l'avancement de l'indexation (mémoires indexées
    sur mémoires actives), le problème de la configuration ou la dernière panne
    d'indexation s'il y en a ;
  - on y choisit ou modifie le modèle (fournisseur, modèle, serveur de la liste de
    l'exploitant pour un serveur local, similarité minimale — vide, la valeur de départ
    du fournisseur, remise à vide quand le fournisseur change —, clé en écriture seule,
    conservée pour le même fournisseur à la même adresse), on le teste, on actualise
    l'avancement, et on revient aux mots après confirmation (vecteurs et clé effacés) ;
  - la page dit ce qui part chez le fournisseur : le texte des mémoires actives et celui
    des demandes ;
  - le serveur de bout en bout donne à toute entreprise un modèle d'embeddings de test :
    aucun fournisseur n'est appelé.
- **Conséquences** :
  - aucune donnée nouvelle ; reste la capitalisation automatique des leçons ;
  - tests : `apps/web/e2e/semantic-memory.spec.ts` : Paul choisit Mistral avec une clé et
    une similarité, la mémoire est indexée en arrière-plan (relu par l'API, puis affiché
    après rechargement), la clé n'apparaît jamais, le test répond, puis le retour aux mots
    efface le choix.

## ADR-076 — Leçons tirées des conversations, proposées dans la mémoire personnelle (premier lot : l'API)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : la mémoire ne grandissait que de ce qu'on y écrit (à la main, ou par l'outil
  `memory_remember` quand l'utilisateur le demande). Ce qu'une conversation a appris — une
  façon de faire qui a marché, un piège évité, une préférence — se perdait.
- **Décision** :
  - **désactivé par défaut**, choisi par entreprise (`GET`, `PUT /admin/memory-settings`,
    `settings:manage` sur toute l'entreprise, sous le verrou d'administration, audit
    `memory.lessons_configured`) : relire une conversation l'envoie une fois de plus au
    modèle ;
  - une fois la réponse donnée, une conversation **qui a appelé au moins un outil** est relue
    en arrière-plan par le modèle des conversations de l'entreprise (le sien, sinon celui de
    la plateforme ; un modèle inutilisable ne relit rien). Il reçoit le run depuis la demande
    qui l'a lancé (la demande, les appels d'outils et leurs résultats, abrégés, la réponse),
    **comme donnée** (enveloppe non fiable dont on ne peut pas sortir), et, comme donnée
    aussi, les titres des leçons que la personne a gardées, reçues ou refusées ; il propose **au plus une leçon**, générale, sans donnée
    d'un tiers, secret, identifiant, montant ni détail d'un client, ou aucune ;
  - la leçon est **proposée dans la mémoire personnelle de la personne servie**
    (`scope = 'user'`, `kind = 'lesson'`, `status = 'proposed'`, source : le run) : elle seule
    la voit et la valide ou la rejette (« Ma mémoire »). Aucun collègue, responsable compris,
    ne lit ce qu'une conversation d'un autre a appris ; la personne pourra la partager si elle
    en a le droit. Titre de 3 à 120 caractères, contenu de 3 à 2 000, sur une ligne, sans
    caractère caché ni balise d'enveloppe ; une leçon dont le titre existe déjà (casse
    ignorée), gardée, proposée ou refusée, n'est pas reproposée ;
  - ce qu'a coûté la relecture est compté avec le run (`usage_records`, `llm`, au nom de la
    personne), dès la réponse et quoi qu'il advienne de la leçon ; l'audit `memory.lesson_proposed` nomme la personne, le run et la mémoire, jamais
    son texte. Une panne perd la leçon, jamais le run ; un arrêt du service pendant la
    relecture la perd aussi (au mieux, une leçon par run).
- **Alternatives écartées** : proposer la leçon à l'équipe ou à toute l'entreprise (mémoire
  d'expérience) — un valideur lirait un détail d'une conversation qu'il n'aurait pas pu voir ;
  relire chaque conversation (le coût sans le bénéfice) ; une relecture au fil du run (elle
  retarderait la réponse).
- **Conséquences** :
  - donnée personnelle nouvelle : la leçon, dans la mémoire personnelle, avec sa rétention, son
    export et sa suppression ;
  - une tâche d'arrière-plan qui panique est désormais décomptée : ni l'arrêt du service ni les
    tests ne l'attendent indéfiniment (`services/background.rs`, testé) ;
  - une leçon est de sensibilité « interne » : sans effet tant qu'elle reste personnelle, mais
    son partage, s'il vient, devra hériter de la sensibilité la plus haute lue par le run ;
  - l'écran de ce réglage : ADR-077 ;
  - tests :
    - `crates/agent/tests/lesson.rs` : la relecture commence à la demande du run et exige un
      appel d'outil, abrège les résultats, remet le run comme donnée (enveloppe non fermable)
      avec les leçons connues ; aucune leçon si le modèle n'en voit pas, hors bornes ou sans
      réponse lisible ; texte nettoyé ;
    - `crates/api/tests/lesson_capture.rs` : rien par défaut ; une fois choisi, une leçon
      proposée à Marc seul (pas à Lucas), tracée sans texte, comptée dans le run, validée ;
      à Nadia seule quand Marc a validé l'action de son run ; rien d'une conversation sans
      outil, quand le modèle n'en voit pas, ni deux fois, une leçon refusée comprise ; une
      relecture en panne laisse le run terminé, sans leçon ;
    - `crates/api/tests/admin_memory_settings.rs` : réglage réservé aux administrateurs des
      réglages de toute l'entreprise, désactivé par défaut, tracé.

## ADR-077 — Choisir les leçons dans « Modèle d'IA » ; des propositions qui disent où elles vont (complète ADR-076)

- **Date** : 2026-10-22 · **Statut** : acceptée
- **Contexte** : les leçons tirées des conversations ne s'activaient que par l'API
  (ADR-076). La carte « Propositions à valider » de la page « Mémoire » ne parlait que des
  espaces partagés, alors qu'une leçon arrive dans la mémoire de la personne.
- **Décision** :
  - l'onglet « Modèle d'IA » ajoute, sous « Recherche dans la mémoire », une section
    « Leçons tirées des conversations » : une case « Proposer à chacun les leçons de ses
    conversations » et « Enregistrer », actif seulement si le choix change. L'état repart
    du réglage rechargé (ADR-041). La page dit qui relit (le modèle des conversations),
    quand (après chaque réponse qui a utilisé des outils : une relecture, et au plus une
    leçon, par demande), à qui va la leçon (la personne servie, seule à la voir et à la
    valider), qu'une leçon refusée ne revient pas sous le même titre et que chaque relecture
    est comptée. Le réglage est dans cet onglet parce que c'est ce
    modèle qui relit, à ses frais ;
  - chaque formulaire de l'onglet porte un nom (`aria-label`) : ses trois boutons
    « Enregistrer » se distinguent, pour les technologies d'assistance comme pour les
    tests ;
  - chaque proposition de la page « Mémoire » dit sa nature (« Leçon »…) et où elle irait
    (« Ma mémoire », l'unité, « Entreprise », « Expérience de l'agent ») ; ses boutons
    portent son titre (« Valider « … » »), pour distinguer plusieurs propositions ;
  - le serveur de bout en bout répond à part à la relecture, qu'il reconnaît à ses
    consignes, toujours par la même leçon : les scénarios des conversations gardent leur
    ordre (`e2e_server/lessons.rs`).
- **Alternatives écartées** : un onglet « Mémoire » dans l'administration, pour un seul
  réglage ; un enregistrement dès que la case change, contraire aux autres formulaires de
  l'administration, pour un choix qui coûte.
- **Conséquences** :
  - aucune donnée nouvelle ;
  - tests : `apps/web/e2e/lessons.spec.ts` : Paul coche la case (cochée après
    rechargement) ; la demande de Marc se termine, refusée ; la leçon est proposée dans sa
    mémoire (relue par l'API), marquée « Leçon » et « Ma mémoire ». Marc la valide (active
    après rechargement), puis la supprime. Quoi qu'il arrive, le réglage est désactivé, la
    leçon supprimée et une validation restée en attente refusée.

## ADR-078 — L'administrateur des comptes déclare l'absence d'un collaborateur empêché (premier lot : l'API)

- **Date** : 2026-10-23 · **Statut** : acceptée
- **Contexte** : seul l'absent déclarait son absence (ADR-071). Un responsable empêché
  (maladie, accident) ne le peut pas : les demandes qui lui sont confiées attendent sa
  relance, puis expirent sans que personne ne les ait regardées.
- **Décision** :
  - un administrateur des comptes (`user:manage` sur toute l'entreprise) déclare,
    remplace ou lève l'absence d'un collaborateur (`GET`, `PUT`, `DELETE
/admin/users/{id}/absence`). C'est la même absence que celle que le collaborateur
    déclare lui-même, avec les mêmes règles : un remplaçant actif de l'entreprise,
    jamais l'absent ; une fin à venir, au plus 90 jours après le début ; aucun droit
    accordé, le remplaçant ne décidant que là où il détient `approval:decide`. Le
    lecteur obtient aussi les remplaçants possibles (ADR-073), ceux de l'absent ;
  - jamais pour soi-même : son absence se déclare dans « Validations ». Jamais non plus en
    se désignant remplaçant (403) : l'administrateur prendrait les avis destinés à un autre
    et déciderait ses demandes, là où il en a le droit ; il n'est pas proposé parmi les
    remplaçants possibles. Un compte inconnu ou d'une autre entreprise reçoit la même
    réponse (404). Un compte inactif est refusé (409) : l'absence y serait sans effet. Les
    changements sont faits sous le verrou d'administration ;
  - sont tracés, comme pour les autres changements d'administration (ADR-038, ADR-047,
    ADR-054) : un refus de droit (sans `user:manage` sur toute l'entreprise, pour soi-même,
    soi-même comme remplaçant), un compte absent ou étranger, un remplaçant qui ne désigne
    rien de l'entreprise. Une requête invalide (période, l'absent comme remplaçant) et un
    compte inactif ne le sont pas ;
  - l'absence garde le nom de l'administrateur qui l'a déclarée (`declared_by_user_id`) :
    - le collaborateur le lit dans sa propre absence (`declaredBy`), comme le
      remplaçant ; « Validations » l'affiche (ADR-079) ;
    - redéclarée par le collaborateur, elle redevient la sienne ;
    - il garde la main : il y met fin, ou en déclare une autre ;
  - le collaborateur est prévenu par e-mail quand un relais est configuré :
    - à la déclaration, l'e-mail dit qui l'a déclarée, qui le remplace, du quand au
      quand, et où y mettre fin ;
    - à la levée, il dit qui l'a levée ;
    - il ne contient rien de ses demandes. Une déclaration détourne les avis qui lui
      étaient destinés vers le remplaçant (ADR-072) : il doit l'apprendre, et pouvoir
      réagir s'il ne l'attendait pas ;
    - un envoi en échec n'annule rien : la réponse dit si le collaborateur a été prévenu
      (`notified`), et l'échec est journalisé sans le destinataire ;
  - l'audit (`approval.delegation_set`, `approval.delegation_ended`) nomme
    l'administrateur comme auteur et le collaborateur comme ressource.
- **Alternatives écartées** :
  - une permission nouvelle, propre aux absences : le compte d'un collaborateur relève
    déjà de l'administrateur des comptes ;
  - laisser le responsable hiérarchique la déclarer : il n'a aucun droit sur le compte
    de l'absent, et ce serait lui donner de quoi détourner les avis d'un autre ;
  - réaffecter les demandes en attente à un autre responsable : la règle reste que rien
    n'est réécrit (ADR-071).
- **Conséquences** :
  - donnée nouvelle : l'auteur de l'absence, nul pour une absence déclarée par l'absent
    ou une fois le compte de l'administrateur supprimé ; supprimée avec l'absence ;
  - l'administrateur des comptes, qui lit déjà l'annuaire, apprend par les remplaçants
    possibles qui valide dans l'entreprise, même sans `role:manage` ;
  - l'écran de l'administration, avec l'auteur affiché dans « Validations » : ADR-079 ;
  - tests :
    - `crates/api/src/services/admin/absence_mail.rs` : texte de la déclaration, de la
      levée et de l'annulation d'une absence à venir ;
    - `crates/api/tests/admin_absences.rs` :
      - Paul déclare l'absence de Marc. Elle est relue par Paul, par Marc (avec son
        auteur) et par Julien, qui le remplace ; Marc en est prévenu par e-mail ;
      - Marc y met fin, puis en déclare une à lui, qui ne nomme plus Paul ;
      - Paul la lève, puis annule une absence à venir, et Marc en est prévenu chaque fois ;
        le tout est tracé au nom de chacun ;
      - sans relais, ou quand l'envoi échoue, l'absence tient et la réponse le dit ; l'échec
        est journalisé sans l'adresse ni la réponse du relais ;
    - `crates/api/tests/admin_absence_refusals.rs` : refus, sans rien enregistrer ni
      prévenir :
      - un responsable sans `user:manage`, un administrateur de son équipe seulement,
        l'administrateur pour lui-même, un compte inconnu ou d'une autre entreprise, tracés ;
      - l'administrateur comme remplaçant (tracé, et absent des remplaçants proposés), un
        remplaçant inconnu ou étranger (tracé) ;
      - l'absent comme remplaçant, une période impossible, un compte désactivé ;
    - `crates/api/tests/first_login.rs` : les trois routes retiennent un mot de passe
      provisoire.

## ADR-079 — Déclarer l'absence d'un collaborateur depuis « Utilisateurs » ; son auteur affiché dans « Validations » (complète ADR-078)

- **Date** : 2026-10-23 · **Statut** : acceptée
- **Contexte** : l'absence d'un collaborateur empêché ne se déclarait que par l'API
  (ADR-078), et le collaborateur ne lisait le nom de son auteur que par l'API.
- **Décision** :
  - dans l'onglet « Utilisateurs », le panneau d'un compte actif ouvre, pour
    l'administrateur des comptes, une section « Absence » :
    - sans absence : ce qu'elle fait (le remplaçant valide là où il en a le droit, la
      désignation n'en donne aucun, la personne voit qui l'a déclarée et peut y mettre
      fin, prévenue par e-mail, ou à prévenir si le serveur n'en envoie pas) et le
      formulaire de « Validations » (remplaçant parmi ceux proposés, premier et dernier
      jour, au plus 90 jours), « Déclarer son absence » ;
    - avec une absence : qui remplace qui, du quand au quand, l'administrateur qui l'a
      déclarée s'il est nommé (sinon, la personne elle-même, ou un compte supprimé :
      rien n'est affirmé), « Lever l'absence » après confirmation ;
    - après chaque changement, la page dit si un e-mail en a informé la personne, ou
      qu'il faut la prévenir ; un changement refusé (l'absence a pris fin entre-temps)
      recharge l'absence telle qu'elle est ;
  - le formulaire est celui de « Validations », rendu commun : ce qu'il envoie et à qui
    lui sont donnés par la page qui l'utilise ;
  - dans « Validations », une absence déclarée par un administrateur le dit : « Absence
    déclarée pour vous par …, depuis l'administration. ».
- **Alternatives écartées** : un onglet « Absences » dans l'administration, quand
  l'absence se rattache au compte de la personne.
- **Conséquences** :
  - aucune donnée nouvelle ;
  - tests : `apps/web/e2e/admin-absence.spec.ts` :
    - Paul déclare l'absence de Marc depuis « Utilisateurs », Julien le remplaçant. Le
      test la relit par l'API (remplaçant, auteur, fin du dernier jour), puis à l'écran
      après rechargement ;
    - Marc, prévenu par e-mail, lit dans « Validations » qui l'a déclarée et y met fin ;
    - Paul la déclare de nouveau, puis la lève après confirmation : Marc en est
      prévenu ;
    - quoi qu'il arrive, Marc reste sans absence.

## ADR-080 — Tâches planifiées : une demande à l'agent, à heure fixe (premier lot : l'API)

- **Date** : 2026-10-23 · **Statut** : acceptée
- **Contexte** : l'agent ne travaillait que lorsqu'on lui écrivait. Une demande récurrente
  (« chaque lundi à 8 h, fais le point sur les interventions de la semaine ») devait être
  retapée à chaque fois. C'est le premier lot du chantier « worker séparé, missions
  longues, reprise par bail, tâches planifiées » : il s'appuie sur l'exécuteur actuel
  (une seule instance exécute les runs, ADR-016, ADR-053), sans infrastructure nouvelle ;
  le worker séparé et la reprise par bail viendront après.
- **Décision** :
  - chacun écrit ses tâches (`GET`, `POST /scheduled-tasks`, `PATCH`, `DELETE
/scheduled-tasks/{id}`) : un intitulé (une ligne, 120 caractères au plus), la demande
    envoyée comme la sienne (4 000 au plus), des jours de la semaine, une heure `HH:MM` et
    un fuseau IANA. Caractères cachés retirés. Vingt tâches au plus par personne. Une
    tâche se met en pause et reprend ; ses changements sont tracés
    (`scheduled_task.created`, `updated`, `deleted`) avec les jours, l'heure, le fuseau
    et l'état, jamais le texte de la demande ;
  - ce sont ses ressources : celles d'un autre répondent comme absentes (404). Créer ou
    reprendre une tâche demande le droit d'utiliser l'agent (`agent:use`) ; la mettre en
    pause ou la supprimer, non : qui l'a perdu arrête encore les siennes ;
  - l'heure est celle de l'auteur, dans son fuseau (`core/src/schedule.rs`) : une heure que
    le passage à l'heure d'été saute part dès qu'elle existe à nouveau ; une heure que le
    retour à l'heure d'hiver répète part la première fois ;
  - chaque minute, le processus qui exécute les runs (celui qui tient le verrou de
    l'exécuteur ; un second processus, qui va refuser de démarrer, ne lance rien) lance les
    tâches dues (`system_due_scheduled_tasks`, entreprise et identifiant seulement, lot de
    50 après lot de 50 tant qu'il en reste). Il ne commence qu'une fois repris les runs
    qu'un arrêt du service a interrompus (ADR-053) : un run lancé avant serait pris pour
    l'un d'eux. À l'arrêt du service, la passe s'interrompt entre deux tâches, jamais au
    milieu d'un lancement. Une occurrence est d'abord réclamée : la ligne verrouillée
    (`for update skip locked`), revérifiée due, la tâche reprogrammée à sa prochaine
    heure. Elle ne part jamais deux fois, même si deux passes se croisent ;
  - elle part comme une demande de son auteur, dans une conversation nouvelle à lui
    (intitulée comme la tâche), avec ses droits tels qu'ils sont alors : le run les relit à
    chaque étape, ses validations suivent le circuit habituel. Son démarrage est tracé
    comme celui de la plateforme, au nom de l'auteur (`agent.run.started`, acteur système,
    avec la tâche) : il ne l'a pas lancé lui-même à cette heure. Rien n'est ouvert si
    aucun modèle n'est disponible, si son entreprise est suspendue ou s'il n'a plus le
    droit d'utiliser l'agent ; une conversation dont le run ne démarrerait pas tout de même
    est archivée (l'application n'en supprime aucune) (remplacé par ADR-109 : la
    conversation s'ouvre dans la transaction du tour, un run qui ne démarre pas n'en
    laisse aucune) ;
  - une occurrence ne part pas :
    - si le dernier run qu'une occurrence a lancé est encore en cours ou attend une
      validation, quel que soit le nombre d'occurrences retenues depuis ;
    - plus d'une heure après son heure (le service était arrêté : la demande peut avoir
      perdu son objet) ;
    - si le compte de l'auteur est désactivé : la tâche se met alors en pause, plutôt que
      d'être refusée chaque jour, et il la reprend une fois son compte rétabli ;
    - s'il doit remplacer un mot de passe provisoire, ou s'il a perdu le droit d'utiliser
      l'agent ;
    - si son entreprise est suspendue ;
    - sans modèle disponible ;
  - un changement qui laisse ses jours, son heure et son fuseau garde sa prochaine
    occurrence, même due et pas encore lancée ; un autre la recalcule ;
  - ce qu'est devenue la dernière occurrence est gardé pour l'auteur (partie, ou pas
    partie et pourquoi, en français), avec la conversation du dernier run lancé, celle qui
    attend peut-être sa validation. L'occurrence est tracée (`scheduled_task.fired` avec
    le run, ou `scheduled_task.not_run` avec la raison ; acteur système, au nom de
    l'auteur).
- **Alternatives écartées** :
  - une expression cron libre : illisible pour un collaborateur, et capable de lancer
    l'agent chaque minute ; une fois par jour au plus par tâche suffit aux usages visés ;
  - une seule conversation par tâche : l'historique s'y allongerait sans fin (la compression
    du contexte est un chantier à venir) et une occurrence dépendrait des précédentes ;
  - rattraper toutes les occurrences manquées : une série de runs au redémarrage, pour
    des demandes qui ont perdu leur objet.
- **Conséquences** :
  - donnée personnelle nouvelle : la tâche et le texte de sa demande, gardés tant que son
    auteur la garde ; supprimés avec elle ou avec son compte ; lisibles en entier par lui
    (`GET /scheduled-tasks`), à joindre à l'export de ses données ;
  - une occurrence qui demande une validation de l'auteur l'attend dans « Validations »,
    sans l'en prévenir par e-mail : à prévoir avec l'écran ;
  - restent l'écran des tâches planifiées (lot 2), puis le worker séparé et la reprise par
    bail ;
  - tests :
    - `crates/core/src/schedule.rs` : jours et heure lus, prochaine occurrence, passages à
      l'heure d'été et d'hiver ;
    - `crates/api/src/services/scheduled_tasks/definition.rs` : intitulé sur une ligne,
      caractères cachés retirés, bornes ;
    - `crates/api/tests/scheduled_tasks.rs` :
      - créer, lire, mettre en pause, changer, reprendre et supprimer les siennes, tracé
        sans le texte ;
      - celles d'un autre répondent absentes ;
      - refus d'une tâche qui ne tient pas et au-delà de vingt ;
      - sans le droit d'utiliser l'agent, ni création ni reprise, mais pause et suppression ;
    - `crates/api/tests/scheduled_task_firing.rs` :
      - une tâche due part une fois, même pour deux passes simultanées ou une passe en
        retard, dans une conversation nouvelle de son auteur, seul à la voir ; tracée, et
        son run démarré par la plateforme au nom de l'auteur ;
      - le serveur ne lance rien avant d'avoir repris les runs interrompus, puis lance à
        l'instant ; il s'arrête sans attendre la passe suivante ; arrêté, il ne lance plus ;
      - une passe va au-delà d'un lot ;
      - changer le texte garde une occurrence due ; changer l'heure la recalcule ;
    - `crates/api/tests/scheduled_task_holds.rs` : ne part pas tant que le dernier run
      lancé attend une validation (deux jours de suite), ni plus d'une heure en retard, ni
      pour un compte désactivé (mise en pause), à mot de passe provisoire ou privé de
      l'agent, ni pour une entreprise suspendue, ni sans modèle, sans ouvrir de
      conversation ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est connue ;
    - `crates/api/tests/first_login.rs` : les routes retiennent un mot de passe provisoire.

## ADR-081 — Écran des tâches planifiées (deuxième lot)

- **Date** : 2026-10-23 · **Statut** : acceptée
- **Contexte** : les tâches planifiées (ADR-080) ne s'écrivaient que par l'API ; il fallait
  un écran pour les créer, les suivre et les arrêter, et lire ce qu'est devenue chacune.
- **Décision** :
  - une page « Tâches planifiées » dans la navigation, pour chacun. Une carte « Nouvelle
    tâche » : intitulé, demande à l'agent, jours cochés (du lundi au vendredi par
    défaut), heure et fuseau (celui du navigateur par défaut). Sans le droit d'utiliser
    l'agent, ou à vingt tâches, la carte le dit à la place du formulaire ;
  - chaque tâche dit quand elle part, en mots (« Du lundi au vendredi à 8 h 00 », son
    fuseau entre parenthèses), sa prochaine occurrence (lue, comme la dernière, dans ce
    fuseau), sa demande, ce qu'est devenue la
    dernière occurrence (un badge, et la raison donnée par la plateforme) et mène à la
    conversation du dernier lancement, qui attend peut-être une validation. « Modifier »,
    « Mettre en pause » ou « Reprendre » et « Supprimer » sont nommés par son intitulé ;
    le formulaire de modification suit la liste rechargée (ADR-041) ; les refus de l'API
    s'affichent tels quels ;
  - une passe du serveur distingue une tâche réclamée d'une tâche qu'une modification en
    cours verrouille : un lot où aucune n'a pu l'être termine la passe, au lieu de les
    relire en boucle (suggestion de l'audit de l'ADR-080) ;
  - le serveur des tests de bout en bout laisse au démarrage une tâche de Léa, due deux
    heures plus tôt : sa première passe la dit « non rattrapée », sans appeler le modèle,
    et la page le montre.
- **Alternatives écartées** :
  - les tâches parmi les conversations : elles s'y perdraient entre les échanges, quand
    la page dit ce qui part, quand, et ce qu'il en est advenu ;
  - une tâche du test lancée pour de vrai : elle prendrait au modèle scénarisé un tour que
    les tests suivants attendent ; le lancement est éprouvé sur l'API (ADR-080).
- **Conséquences** :
  - restent l'e-mail à l'auteur quand une occurrence attend sa validation, puis le worker
    séparé (qui bornera aussi les runs lancés ensemble), la reprise par bail et les
    missions longues ;
  - tests :
    - `apps/web/e2e/scheduled-tasks.spec.ts` : l'occurrence manquée lue, puis une tâche
      créée, mise en pause, reprise, déplacée (le focus rendu à « Modifier ») et
      supprimée, chaque changement relu par l'API ; à vingt tâches, la note à la place du
      formulaire et le rythme « Du lundi au vendredi » ;
    - `apps/web/e2e/navigation.spec.ts` : la page se charge sans erreur ;
    - `crates/api/tests/scheduled_task_passes.rs` : une passe laisse à la suivante un lot
      de tâches en cours de modification.

## ADR-082 — L'auteur d'une tâche planifiée prévenu quand son run attend sa validation

- **Date** : 2026-10-23 · **Statut** : acceptée
- **Contexte** : un run lancé par une tâche planifiée (ADR-080) part sans que personne le
  suive. S'il demande la validation de son auteur, la demande attend dans
  « Validations », sans que l'auteur le sache, jusqu'à son échéance (ADR-061). L'ADR-080
  prévoyait de l'en prévenir.
- **Décision** :
  - quand le run qu'une tâche a lancé en dernier se met en pause, l'auteur reçoit un e-mail
    pour les demandes qu'il décide lui-même, si son compte et son entreprise sont actifs.
    Celles confiées à un responsable sont annoncées à ce responsable seul (ADR-060). Un run
    que l'auteur suit dans une conversation à lui ne l'est pas : il le voit. Le run est
    lié à sa tâche dans la transaction qui l'ouvre : avant de pouvoir s'exécuter, donc se
    mettre en pause ;
  - un e-mail par pause : le nom de la tâche, le nombre d'actions et leurs risques,
    l'échéance et le lien vers la conversation du run, où elles se décident. Ni les
    actions ni leurs paramètres : le message quitte la plateforme ;
  - chaque demande annoncée est tracée comme celle d'un responsable
    (`approval.notified`, acteur système, au nom de l'auteur, avec `approverId`). Une
    demande n'est donc jamais annoncée deux fois. Un message refusé par le relais n'est
    pas renvoyé ;
  - si un arrêt du service a perdu l'e-mail, il est envoyé au démarrage suivant, comme
    l'avis à un responsable (ADR-070), via `system_unnotified_scheduled_runs` (entreprise
    et identifiant seulement). Il n'y a pas de relance la veille de l'échéance : l'avis
    perdu est envoyé pour toute demande pas encore expirée, même proche de son échéance ;
  - sans relais SMTP, rien n'est envoyé.
- **Alternatives écartées** :
  - prévenir l'auteur de toute pause, même dans une conversation qu'il suit : il est
    devant ; l'e-mail serait du bruit ;
  - une relance la veille, comme pour un responsable : une tâche repart à son heure
    suivante. La pause retient les occurrences suivantes (ADR-080), et la page des tâches
    le dit.
- **Conséquences** :
  - aucune donnée nouvelle : la trace d'audit existante sert à n'annoncer qu'une fois ;
  - une décision reprend le même run : une seconde pause après une décision prise dans la
    conversation renvoie un e-mail à l'auteur, qui est pourtant devant. C'est conforme à
    « un e-mail par pause », mais à réévaluer à l'usage (ne prévenir que de la première
    pause, par exemple) ;
  - restent le worker séparé, la reprise par bail et les missions longues ;
  - tests :
    - `crates/api/src/services/runs/approval_notice/author_mail.rs` : texte du message ;
    - `crates/api/tests/scheduled_run_notices.rs` :
      - l'auteur prévenu une fois, avec le lien, sans l'action ;
      - ni au redémarrage, ni pour l'occurrence retenue, ni pour un run qu'il suit
        lui-même ;
      - une demande confiée à un responsable annoncée à lui seul ;
      - l'auteur ignore les demandes qu'il ne décide pas ;
      - un message refusé par le relais tracé en échec, et pas renvoyé ;
    - `crates/api/tests/scheduled_run_notice_catch_up.rs` :
      - l'avis perdu envoyé au démarrage, une fois ;
      - ni pour un auteur désactivé, ni dans une entreprise suspendue, ni pour une demande
        expirée, chaque demande retenue par elle-même, quoi qui liste son run ;
      - celui d'une autre entreprise envoyé et tracé chez elle seule ;
    - `crates/api/tests/scheduled_task_firing.rs` : le run est lié à sa tâche dès son
      ouverture ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est connue.

## ADR-083 — Worker séparé : les runs pris par bail (premier lot : les baux)

- **Date** : 2026-10-23 · **Statut** : acceptée · garde d'écriture mise en place par l'ADR-086
- **Contexte** : l'API exécute les runs dans son propre processus (ADR-016). Un verrou
  d'instance unique garantit qu'un seul processus les exécute et reprend au démarrage
  ceux qu'un arrêt a interrompus (ADR-053). Séparer l'exécution de l'API permettrait de
  redéployer l'API sans couper les runs et de répartir la charge sur plusieurs workers
  (ARCHITECTURE §11). Il manque pour cela trois choses :
  - savoir quel processus exécute quel run, et reprendre ceux d'un processus disparu
    sans attendre qu'il redémarre (baux, prévus par ADR-012) ;
  - que l'API suive les événements d'un run exécuté par un autre processus (§8) ;
  - un point d'entrée « worker », et des runs mis en file par l'API puis réservés par
    un worker.
- **Décision (cible)** :
  - un même programme, lancé dans l'un de trois rôles : `all` (par défaut, comme
    aujourd'hui), `api` (sert le HTTP, n'exécute rien) ou `worker` (exécute les runs, les
    tâches planifiées et les balayages) ;
  - chaque run en cours d'exécution est tenu par un bail : l'instance qui l'exécute
    (identifiant tiré au démarrage) et une échéance, renouvelée tant qu'il s'exécute. Un
    run dont le bail a expiré est repris par un exécuteur, sous les règles de l'ADR-053 ;
  - les événements des runs passent par PostgreSQL (`NOTIFY` dans la transaction qui les
    enregistre), et chaque processus les relaie à ses flux SSE ;
  - les runs créés par l'API sont mis en file (`queued`), puis réservés par un exécuteur
    (`FOR UPDATE SKIP LOCKED`). Un exécuteur qui a perdu son bail cesse d'écrire ;
  - le plan en lots :
    1. les baux (ce lot) ;
    2. les événements par PostgreSQL ;
    3. les rôles `api` et `worker`, la file d'attente, la fin du verrou d'instance
       unique et les piles Docker ;
    4. ensuite, les missions longues.
- **Décision (lot 1)** :
  - les colonnes `agent_runs.lease_owner` et `lease_expires_at`, réservées dès le schéma
    initial avec leur index, servent enfin : l'exécuteur y inscrit son identifiant (la
    migration 0030 fait de `lease_owner` un uuid). Un déclencheur les efface dès que le run
    quitte `running` (pause, fin, échec, balayage) : un run en pause ou terminé
    n'appartient à personne ;
  - chaque processus tire au démarrage un identifiant d'exécuteur. Il prend le bail d'un
    run dans la transaction même qui le fait passer à `running` : un nouveau tour de
    conversation, la décision ou l'expiration qui le relance, ou sa reprise après un arrêt,
    sous le verrou du run. Qui lance un run l'exécute : il n'existe pas de run `running`
    sans bail entre sa création et son exécution ;
  - tant que le run s'exécute, son bail (60 s) est renouvelé toutes les 20 s, pour les
    seuls runs que le processus exécute réellement. Il est rendu quand le run quitte
    `running`. Échéances écrites et comparées sur la seule horloge de la base
    (`clock_timestamp()`) : ni l'horloge d'un exécuteur ni l'attente d'un verrou ne les
    faussent ;
  - toutes les minutes, l'exécuteur reprend, sous les règles de l'ADR-053, les runs dont
    le bail a expiré et qu'il n'exécute pas lui-même. Seul le processus qui tient le verrou
    d'exécution le fait, une fois repris au démarrage les runs qu'un arrêt a interrompus,
    comme pour les demandes échues (ADR-061). Il trouve ces runs par
    `system_expired_run_leases`, qui ne donne que l'entreprise et l'identifiant, et
    revérifie l'échéance sous le verrou du run. Un run s'exécute ici tant que l'une de ses
    exécutions est en cours : celle qui finit d'enregistrer une pause ne retire pas celle
    qu'une décision vient de relancer. Un run dont l'exécution s'est perdue alors que le
    processus vit encore (son issue n'a pas pu être enregistrée, la base étant
    injoignable) n'attend donc plus le balayage des runs bloqués, un quart d'heure plus
    tard. Son message de fin reste celui d'un arrêt du service, que ce cas rejoint (et
    qu'il sera, au lot 3, l'arrêt d'un worker). Un run sans bail (créé avant cette
    migration) n'est repris qu'au démarrage ;
  - au démarrage, l'exécuteur, seul à tenir le verrou (ADR-053), reprend tous les runs
    `running`, baux compris : leur exécuteur s'est arrêté. Le verrou d'instance unique
    demeure jusqu'au lot 3 ;
  - le balayage des runs bloqués (ADR-016) reste : il fait échouer un run sans progrès
    depuis 15 minutes, que son bail soit vivant ou non (par exemple un appel au modèle qui
    ne revient pas).
- **Alternatives écartées** :
  - une file externe, ou pg-boss qu'évoquait l'ADR-012 : ce serait une infrastructure de
    plus, alors que des baux dans la table des runs suffisent ;
  - un bail renouvelé seulement à chaque étape : une étape (appel au modèle, outil) peut
    durer plus longtemps que le bail ;
  - un bail long sans renouvellement : un run perdu attendrait d'autant sa reprise ;
  - une réservation après coup, dans la tâche qui exécute le run : entre la transaction et
    la réservation, le run n'appartiendrait à personne. Au lot 3, la file d'attente
    réservera les runs mis en file par l'API (`FOR UPDATE SKIP LOCKED`).
- **Conséquences** :
  - rien ne change pour les utilisateurs : ce lot prépare le worker séparé ;
  - garde d'écriture (un exécuteur qui a perdu son bail n'écrit plus) : prévue au lot 3,
    sans objet tant qu'un seul processus exécute ;
  - tests : `crates/api/tests/run_leases.rs` (baux tenus) et `run_lease_takeover.rs` (reprise).

## ADR-084 — Worker séparé : les événements des runs par PostgreSQL (deuxième lot)

- **Date** : 2026-10-23 · **Statut** : acceptée · complète ADR-083 et ADR-016
- **Contexte** : le suivi en direct d'un run (SSE, ARCHITECTURE §8) passe par un bus en
  mémoire du processus qui l'exécute. Au lot 3 de l'ADR-083, l'API (rôle `api`) servira
  les flux de runs qu'un worker exécute : il lui faut leurs événements, sans nouvelle
  infrastructure.
- **Décision** :
  - chaque événement enregistré dans le journal d'un run est annoncé sur le canal
    PostgreSQL `mimorya_run_events` (`pg_notify`), **dans la transaction même qui
    l'enregistre** : l'avis part si, et seulement si, l'événement est enregistré, et dans
    l'ordre des enregistrements. Il ne porte que le processus d'origine, le run et le
    numéro de l'événement ;
  - chaque processus écoute ce canal sur une connexion à lui (rôle applicatif, nommée
    `mimorya-run-events`), hors du pool ; il ignore ses propres avis. Pour un run que l'un
    de ses abonnés suit, il relit le journal **dans l'entreprise de l'abonné** (celle dont
    le flux a vérifié que le run lui appartient ; l'avis ne nomme aucune entreprise) à
    partir du dernier événement que ses abonnés ont tous : celui que leur flux a rejoué,
    ou publié depuis. Un avis perdu entre-temps est lu avec le suivant. Ce point de
    départ est gardé avec les abonnés du run, et part avec le dernier ;
  - les avis sont reçus par une tâche à part, jamais interrompue en cours de route. Une
    fois sa connexion d'écoute rétablie, il relit les journaux de tous les runs suivis
    (les avis envoyés entre-temps sont perdus). De même après une relecture échouée, à
    une échéance fixe que les avis reçus entre-temps ne repoussent pas : l'avis suivant
    du run pourrait ne jamais venir (sa fin). Échéance d'une seconde, doublée à chaque
    nouvel échec jusqu'à 30 s ; de même pour une connexion d'écoute qui échoue ;
  - les fragments de texte du modèle (`assistant_delta`), qui ne sont pas enregistrés,
    voyagent dans l'avis lui-même, découpé sous la limite de PostgreSQL (8 000 octets).
    Ils partent dans l'ordre, hors transaction, regroupés par run, au mieux : une file
    pleine (1 024) en perd, que le message qui suit remplace. Ils ne partent que si un
    autre processus écoute (vérifié toutes les 5 s) : une instance unique n'en envoie
    aucun. Chacun porte le dernier événement que son exécution a enregistré avant lui
    (aucun avant le premier) ; un fragment plus ancien qu'un événement que les abonnés
    ont déjà est écarté (sinon il rouvrirait un brouillon que ce message a remplacé) ;
  - pour l'utilisateur d'une instance unique, rien ne change : elle reçoit tout
    directement. Ce lot prépare les rôles `api` et `worker` du lot 3.
- **Alternatives écartées** :
  - porter l'événement entier dans l'avis : un événement peut dépasser la limite de
    8 000 octets, et tout le journal (outils, résultats) passerait sur le canal commun,
    quand seuls les fragments de texte y passent, et seulement entre processus ;
  - annoncer l'événement après la transaction : un arrêt entre les deux le perdrait, et
    l'ordre ne serait plus celui du journal ;
  - Redis ou une file externe : une infrastructure de plus, que PostgreSQL rend inutile à
    cette échelle (ARCHITECTURE §8) ;
  - passer les fragments de texte par le journal : ils sont nombreux, et le message final
    les remplace.
- **Conséquences** :
  - une connexion PostgreSQL de plus par processus, gardée ouverte ;
  - **canal commun** : PostgreSQL ne contrôle aucun privilège sur un canal. Tout rôle
    connecté à la base peut y écouter (les fragments de texte de toutes les entreprises)
    ou y envoyer (un avis forgé, un texte). L'isolation des événements enregistrés tient
    à leur relecture dans l'entreprise de l'abonné ; celle des fragments, au run que le
    flux a vérifié. Condition d'exploitation : aucun autre rôle que l'applicatif (et le
    propriétaire, pour les migrations) ne se connecte à la base (SECURITY.md §3) ;
  - **coût** : chaque enregistrement d'événement fait un `pg_notify` de plus et prend, à
    sa validation, le verrou de la file des notifications de PostgreSQL ; ces
    validations se suivent donc une à une, toutes entreprises confondues. Si cette file
    se remplit (un écouteur figé qui garde sa connexion), les enregistrements échouent, et
    les runs avec eux : à surveiller (`pg_notification_queue_usage()`) avant le lot 3 ;
  - tests : `crates/api/tests/run_event_relay.rs` et, pour les pannes (lecture refusée,
    connexion d'écoute coupée puis refusée), `run_event_relay_faults.rs`.

## ADR-085 — Worker séparé : rôles `api` et `worker`, file d'attente des runs (troisième lot, première partie)

- **Date** : 2026-10-23 · **Statut** : acceptée · complète ADR-083 et ADR-084
- **Contexte** : les runs s'exécutent sous bail (ADR-083) et leurs événements passent
  d'un processus à l'autre par PostgreSQL (ADR-084). Reste à séparer l'API, qui sert le
  HTTP, de l'exécution, pour redéployer l'une sans couper l'autre et multiplier les
  instances de l'API. Le lot 3 de l'ADR-083 est découpé en trois : les rôles et la file
  d'attente (ce lot), la fin du verrou d'instance unique (plusieurs workers), puis les
  piles Docker.
- **Décision** :
  - un même programme, lancé dans l'un de trois rôles (`MIMORYA_ROLE`) :
    - `all` (par défaut, comme avant) : sert l'API et exécute les runs, en instance
      unique ;
    - `api` : sert l'API, plusieurs instances possibles. Il n'exécute aucun run, ne prend
      pas le verrou d'exécution, et n'a d'autre tâche de fond que le relais des
      événements (ADR-084) ;
    - `worker` : exécute les runs, dont ceux mis en file, et toutes les tâches de fond
      (maintenance, demandes échues et relances, tâches planifiées, baux, indexation de
      la mémoire) ; il ne sert pas de HTTP. Un seul processus `all` ou `worker` à la fois
      (verrou d'exécution, ADR-053), jusqu'au lot suivant ;
  - un run qu'un processus `api` lance (nouveau tour de conversation) ou relance (décision
    sur une demande) passe en `queued` dans la transaction même qui le crée ou le relance,
    au lieu de prendre un bail, et son journal le dit (événement `run_queued`, que le
    suivi en direct affiche « En file » jusqu'à ce qu'un worker le démarre ou le
    reprenne). Un avis sur le canal `mimorya_run_queue` réveille le worker à la
    validation ; celui-ci relit aussi la file toutes les 5 s, par lots de 100 tant
    qu'il en réserve, et s'arrête entre deux runs quand le service s'arrête ;
  - le worker liste les runs en file par `system_queued_runs` (entreprise et identifiant
    seulement), puis réserve chacun dans son entreprise (`FOR UPDATE SKIP LOCKED` : un run
    en cours de réservation est sauté sans attendre), le passe en `running` sous son bail
    dans la même transaction, puis l'exécute comme l'aurait fait l'API :
    - un run jamais exécuté repart de sa conversation : ses messages précédents, puis
      sa demande, retrouvée par l'identifiant que le run en garde
      (`agent_runs.request_message_id`) ; une demande introuvable termine le run ;
    - un run qui a enregistré une étape reprend de celle-ci, avec l'issue de ses
      demandes ;
    - les droits sont ceux de la personne pour qui il agit, tels qu'ils sont à cet
      instant : un compte désactivé, une entreprise suspendue ou l'usage de l'agent
      retiré entre-temps terminent le run avant qu'il ne démarre ;
  - un run resté plus d'une heure en file n'est pas exécuté : il se termine, comme un run
    qu'un arrêt a interrompu trop longtemps (ADR-053) ;
  - un run en file bloque un nouveau tour dans sa conversation, comme un run en cours ;
  - le worker, qui n'écoute pas les événements, envoie le texte du modèle dès qu'un autre
    processus écoute (ADR-084) ;
  - les caches que chaque processus garde (accès d'une entreprise, son modèle, son modèle
    d'embeddings, instances de connecteurs) et le réveil de l'indexation sont partagés :
    le processus qui modifie applique la modification chez lui, puis l'annonce aux autres
    sur le canal `mimorya_invalidations` (`{origin, kind, id}`), une fois sa transaction
    validée ; chacun applique celles des autres, sans les annoncer de nouveau
    (`services/invalidations/`). Une politique écrite par un processus `api` vaut donc
    aussitôt dans le worker (ADR-035). L'annonce est au mieux : une annonce perdue, la
    courte durée de vie des caches (une minute au plus) borne l'écart ; un processus qui
    perd sa connexion au canal vide ses caches une fois reconnecté. Les connecteurs gardent
    leurs instances, dont chaque usage vérifie la version ;
  - l'échec de la dernière passe d'indexation d'une entreprise est enregistré avec son
    choix de modèle (`tenant_embedding_settings.indexing_failure`), non plus dans la
    mémoire du processus qui indexe : le worker l'écrit, un processus `api` le montre. Il
    s'efface quand une passe réussit ou que le choix change, et n'est pas montré quand
    rien n'attend plus d'être indexé.
- **Alternatives écartées** :
  - une file externe : une infrastructure de plus, quand la table des runs et
    `SKIP LOCKED` suffisent ;
  - garder le mode d'exécution du run en base : il se déduit de son point de reprise (aucun
    pour un nouveau tour) et de ses demandes ;
  - la relecture périodique seule : jusqu'à 5 s d'attente avant chaque réponse ;
  - des caches sans invalidation entre processus, à durée de vie raccourcie : un retrait
    de droits n'y serait plus immédiat ;
  - des caches en base ou dans un service externe : une lecture de plus à chaque requête,
    ou une infrastructure de plus, pour des données qui changent rarement.
- **Conséquences** :
  - en instance unique (`all`), rien ne change pour l'utilisateur. Côté exploitation,
    le processus ouvre deux connexions d'écoute de plus (file et invalidations), relit
    `system_queued_runs` toutes les 5 s et envoie un `NOTIFY` par invalidation ;
  - un processus `api` sans worker laisse les runs en file : leur suivi montre « En file » ;
  - tests : `crates/api/tests/run_queue.rs`, `run_queue_access.rs`, `shared_caches.rs` ;
    parcours web `apps/web/e2e/queued-run.spec.ts` (un run en file que le serveur de bout
    en bout retient jusqu'à ce que le navigateur le suive).

## ADR-086 — Plusieurs workers : fin du verrou d'instance unique (troisième lot, deuxième partie)

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-083 et ADR-085, remplace la
  reprise au démarrage de l'ADR-053 et le rattrapage au démarrage de l'ADR-070
- **Contexte** : depuis l'ADR-085, l'API et l'exécution tournent dans des processus
  distincts, mais un seul processus `all` ou `worker` à la fois : il tient le verrou
  d'exécution (ADR-053). Ce verrou garantissait quatre choses :
  - au démarrage, chaque run resté `running` avait été interrompu, et pouvait être repris
    sans regarder son bail ;
  - un run ne s'exécutait qu'à un endroit : seul ce processus reprenait un bail expiré, et
    jamais celui d'un run qu'il exécutait encore ; la garde d'écriture de l'ADR-083 était
    donc sans objet ;
  - les balayages périodiques (demandes échues, relances, rattrapage des avis perdus) ne
    tournaient qu'une fois ;
  - l'indexation de la mémoire faisait une passe à la fois (un verrou en mémoire).
- **Décision** :
  - plus de verrou d'exécution : chaque processus `all` ou `worker` exécute des runs dès
    qu'il a démarré (`start_executing`) ; un processus `api` n'en exécute toujours aucun ;
  - un run n'est plus repris au démarrage que s'il l'aurait été par bail (ADR-083) : son
    bail a expiré, ou il n'en a jamais eu (un run laissé `running` sans exécuteur). Un run
    qu'un worker vivant tient n'est pas touché. Le premier qui verrouille un run le reprend ;
    les autres le trouvent repris ;
  - un exécuteur qui a perdu le bail d'un run s'arrête sans rien écrire (garde d'écriture
    prévue par l'ADR-083) :
    - chaque transaction où il modifie le run (journal, point de reprise, demandes,
      livrables, mémoire, issue) verrouille d'abord la ligne du run tant qu'elle porte son
      bail (`lock_held_lease`, `FOR NO KEY UPDATE`) : personne ne peut le reprendre avant
      la fin de la transaction, et s'il a été repris, rien n'est écrit. Un appel d'outil
      n'est lancé qu'après son événement `tool_call`, écrit sous ce verrou ;
    - ce qui dit ce qui s'est passé s'enregistre sans condition : l'audit, la consommation
      et l'appel d'outil terminé (`tool_invocations`, écrit au retour de l'outil). Un
      outil qui rend la main après la reprise du run par un autre exécuteur reste donc
      tracé, même si son résultat ne va plus au run ;
    - avant chaque étape, l'exécution vérifie que son bail n'a été ni trouvé perdu par un
      renouvellement, ni laissé sans renouvellement plus longtemps qu'un autre exécuteur
      n'attend pour le reprendre (durée du bail moins un intervalle de renouvellement) :
      au-delà, elle n'appelle plus ni le modèle ni un outil, et ne diffuse plus les
      fragments de sa réponse en cours ;
    - dans les deux cas, elle s'arrête sans enregistrer d'échec : le run est à l'autre. Une
      exécution qui plante n'écrit son échec que pour un run encore à elle ou déjà
      terminé ; un run repris, mis en pause ou en file entre-temps reste tel quel ;
  - un run repris note l'heure de sa reprise (`taken_up_at`). Le balayage des runs bloqués
    compte depuis la plus récente de sa dernière étape et de sa reprise : il ne l'échoue
    donc pas avant sa prochaine étape. L'heure au-delà de laquelle un run interrompu n'est
    plus repris se compte toujours depuis sa dernière étape : un worker qui disparaît
    entre la reprise et la première étape ne relance pas l'horloge ;
  - les balayages qui ne doivent tourner qu'une fois prennent chacun un verrou consultatif
    PostgreSQL de session à leur nom (`try_sweep_lock`, `sweep_lock`), sur une connexion à
    eux, rendu avec elle même si le processus disparaît :
    - expiration des demandes échues (ADR-061) et relances (ADR-062) : un balayage déjà en
      cours ailleurs est sauté, le suivant a lieu une minute plus tard ;
    - indexation de la mémoire (ADR-074) : attendue, une passe à la fois dans tous les
      processus ;
    - la connexion du verrou est sondée par le serveur (`tcp_keepalives_*` : toutes les
      30 s après une minute d'inactivité) : un hôte disparu libère le verrou en quelques
      minutes, pas en heures ; l'indexation n'attend pas plus de cinq minutes la passe en
      cours ailleurs, puis échoue et réessaie à la passe suivante ;
  - le rattrapage des avis perdus (ADR-070, ADR-082) devient périodique, à chaque balayage
    des demandes et sous son verrou, et non plus au démarrage : une demande jamais signalée
    dix minutes après la pause de son run est tenue pour perdue (un avis part en quelques
    secondes après la pause, et un run en pause ne change plus avant d'en sortir), et son
    avis est envoyé dans le balayage même. Un worker qui disparaît voit ses avis perdus
    rattrapés par un autre, sans attendre un redémarrage ;
  - ce qui était déjà sûr à plusieurs ne change pas : la file (`FOR UPDATE SKIP LOCKED`), les
    tâches planifiées (chaque occurrence réservée et replanifiée sous verrou), la prise d'un
    bail expiré (sous le verrou du run), les runs bloqués (une seule requête qui ne rend que
    ce qu'elle a changé) et les purges ;
  - `system_expired_run_leases` liste aussi les runs sans bail ; `system_interrupted_runs`,
    qui listait tout run `running`, disparaît.
- **Alternatives écartées** :
  - élire un worker pour tous les balayages : un bail de plus à tenir et à reprendre, quand
    un verrou par balayage suffit et ne retient que ce balayage ;
  - garder la reprise immédiate au démarrage en donnant à chaque worker une identité stable,
    pour reconnaître ses propres runs : une configuration de plus pour gagner une minute au
    redémarrage ;
  - un verrou consultatif de transaction : une transaction ouverte pendant tout le balayage,
    appels au modèle et envois de courriels compris ;
  - rattraper les avis au démarrage seulement, avec un délai de grâce : les avis d'un worker
    disparu attendraient le redémarrage d'un autre.
- **Conséquences** :
  - on peut lancer autant de workers que voulu ; chacun réserve des runs en file, reprend
    ceux dont l'exécuteur a disparu et partage les balayages ;
  - une instance unique qui redémarre reprend ses runs interrompus quand leur bail expire :
    au plus une minute de bail et une minute de balayage, au lieu d'aussitôt ;
  - un avis perdu part dix à onze minutes après sa demande, au lieu du redémarrage suivant ;
  - chaque balayage exclusif ouvre une connexion pour son verrou, refermée à la fin ;
  - un exécuteur coupé de la base plus longtemps que son bail perd ses runs, même s'il
    revient : la réponse du modèle ou d'un outil en cours est perdue, l'autre exécuteur
    reprend depuis le dernier point de reprise, en nommant l'action dont l'effet n'est pas
    connu (ADR-053) ;
  - un arrêt du service entre l'envoi d'un avis rattrapé et son audit peut faire renvoyer
    cet avis par un autre worker : au pire un courriel en double, jamais un avis perdu ;
  - mise à jour : arrêter les processus `all` et `worker` de la version précédente avant de
    démarrer ceux-ci. Ils ne prennent ni les verrous de balayage ni la garde d'écriture :
    côte à côte avec les nouveaux, ils doubleraient relances et rattrapages ;
  - les limites de débit restent comptées par processus : plusieurs processus `api` en
    comptent chacun les leurs (SECURITY §4), jusqu'à ce qu'elles soient partagées ;
  - tests : `crates/api/tests/several_workers.rs`, `run_lease_fence.rs`, et ceux de la
    reprise, des demandes échues, des relances et du rattrapage, adaptés.

## ADR-087 — Piles Docker : un service `api` et un service `worker` (troisième lot, fin)

- **Date** : 2026-10-24 · **Statut** : acceptée · termine le lot 3 de l'ADR-083, après
  l'ADR-085 et l'ADR-086
- **Contexte** : le programme sait tourner en rôle `api` ou `worker` (ADR-085) et plusieurs
  workers peuvent tourner côte à côte (ADR-086). Les piles de démonstration (ADR-032) et
  de production (ADR-034) lançaient pourtant encore un seul conteneur en rôle `all`.
- **Décision** :
  - dans les deux piles, la même image `mimorya-api` sert deux services :
    - `api` (`MIMORYA_ROLE=api`) sert le HTTP derrière Caddy ; un run qu'il lance ou
      relance attend en file ;
    - `worker` (`MIMORYA_ROLE=worker`) exécute les runs et toutes les tâches de fond. Il
      n'ouvre aucun port et attend la fin des migrations (production) ou du jeu de
      démonstration ;
  - leurs variables communes (base, origine du web, fournisseur de modèle, clé maître,
    relais SMTP) sont écrites une fois, dans une extension du fichier compose
    (`x-backend-environment`) ; seuls le rôle et `TRUST_PROXY` (l'API, derrière Caddy)
    diffèrent ;
  - le nombre de workers vient de `MIMORYA_WORKERS` (1 par défaut, au moins 1 : sans
    worker, les runs resteraient en file ; dans `.env` ou le fichier d'environnement de la
    production), que chaque `up -d` réapplique : un `--scale` ne
    vaudrait que jusqu'au `up` suivant. L'API reste unique dans la pile : ses limites de
    débit sont comptées par processus (SECURITY §4) ;
  - le worker a deux minutes pour s'arrêter (`stop_grace_period`) : il ne prend plus de
    run, et ceux en cours continuent jusqu'à leur fin ou leur pause. Un run encore en cours
    au-delà est repris par un autre worker quand son bail expire (ADR-083) ;
  - les tests des piles vérifient les rôles dans la base. Chaque worker garde une
    connexion nommée `mimorya-run-queue` (attente des runs en file), chaque processus
    `api` une connexion `mimorya-run-events` (relais des événements) ; une API qui
    exécuterait aussi des runs, ou un worker qui servirait aussi, en compterait une de
    plus (`check_roles`, `infra/docker/checks.sh`, deux lectures concordantes de suite). Le
    test de production ajoute un worker à ceux de la pile, puis vérifie qu'aucun des
    conteneurs ne tourne en root.
- **Alternatives écartées** :
  - garder un conteneur `all` en production et ne séparer qu'en SaaS : deux façons
    d'exploiter, et la séparation n'aurait été éprouvée nulle part ;
  - deux images (API et worker) : le même programme, compilé deux fois, pour un rôle qui
    tient dans une variable ;
  - plusieurs `api` derrière Caddy : sans limites de débit partagées, un client réparti
    entre les instances verrait les siennes multipliées ;
  - un contrôle de santé HTTP du worker : il ne sert rien. Sa connexion d'écoute dans la
    base dit qu'il est vivant et dans son rôle.
- **Conséquences** :
  - une pile compte un conteneur de plus ; une mise à jour (`pull`, puis `up -d`)
    recrée l'API et le worker, et un run interrompu par la mise à jour est repris par bail ;
  - passer d'une version antérieure à l'ADR-086 demande d'arrêter d'abord l'ancien
    conteneur `all` (ADR-086) : `docker compose … stop api`, puis `pull` et `up -d` ;
  - tests : `infra/docker/smoke-test.sh` (une API et les workers de la pile),
    `infra/docker/production-test.sh` (une API et les workers de la pile, puis un worker
    de plus), en CI. Le nombre attendu se lit dans la configuration de la pile
    (`stack_workers`), pas dans une constante.

## ADR-088 — Limites d'authentification partagées par tous les processus `api`

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-016, ADR-063 et ADR-085
- **Contexte** : les limites de débit sont comptées en mémoire, par processus (ADR-016).
  Avec plusieurs processus `api` (ADR-085), chacun compte les siennes, et une tentative
  répartie sur N instances dispose de N fois chaque limite : N fois plus de mots de passe
  essayés par compte, de liens envoyés par adresse, d'adresses sondées par la découverte
  du fournisseur d'identité. Les piles Docker n'en lancent donc qu'un (ADR-087).
- **Décision** :
  - les limites d'authentification sont comptées dans PostgreSQL, partagées par tous les
    processus : connexions par adresse cliente (`login`), échecs par compte
    (`failed_logins`, rendus en cas de succès), liens de réinitialisation par adresse
    e-mail (`reset_requests`), adresses essayées par la découverte ou le départ d'une
    connexion par le fournisseur (`sso_addresses`, une adresse déjà comptée ne coûtant
    rien) et étapes de ces connexions (`sso_sign_in`). Mêmes limites et mêmes fenêtres
    fixes qu'avant ;
  - deux tables non journalisées (`UNLOGGED`) les portent : `rate_limit_windows` (début
    de la fenêtre et compte, par limiteur et par clé) et `rate_limit_values` (les valeurs
    distinctes d'une clé). Elles ne gardent qu'une empreinte SHA-256 de chaque clé
    (adresse cliente, adresse e-mail) et de chaque valeur, calculée dans le processus ;
  - le rôle applicatif n'a aucun droit sur elles, et une politique y refuse toute ligne.
    Quatre fonctions `SECURITY DEFINER` y touchent seules :
    - `system_rate_limit_hit` compte un passage et rend s'il est permis. Le compte
      s'arrête à un au-delà de la limite, et rouvre une fenêtre une fois la précédente
      close ;
    - `system_rate_limit_release` rend un passage permis ;
    - `system_rate_limit_distinct` compte une valeur une fois par fenêtre ;
    - `system_purge_rate_limits` oublie les fenêtres closes ;
  - chaque appel est une transaction à lui, sur l'horloge de la base. Les passages d'une
    même clé sont comptés l'un après l'autre (verrou de sa ligne) : des tentatives
    simultanées sur plusieurs processus ne dépassent pas la limite ;
  - la maintenance périodique (dans chaque processus qui exécute les runs) purge les
    fenêtres ouvertes il y a plus de deux heures, au-delà de la plus longue (une heure) ;
  - un décompte qui ne peut être lu est une erreur : la requête échoue, comme la connexion,
    qui lit la même base. Le retour d'une connexion par le fournisseur est alors
    éconduit, comme au-delà de la limite. Un passage qui ne peut être rendu reste compté ;
  - la limite globale par requête et par adresse (600 par minute) reste comptée en
    mémoire, dans chaque processus.
- **Alternatives écartées** :
  - un service de comptage (Redis…) : une infrastructure de plus (règle d'architecture
    1), pour quelques écritures par connexion ;
  - compter aussi la limite globale en base : une écriture par requête, sur toutes les
    routes, pour une protection grossière contre l'inondation, qu'un reverse proxy peut
    aussi porter. Multipliée par le nombre de processus, elle reste bornée ;
  - réserver des tranches de quota par processus pour la limite globale : moins
    d'écritures, mais des parts perdues et un code plus difficile à prouver ;
  - garder les clés en clair : des adresses e-mail et IP en base, sans besoin ;
  - l'affinité de session du reverse proxy : elle dépend de l'installation, et l'adresse
    cliente qu'elle suit n'est pas la clé des limites par compte ou par adresse e-mail.
- **Conséquences** :
  - plusieurs processus `api` peuvent tourner sans multiplier les limites
    d'authentification. Les piles Docker gardent une seule API : Caddy n'en répartit
    pas plusieurs ;
  - chaque tentative de connexion, demande de lien ou étape d'une connexion par le
    fournisseur coûte une à trois écritures de plus en base (une connexion réussie par
    mot de passe : la connexion par adresse, l'échec réservé puis rendu) ;
  - une panne de la base vide les fenêtres : un client y gagne une fenêtre neuve ;
  - tous les processus doivent avoir les mêmes réglages
    (`LOGIN_RATE_LIMIT_PER_MINUTE`), sans quoi chacun compare le compte partagé à sa
    propre limite ;
  - tests : `crates/api/tests/shared_rate_limits.rs` (deux processus, empreintes hors de
    portée du rôle applicatif, purge) et `crates/db/tests/rate_limits.rs` (fenêtres,
    passage rendu, valeurs distinctes, purge).

## ADR-089 — Missions longues : des étapes enchaînées dans la conversation de la mission

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-053, ADR-083, ADR-085 et
  ADR-086
- **Contexte** : un tour de conversation dispose d'un budget borné (16 appels au modèle,
  10 minutes, 3 USD) : un travail plus long échoue sur « nombre maximal d'étapes
  atteint ». La vision produit veut qu'on confie un résultat, pas une suite de commandes
  (PRODUCT_VISION §4) ; la mission, « travail confié, suivi dans la durée, pouvant
  comporter plusieurs exécutions », a sa table depuis le schéma initial, jamais écrite.
  Un seul run au budget relevé garderait en contexte tous les échanges avec les outils,
  de plus en plus coûteux, et une panne ferait perdre tout ce qui suit son dernier point
  de reprise.
- **Décision** :
  - une mission est confiée par son auteur : un titre, un objectif, de un à huit
    critères de réussite. Elle ouvre sa propre conversation, à son nom ; trois au plus
    sont en cours par personne. Il faut, comme pour un tour, le droit `agent:use`, un
    modèle disponible et une entreprise active, et le droit de confier des missions
    (`mission:create`) ; les suivre et les arrêter demande `mission:read`. Ces deux
    permissions, au catalogue depuis le début sans être vérifiées, le sont désormais :
    une politique de refus peut les retirer ;
  - elle avance par **étapes** : chacune est un run de la conversation de la mission
    (`agent_runs.mission_id`), toujours mis en file pour un worker (ADR-085), au budget
    d'une étape (`MISSION_STAGE_BUDGET` : 24 appels au modèle, 15 minutes, 3 USD). La
    première a pour demande la mission elle-même, message de son auteur ; les suivantes,
    un mot de la plateforme ;
  - le budget d'une étape atteint, le moteur ne la fait plus échouer : il demande au
    modèle un **point d'étape** (ce qui est fait, avec les identifiants et les sources
    utiles, ce qui reste à faire, la prochaine action), sans outil : un outil appelé
    quand même est ignoré, jamais exécuté. Ce point coûte un appel au-delà du budget,
    pas plus. L'étape se termine (`completed`), son point affiché dans la conversation ;
  - dans la transaction qui enregistre cette fin, sous le bail de l'étape (ADR-086) et
    la mission verrouillée : tant que les limites de la mission le permettent (six
    étapes, 12 USD, gardées sur la mission telles qu'à sa création), l'étape suivante
    est créée et mise en file ; au-delà, la mission s'arrête inachevée (`failed`).
    Chaque étape dispose du budget d'une étape, son coût borné par ce qui reste à la
    mission ;
    Aucune étape ne se perd entre deux : un arrêt du service laisse la suivante en file,
    prise par le premier worker ;
  - l'étape suivante repart de la conversation de la mission (la mission et les points
    d'étape, pas les échanges avec les outils : un contexte borné, l'historique en ajout
    seul) et des **sources** que les étapes précédentes ont trouvées, gardées sur la
    mission (`missions.evidence`, les 200 dernières) : la réponse finale peut citer ce
    qu'une étape antérieure a lu (ancrage, AGENT_ENGINE §6) ;
  - une réponse finale, sans appel d'outil, clôt la mission (`done`). Elle est revue
    par un modèle au regard des critères de réussite, quel que soit `MIMORYA_LLM_JUDGE`,
    avec la correction bornée habituelle. Une étape qui échoue (erreur du modèle,
    droits retirés…) clôt la mission (`failed`). Le statut écrit sur la mission est
    celui de sa fin, plantage de l'exécuteur compris ; en cours, il se lit sur sa
    dernière étape : en attente d'une décision, ou échouée par un chemin qui ne passe
    pas par la fin d'une étape (balayage des runs bloqués) ;
  - un message est horodaté à son écriture (`clock_timestamp()`), et non au début de
    sa transaction : le point d'étape et la demande de l'étape suivante, écrits
    ensemble, gardent leur ordre dans la conversation ;
  - son auteur peut **arrêter** sa mission : l'étape en cours est annulée
    (`cancelled`), ses demandes de validation aussi, et un message le dit dans la
    conversation. L'exécuteur qui la tenait perd son bail avec le statut (déclencheur
    d'ADR-083) et s'arrête à sa prochaine écriture, sans rien écrire ni appeler de plus
    (ADR-086). L'étape est verrouillée avant la mission, dans l'ordre où se termine une
    étape : une étape qui se termine pendant l'arrêt est attendue, et c'est la
    suivante, qu'elle a mise en file, qui est annulée. Une mission terminée, ou dont la
    dernière étape a échoué, ne s'arrête plus (409) : son échec garde sa cause ;
  - retirer `mission:create` empêche d'en confier de nouvelles, pas de poursuivre celles
    en cours ; retirer `agent:use` (ou désactiver le compte) arrête l'étape en cours, et
    donc la mission ;
  - les sources montrées avec une réponse sont les 50 plus récentes : celles de l'étape
    viennent après celles des étapes précédentes ;
  - routes : `GET` et `POST /api/missions`, `GET /api/missions/{id}`,
    `POST /api/missions/{id}/stop`. La mission d'un autre répond 404, même à un
    administrateur.
- **Alternatives écartées** :
  - un seul run au budget relevé : un contexte qui grossit à chaque appel d'outil (coût,
    limite de contexte), et une panne qui fait tout perdre au-delà du dernier point de
    reprise ;
  - un outil de note que le modèle appellerait de lui-même : rien ne garantit qu'il le
    fasse avant la fin du budget, alors que le point demandé à la fin de l'étape est
    systématique ;
  - un résumé par un appel séparé, hors contexte, comme la leçon (ADR-076) : moins
    fidèle que le modèle qui a les résultats des outils sous les yeux, et sans le cache ;
  - créer l'étape suivante après la transaction : un arrêt du service entre les deux
    laisserait la mission sans étape ;
  - écrire l'échec de la mission sur chaque chemin qui fait échouer un run : ils sont
    nombreux, dont une fonction SQL inter-entreprises ; lire le statut sur la dernière
    étape suffit.
- **Conséquences** :
  - un travail long n'échoue plus au bout de 16 appels : il avance par étapes, chacune
    lisible par son point d'étape ;
  - une mission coûte au plus ses limites, plus, par étape, l'appel qui franchit son
    budget et son point d'étape, et la revue de la réponse finale ;
  - jusqu'au lot 2, la conversation d'une mission montre le mot de la plateforme qui
    lance une étape comme un message de son auteur ;
  - restent la page « Missions » et l'affichage des points d'étape (lot 2), l'avis à
    l'auteur quand une étape attend sa validation ou que la mission se termine (lot 3),
    et des limites réglables par l'entreprise ;
  - tests : `crates/agent/tests/runtime_handoff.rs` (point d'étape, outil ignoré,
    sources des étapes précédentes), `crates/api/tests/missions.rs` (étapes enchaînées
    jusqu'à la réponse revue, limites, budget d'une étape borné par ce qui reste, étape
    en échec), `mission_stop.rs` (arrêt d'une étape en file, en attente d'une décision,
    en cours, pendant qu'une étape se termine ; refus après un échec) et
    `mission_access.rs` (mission
    d'un autre, droits refusés par une politique, critères, trois en cours au plus).

## ADR-090 — Page « Missions » et étapes d'une mission dans sa conversation (deuxième lot)

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-089
- **Contexte** : les missions (ADR-089) ne se confiaient et ne se suivaient que par l'API,
  et la conversation d'une mission montrait le mot de la plateforme qui lance une étape
  comme un message de son auteur.
- **Décision** :
  - une page « Missions » dans la navigation, pour qui a le droit de les suivre
    (`mission:read`). Une carte « Nouvelle mission » : intitulé, objectif, critères de
    réussite, un par ligne (huit au plus, ce que dit son libellé). Sans le droit de
    confier des missions ou d'utiliser l'agent, ou avec trois missions en cours, la carte
    le dit à la place du formulaire ; l'API vérifie de toute façon, et ses refus
    (missions en cours, critères, modèle indisponible) s'affichent tels quels ;
  - chaque mission de son auteur, de la plus récente à la plus ancienne, dit son statut
    (« En cours », « En attente de validation », « Terminée », « Inachevée »,
    « Arrêtée »), où elle en est au regard de ses limites (étape, coût estimé), son
    objectif et ses critères, sa réponse finale avec sa revue, ou la raison de son
    échec. Elle mène à sa conversation, et « Arrêter », nommé par son intitulé, l'arrête
    après confirmation ;
  - la revue dit « Critères vérifiés » seulement si un relecteur a rendu un verdict
    favorable ; « Points non vérifiés » si des manquements restent ; « Critères non
    contrôlés (relecture indisponible) » si la réponse passe sans verdict exploitable
    (relecteur en échec, réponse sans verdict ou verdict écarté) : jamais une réponse
    non jugée n'est dite conforme à ses critères ;
  - la page relit la liste toutes les 10 secondes tant qu'une mission est en cours ;
  - dans la conversation, la partie de la mission que porte un message (ADR-089) se
    voit : la mission confiée, message de son auteur, porte le badge « Mission
    confiée » ; le point d'étape, réponse de l'agent, le badge « Point d'étape » ; le
    mot qui lance l'étape suivante est une note centrée de la plateforme, et non une
    bulle de l'auteur ;
  - le serveur des tests de bout en bout joue les missions à part, reconnues à leur
    consigne (`e2e_server/missions.rs`) : la première étape cherche les interventions de
    la semaine jusqu'à son budget, puis écrit son point d'étape ; la seconde répond, et
    la revue l'accepte. Le modèle d'une mission dont l'intitulé demande à être arrêtée
    prend vingt secondes à chaque appel, le temps de l'arrêter ; une mission dont
    l'objectif le demande répond aussitôt, et son relecteur ne rend pas de verdict.
- **Alternatives écartées** :
  - un flux d'événements par mission, ou par personne : les événements d'une étape
    passent déjà par sa conversation (suivi en direct du run), et l'état d'une mission
    ne change qu'à la fin d'une étape ; un flux de plus serait à relayer entre processus
    (ADR-084) pour quelques changements par mission ;
  - les étapes détaillées sur la page : sa conversation les montre déjà, avec leurs
    points d'étape, et la page y mène ;
  - les missions parmi les conversations seulement : rien n'y dirait leur statut, leurs
    limites, ni ne permettrait de les arrêter.
- **Conséquences** :
  - l'état d'une mission sur la page a au plus dix secondes de retard ; sa conversation,
    elle, suit l'étape en cours en direct ;
  - restent l'avis à l'auteur quand une étape attend sa validation ou que la mission se
    termine (lot 3), et des limites réglables par l'entreprise ; un verdict qui compte
    plus de dix manquements est aujourd'hui écarté, la réponse passant non jugée, au lieu
    d'échouer (antérieur à ce lot, noté dans la feuille de route) ;
  - tests :
    - `apps/web/e2e/missions.spec.ts` : une mission confiée depuis la page va d'étape en
      étape jusqu'à sa réponse revue, que la page montre sans être rechargée, puis relue
      après rechargement ; sa conversation montre la mission confiée, le point d'étape,
      le mot qui lance la seconde étape et la réponse ; une réponse que le relecteur n'a
      pas jugée n'est pas dite conforme ; à trois missions en cours, la page le dit à la
      place du formulaire, et l'une d'elles est arrêtée depuis la page, l'arrêt relu après
      rechargement ;
    - `apps/web/e2e/navigation.spec.ts` : la page se charge sans erreur.

## ADR-091 — L'auteur d'une mission prévenu par e-mail (troisième lot)

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-082, ADR-089 et ADR-090
- **Contexte** : une mission (ADR-089) avance en arrière-plan. Quand une de ses étapes
  demande une validation à son auteur, elle attend sans que personne le voie, jusqu'à
  l'expiration de la demande ; quand elle se termine, son auteur ne l'apprend qu'en
  ouvrant « Missions ». C'est le cas du run d'une tâche planifiée, dont l'auteur est
  prévenu depuis ADR-082.
- **Décision** :
  - **étape en attente** : l'avis à l'auteur d'une tâche planifiée vaut aussi pour
    l'étape d'une mission en cours. Une fois par pause, son auteur est prévenu des
    demandes qu'il décide lui-même (celles qu'un circuit confie à un responsable vont à
    ce responsable, ADR-060). L'e-mail nomme la mission (« Votre mission « … » attend
    votre validation »), le nombre d'actions et leurs risques, l'échéance, et mène à sa
    conversation ; ni les actions ni leurs paramètres. Un avis qu'un arrêt du service a
    perdu part dix minutes après la pause, par le rattrapage périodique (ADR-070,
    ADR-086), à partir de `system_unnotified_mission_runs`. Les requêtes de l'avis à
    l'auteur (`author_notices`) couvrent les deux origines ;
  - **mission terminée** : son auteur est prévenu une fois, qu'elle soit menée à bien
    ou arrêtée inachevée (limites atteintes, étape en échec, exécuteur arrêté, balayage
    des runs bloqués), mais pas quand il l'a arrêtée lui-même. L'e-mail dit si sa
    réponse finale a été vérifiée au regard de ses critères, si des points restent non
    vérifiés, ou si elle n'a pas pu être relue (ADR-090), ou qu'elle s'est arrêtée
    inachevée, et mène à « Missions » ; ni sa réponse ni la raison de l'arrêt, que la
    page montre après connexion ;
  - cet avis part de la passe périodique, chaque minute, dans un processus qui exécute
    les runs et un seul à la fois (verrou de balayage, ADR-086), pour les missions
    terminées depuis moins d'un jour (`system_unnotified_mission_ends`, entreprise et
    identifiant seulement), chacune relue ensuite dans sa propre entreprise. Leur
    entreprise et le compte de leur auteur doivent être actifs. La mission garde
    l'heure de l'avis (`missions.end_notified_at`) ; un message que le relais refuse est
    noté de même, et ne repart pas ; sans relais, rien ne part. Chaque avis est tracé
    (`mission.notified`, son issue et le statut annoncé), sans l'adresse ni la réponse
    du relais.
- **Alternatives écartées** :
  - envoyer l'avis de fin au moment où la mission se termine : les chemins qui la
    terminent sont nombreux (fin d'une étape, plantage de l'exécuteur, reprise, balayage
    des runs bloqués, dont une fonction SQL inter-entreprises), et il faudrait de toute
    façon une passe qui rattrape les avis perdus. Une seule passe les couvre tous, avec
    au plus une minute de retard, sans relier l'envoi d'e-mails au résultat d'une étape ;
  - tenir la trace des avis de fin dans le journal d'audit, comme les demandes : à
    chaque minute, chercher l'absence d'une entrée pour chaque mission parcourrait le
    journal. La colonne, couverte par un index partiel, le dit directement ;
  - prévenir aussi l'auteur qui a arrêté sa mission : il le sait ;
  - prévenir d'une fin ancienne : une mission terminée quand aucun relais n'était
    configuré n'est pas annoncée des jours plus tard, une fois qu'il l'est.
- **Conséquences** :
  - l'auteur d'une mission apprend par e-mail ce qui l'attend et quand elle se termine,
    sans surveiller la page ;
  - un arrêt du service entre l'envoi et son enregistrement peut faire partir un avis
    deux fois, comme pour les demandes (ADR-070) ;
  - restent des limites réglables par l'entreprise, et le choix, pour chacun, des avis
    qu'il reçoit ;
  - tests : `crates/api/tests/mission_stage_notices.rs` (étape en attente : une fois,
    sa conversation, ni les actions ni leurs paramètres ; avis perdu rattrapé une fois,
    dix minutes après la pause) et `mission_end_notices.rs` (mission menée à bien, une
    fois, réponse vérifiée ou non relue ; inachevée par ses limites ou par le balayage ;
    rien pour une mission arrêtée par son auteur, terminée depuis plus d'un jour ou dont
    l'auteur est désactivé ; rien sans relais ni dans un processus qui n'exécute pas
    les runs, ni pendant qu'un autre tient la passe, puis un seul avis pour deux
    workers à la fois ; message refusé par le relais tracé et non renvoyé ; missions
    de chaque entreprise lues dans la seule entreprise), le statut annoncé selon la
    revue dans `services/missions/end_notice/mod.rs`, les textes dans
    `end_notice/mail.rs` et `approval_notice/author_mail.rs`.

## ADR-092 — Un verdict de relecture garde ses premiers manquements

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-089 et ADR-090
- **Contexte** : le verdict du modèle relecteur (`{ pass, issues[] }`, AGENT_ENGINE §6)
  était écarté dès qu'il comptait plus de dix manquements. La réponse passait alors
  comme non jugée, donc acceptée : une réponse très insuffisante passait plus
  facilement qu'une réponse presque juste. Depuis ADR-090, une telle réponse est dite
  « Critères non contrôlés », ce qui est honnête, mais reste un échec manqué
  (constat noté par l'audit d'ADR-090).
- **Décision** :
  - un verdict qui compte beaucoup de manquements est **retenu** : la réponse échoue
    et passe par la correction bornée habituelle ;
  - le verdict garde ses **dix premiers manquements**, chacun coupé à 500 caractères,
    et une ligne compte les autres (« Et N autres manquements signalés par le contrôle
    qualité. ») : ce que le relecteur écrit est montré avec la réponse et renvoyé au
    modèle, sa taille reste donc bornée ;
  - chaque manquement est **nettoyé** avant d'être répété : sur une ligne, sans
    caractères invisibles, incapable d'ouvrir ou de fermer une enveloppe
    (`neutralise_envelopes`), comme une leçon (ADR-076). Le relecteur lit la réponse,
    qui peut citer un résultat d'outil, et ce qu'il écrit revient à l'agent dans un
    message de la plateforme ; un manquement vide une fois nettoyé est ignoré ;
  - n'est plus écarté qu'un verdict qui n'en est pas un (pas de `pass` booléen,
    manquements qui ne sont pas des textes) : la réponse reste alors non jugée, comme
    quand le relecteur échoue.
- **Alternatives écartées** :
  - garder l'écart et seulement l'afficher : un relecteur qui trouve beaucoup à redire
    serait toujours ignoré ;
  - transmettre tous les manquements : un relecteur peut en écrire sans fin, et ils
    voyagent avec la réponse et dans la conversation du modèle ;
  - faire échouer la réponse sans détailler : l'agent ne saurait pas quoi corriger.
- **Conséquences** :
  - une réponse jugée très insuffisante échoue et dit pourquoi, au lieu de passer non
    jugée ;
  - le panneau d'activité, qui compte les lignes du rapport, annonce alors onze points
    pendant la correction : les dix premiers manquements et la ligne qui compte les
    autres, comme les affiche la réponse ;
  - tests : `crates/agent/src/runtime/verification.rs` (dix premiers gardés, les autres
    comptés au singulier comme au pluriel, manquement trop long coupé, enveloppe
    forgée, saut de ligne et caractère invisible neutralisés, ce qui n'est pas un
    verdict écarté) et `crates/agent/tests/runtime_verification.rs` (une relecture
    de quinze manquements fait échouer la réponse, la correction reçoit les dix premiers
    et le compte des autres, la réponse les affiche).

## ADR-093 — Limites des missions réglées par l'entreprise

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-089
- **Contexte** : les limites d'une mission (six étapes, 12 USD, ADR-089) étaient celles
  de la plateforme, pour toutes les entreprises. Une entreprise peut vouloir des missions
  plus courtes et moins chères, ou au contraire plus longues pour ses travaux de fond.
- **Décision** :
  - ceux qui gèrent les réglages de l'entreprise (`settings:manage`, sur toute
    l'entreprise) fixent dans « Modèle d'IA », section « Limites des missions », le
    nombre d'étapes et le coût d'une mission, entre 1 et 20 étapes et entre 1 et
    60 USD : ce que peuvent coûter vingt étapes de 3 USD, un plafond plus haut ne serait
    jamais atteint. La base porte les mêmes bornes ; l'API refuse au-delà, en le disant ;
  - sans réglage propre, les limites de la plateforme s'appliquent ; on y revient d'un
    bouton (`DELETE /api/admin/mission-settings`) ;
  - une mission garde les limites du jour où elle est confiée (ADR-089) : un
    changement vaut pour les missions confiées ensuite, jamais pour celles en cours ;
  - le budget d'une étape (24 appels, 15 minutes, 3 USD) reste celui de la plateforme,
    son coût borné par ce qui reste à la mission ;
  - chaque changement est tracé (`mission.limits_configured` avec les limites,
    `mission.limits_reset`) ;
  - routes : `GET`, `PUT` et `DELETE /api/admin/mission-settings` ; la lecture donne
    aussi les limites de la plateforme et les bornes, que la page affiche.
- **Alternatives écartées** :
  - régler aussi le budget d'une étape : il borne ce que garde le contexte d'un seul
    run et le temps d'un exécuteur ; le régler par entreprise relève d'un autre lot ;
  - régler aussi le nombre de missions en cours par personne : la limite de trois
    protège l'exécution des runs de tous, pas le budget d'une entreprise ;
  - appliquer un changement aux missions en cours : leur auteur les a confiées avec
    des limites qu'il voyait, et une mission pourrait se voir arrêtée en route.
- **Conséquences** :
  - une entreprise règle la durée et le coût de ses missions, dans les limites de la
    plateforme ;
  - tests : `crates/api/tests/mission_settings.rs` (une mission garde les limites du
    jour, après un changement comme un retour à celles de la plateforme ; le budget de
    sa première étape borné par elles ; bornes refusées, champ inconnu refusé ; seuls
    les administrateurs des réglages sur toute l'entreprise, ni un collaborateur ni un
    administrateur partiel ; les limites d'une autre entreprise restent les siennes),
    `services/missions/limits.rs` (bornes) et `apps/web/e2e/mission-limits.spec.ts`
    (réglées depuis « Modèle d'IA », relues après rechargement, reprises par une
    mission confiée ensuite, retour à celles de la plateforme).

## ADR-094 — Chacun choisit les e-mails qu'il reçoit

- **Date** : 2026-10-24 · **Statut** : acceptée · complète ADR-060, ADR-062, ADR-072,
  ADR-082 et ADR-091
- **Contexte** : la plateforme envoie plusieurs avis par e-mail : demandes qui attendent
  un responsable et leur relance, avis au remplaçant d'un responsable absent, run d'une
  tâche planifiée ou étape d'une mission qui attend son auteur, fin d'une mission.
  Chacun les recevait tous, sans pouvoir s'en passer.
- **Décision** :
  - dans « Mon compte », section « E-mails reçus », chacun choisit pour lui-même trois
    sortes d'avis : les demandes de validation qui l'attendent (comme responsable ou
    comme remplaçant, avec la relance de la veille) ; ses tâches planifiées et ses
    missions qui attendent sa validation ; la fin de ses missions. Tous sont reçus tant
    qu'on n'a pas choisi autrement ;
  - les e-mails d'invitation, de réinitialisation du mot de passe et d'absence déclarée
    par l'administrateur sont toujours envoyés : ils touchent au compte lui-même ;
  - l'avis qu'un destinataire a choisi de ne pas recevoir n'est pas envoyé, mais tracé
    comme les autres, refusé (`denied`, `optedOut` dans ses métadonnées) : il compte
    comme donné, ni rattrapé ni renvoyé ; une mission terminée garde l'heure de cet avis
    non envoyé. Le choix se lit au moment d'envoyer : le destinataire réel (le
    remplaçant d'un responsable absent, ADR-071) voit le sien appliqué ;
  - routes : `GET` et `PUT /api/account/notifications`, pour son propre compte
    seulement ; chaque changement est tracé (`account.notifications_configured`). Le
    choix est supprimé avec le compte.
- **Alternatives écartées** :
  - réglage par l'administrateur pour toute l'entreprise : l'e-mail est une affaire
    personnelle, et un responsable qui préfère « Validations » n'a pas à demander ;
  - ne rien tracer d'un avis non envoyé : le rattrapage (ADR-070) le chercherait chaque
    minute jusqu'à l'échéance de la demande ;
  - un choix par sorte d'événement plus fin (relance à part, remplaçant à part) : trois
    choix suffisent à ne recevoir que ce qui sert, et restent lisibles.
- **Conséquences** :
  - un responsable qui ne veut pas des e-mails trouve ses demandes dans « Validations » ;
    elles expirent à leur échéance comme les autres (ADR-061) ;
  - tests : `crates/api/tests/notification_settings.rs` (chacun son choix, champ
    inconnu ou manquant refusé ; un responsable qui l'a choisi ni prévenu, ni rattrapé,
    ni relancé, puis prévenu de nouveau une fois son choix changé ; l'auteur d'une
    mission ni prévenu de l'étape qui l'attend, ni rattrapé, ni de sa fin, chaque avis
    tracé refusé et non retenté), `notification_stand_ins.rs` (pendant l'absence d'un
    responsable, le choix du remplaçant s'applique, celui du responsable absent ne fait
    taire personne), la liste des routes de `first_login.rs` et
    `apps/web/e2e/notification-settings.spec.ts` (choisi dans « Mon compte », enregistré,
    relu après rechargement).

## ADR-095 — Documents de l'entreprise : lecture isolée, rangement et droits

- **Date** : 2026-10-24 · **Statut** : acceptée (lot 1a : lecture des fichiers ; lot 1b : dépôt,
  droits et rangement)
- **Contexte** : l'entreprise veut déposer ses documents (procédures, notes, rapports) pour
  que son agent y trouve des passages et les cite. Lire un PDF ou un fichier Word revient
  à analyser une entrée hostile : un fichier piégé peut faire planter, boucler ou épuiser
  la mémoire du programme qui le lit. Dans le processus de l'API, il ferait tomber le
  service de toutes les entreprises.
- **Décision** :
  - **formats acceptés** : PDF, Word (.docx), Markdown et texte. Le type d'un fichier est
    donné par son extension et confirmé par ses premiers octets, jamais par ce que dit le
    navigateur. Un document Word ancien (.doc, ou un .docx qui en est un) ou protégé par
    un mot de passe est refusé avec la marche à suivre ;
  - **lecture dans un processus à part** (`mimorya-extract`, caisse `crates/extract`) :
    - l'API le lance pour chaque fichier, avec un environnement vide ; le fichier passe
      sur son entrée standard et sa lecture revient en JSON sur sa sortie standard ;
    - avant de rien lire, le processus se borne lui-même : 1 Gio de mémoire, 20 s de
      processeur, aucun fichier écrit, 16 descripteurs, pas de vidage mémoire (ou moins,
      si les limites dont il hérite sont plus basses) ;
    - côté API, sa réponse est revérifiée : une lecture qui dépasse les pages ou le texte
      permis, ou sans texte, est tenue pour une défaillance ;
    - côté API : deux lectures à la fois au plus par processus, 30 s chacune (au-delà, le
      processus est tué), une réponse lue jusqu'à 8 Mio. Un processus qui meurt, déborde
      ou répond autre chose fait refuser le fichier ;
    - la lecture se fait au dépôt, de façon synchrone : celui qui dépose apprend tout de
      suite pourquoi son fichier est refusé (chiffré, numérisé, trop de pages), et rien
      d'illisible n'est gardé ;
  - **limites** (`core/src/documents/limits.rs`) :
    - 10 Mio par fichier, 500 pages par PDF, un million de caractères de texte ;
    - pour un .docx : 2 000 entrées au plus ; seul le corps `word/document.xml` est lu,
      à 50 Mio au plus et à 100 fois sa taille compressée au plus (une bombe de
      décompression s'arrête là) ; aucune entité XML développée hors celles que XML
      prédéfinit ;
    - un PDF qui s'ouvre sans mot de passe est lu, avec ou sans restrictions de son
      propriétaire ; un PDF qui en demande un est refusé ;
  - **texte nettoyé** : forme canonique (NFC), sans caractères cachés ni contrôles,
    tabulations en espaces, au plus une ligne vide de suite ; un PDF est lu page par page,
    les titres d'un .docx deviennent des titres Markdown (`#`), et seul le texte des
    passages de texte est lu (pas le texte supprimé en révision, pas les codes de champ) ;
  - **passages** (`core/src/documents/chunking.rs`) : le texte est découpé dès le dépôt en
    passages contigus, jamais à cheval sur deux pages, d'environ 1 200 caractères et
    2 000 au plus, coupés aux paragraphes, puis aux phrases, puis entre les mots ; un
    titre, précédé ou non d'une ligne vide, ouvre un passage et nomme ceux qui le suivent.
    Une citation donne ainsi la page et la section ;
  - **rangement** (lot 1b) :
    - le fichier d'origine est gardé dans PostgreSQL, dans une table à part (`bytea`,
      stockage externe, sous RLS), avec un quota par entreprise vérifié sous verrou dans
      la transaction du dépôt : aucune nouvelle infrastructure, l'isolation et les
      sauvegardes restent communes ;
    - pas de déduplication : un « déjà déposé » révélerait un document hors de portée ;
  - **droits** (lot 1b) :
    - deux espaces, l'entreprise et une unité, comme pour la mémoire partagée ;
    - `document:read` (délégable) lit les documents des unités de son périmètre, de ses
      unités et de leurs parentes, et ceux de l'espace de l'entreprise ;
    - `document:write` (dépôt et suppression dans les unités de son périmètre ; dans
      l'espace de l'entreprise, seulement s'il s'étend à toute l'entreprise) n'est **pas
      délégable** :
      un document écrit par l'agent deviendrait une source d'injection durable pour les
      runs de tous ses collègues ;
    - chaque document a une sensibilité, au-dessus de laquelle on ne lit ni ne dépose ;
  - **transport** (lot 1b) : le fichier est le corps brut de la requête, son nom et son
    espace passent dans la chaîne de requête, typée et stricte ;
  - **routes** (lot 1b) :
    - `GET /api/documents` : les documents lus (les 500 plus récents ; la pagination
      viendra avec la page, lot 3), les espaces où l'on dépose et, pour qui dépose,
      l'usage du quota de l'entreprise ;
    - `POST /api/documents?name=…&unitId=…&sensitivity=…` : le dépôt. L'espace est
      vérifié avant que le fichier ne soit lu : une unité que l'entreprise n'a pas répond
      404, un espace ou une sensibilité hors de ses droits 403 ; un fichier annoncé ou trouvé au-delà
      de 10 Mio n'est pas lu plus avant (413) ; un type refusé répond 415, un fichier
      illisible tel quel 422, avec la raison ; le quota atteint 409 ; trop de lectures
      en cours 503 ;
    - `GET /api/documents/{id}`, `GET /api/documents/{id}/file` (le fichier tel que
      déposé, en pièce jointe, de type fixé par son type reconnu) et
      `DELETE /api/documents/{id}` ; ce qu'on ne lit pas, ou qu'on ne peut supprimer,
      répond comme absent (404) ;
  - **quota** (lot 1b) : 1 Gio par entreprise par défaut (`MIMORYA_DOCUMENT_QUOTA_MB`),
    compté sous un verrou de l'entreprise dans la transaction du dépôt : deux dépôts
    simultanés se comptent l'un l'autre ;
  - **rôles** (lot 1b) : le rôle de base de la plateforme lit les documents, le
    gestionnaire des connaissances les dépose aussi ; la migration l'accorde aux rôles
    de la plateforme des entreprises déjà là. Dans la démonstration, les responsables
    d'équipe lisent et déposent dans leur périmètre, la direction dans toute
    l'entreprise ;
  - **trace** (lot 1b) : dépôt, téléchargement et suppression, et fichier que le
    processus n'a pu lire (`document.rejected`), sans nom ni texte ;
  - **suite** : recherche par les mots et par le sens et outils `document_search` et
    `document_read` avec citations (lot 2, ADR-096) ; page « Documents », et relais
    `/api` de l'interface vérifié pour un fichier proche de 10 Mio (lot 3, ADR-097).
- **Alternatives écartées** :
  - stockage objet (S3) : une infrastructure de plus pour des volumes que PostgreSQL
    tient ; il reste possible si les volumes le justifient un jour ;
  - lire dans le processus de l'API, protégé par `catch_unwind` : un dépassement de pile ou
    un épuisement de la mémoire arrête quand même tout le processus, et une boucle ne se
    tue pas ;
  - lire en arrière-plan dans le worker : le processus à part isole déjà, et un refus
    différé laisserait un document déposé mais illisible ;
  - envoi `multipart` : un analyseur de plus pour une entrée hostile, plusieurs fichiers
    par requête à borner, sans gain pour un fichier à la fois ;
  - `document:manage` : les permissions `*:manage` sont celles d'administration de toute
    l'entreprise ; déposer dans son unité n'en est pas une.
- **Conséquences** :
  - l'image de l'API contient `mimorya-extract`, à côté de `mimorya-api`, qui le trouve
    là ;
  - un PDF numérisé, sans texte, est refusé : la reconnaissance de caractères n'est pas
    prévue ;
  - en mode debug, `zlib-rs` est compilé optimisé : les tests construisent et lisent des
    flux qui gonflent au-delà d'un gigaoctet ;
  - tests :
    - `crates/core/src/documents/` : reconnaissance des types, refus, découpage ;
    - `crates/extract/tests/` (`pdf.rs`, `docx.rs`, `text.rs`) : texte lu page par page ;
      PDF à mot de passe, de trop de pages, sans texte ou qui n'en est pas un ; archive
      Word qui gonfle au-delà de son rapport ou de sa borne absolue, de trop d'entrées,
      sans corps ou chiffrée ; entité déclarée non développée ; rien de lu hors des
      passages de texte ; texte non UTF-8 ou binaire ;
    - `crates/api/tests/document_extraction.rs` : chaque type lu par le vrai processus ;
      ses limites, relues dans `/proc` pendant qu'il attend son entrée ; un PDF dont le
      contenu gonfle à 1,3 Gio le fait échouer, faute de mémoire, et le processus suivant
      lit normalement ; un processus qui ne finit pas est tué à son heure ; un qui meurt,
      inonde sa sortie, répond n'importe quoi ou au-delà des limites fait refuser le
      fichier ; l'environnement de l'API ne lui parvient pas ; au-delà de deux lectures
      à la fois, refus ;
    - `crates/core/tests/document_spaces.rs` (lot 1b) : qui lit et dépose où, sur
      l'entreprise de démonstration ; jamais l'agent ;
    - `crates/api/tests/documents.rs` : chaque type déposé, découpé à ses titres, lu,
      téléchargé tel quel et supprimé avec son fichier et ses passages ; chaque étape
      tracée sans le nom ; un fichier refusé dit pourquoi et rien n'en reste ;
    - `crates/api/tests/document_access.rs` : l'équipe voisine, un document au-dessus
      de l'habilitation, une suppression hors de son espace, une politique de refus sur
      `document:read` ou sur `document:write`, le document ou l'unité réelle d'une autre
      entreprise (par qui dépose partout dans la sienne), un dépôt sans
      l'en-tête ou hors de ses droits refusé avant la lecture du fichier, une unité qui
      contient des documents : autant de tentatives de contournement refusées ;
    - `crates/api/tests/document_uploads.rs` : taille annoncée ou réelle au-delà de la
      borne, chaîne de requête stricte, deux dépôts près du quota, processus défaillant
      tracé, trois lectures à la fois ;
    - `crates/db/tests/document_roles.rs` : la migration accorde les documents aux rôles
      de la plateforme déjà là, une seule fois, sans toucher aux rôles de l'entreprise ;
      `check_constraints.rs` et les tests du contrat : les valeurs des colonnes sont
      celles des énumérations ;
    - le test de fumée de la pile de démonstration dépose un document par le relais de
      l'interface et le supprime.

## ADR-096 — Documents : recherche par les mots et par le sens, outils de l'agent, citations

- **Date** : 2026-10-25 · **Statut** : acceptée (lot 2 des documents)
- **Contexte** : les documents déposés (ADR-095) sont gardés en passages cherchables par
  leurs mots, mais l'agent ne les lit pas encore. Il doit y trouver ce qui répond à une
  demande, lire la suite d'un passage et citer ses sources, sans jamais atteindre un
  document hors des droits de son utilisateur. La mémoire a déjà sa recherche hybride
  (ADR-074) et son modèle d'embeddings par entreprise.
- **Décision** :
  - **deux outils de la plateforme**, offerts à tout run dont l'utilisateur tient
    `document:read` (délégable), de risque « lecture » :
    - `document_search` : une demande (2 à 300 caractères) et au plus 10 passages
      (5 par défaut). Chaque passage revient avec son document (identifiant et titre), sa
      position, sa page et sa section ;
    - `document_read` : un document et la position du premier passage à lire (0 par
      défaut, sans borne : un document a autant de passages que de sections). Cinq
      passages reviennent à la fois, avec leur nombre total et la position où reprendre ;
  - **droits** : les deux outils filtrent avec l'autorisation de l'appel
    (`document_read_filter`, les droits en vigueur à cette étape) : espace de
    l'entreprise, unités lues, habilitation. Les documents hors de portée sont écartés en
    SQL avant toute comparaison. Un document hors de portée et un document inexistant
    reçoivent la même réponse (« Document introuvable ou non accessible »). Sans
    `document:read`, aucun des deux outils n'est montré au modèle, et un appel est refusé
    par le PEP ;
  - **données non fiables** : les passages sont remis au modèle dans l'enveloppe des
    résultats d'outils, que leur texte ne peut ni ouvrir ni fermer ;
  - **citations** : chaque document trouvé ou lu est cité une fois (`document`, son
    identifiant, son titre). Il figure parmi les sources de la réponse et rejoint celles
    d'une mission ;
  - **recherche hybride**, comme celle de la mémoire :
    - par les mots : plein texte français ;
    - par le sens, quand l'entreprise a choisi un modèle d'embeddings : le même modèle,
      la même similarité minimale et le vecteur de la demande, attendu 5 secondes au
      plus et compté au nom de l'utilisateur et du run ;
    - les deux classements sont fusionnés par rang réciproque (constante 60) ;
    - les vecteurs sont comparés exactement, sur les seuls passages lisibles ;
  - **indexation** : le même passage en arrière-plan donne leur vecteur aux mémoires, puis
    aux passages (texte envoyé : titre du document, section, passage). Il travaille par
    lots de 32, huit lots par entreprise, par source et par passage, avec les mêmes
    règles de refus, de panne et de changement de modèle. Un dépôt réveille l'indexeur.
    `system_tenants_awaiting_embeddings` rend aussi les entreprises dont des passages
    attendent leur vecteur (index `(tenant_id, embedding_model)`). Le rôle applicatif ne
    peut modifier, dans un passage, que son vecteur et son modèle ;
  - **retour aux mots** : il efface aussi les vecteurs des passages (trace
    `embedding.reset` : `passageVectorsCleared`). L'erreur d'indexation reste montrée
    tant que des mémoires ou des passages attendent, et l'écran du modèle d'embeddings
    dit que les passages des documents partent vers le fournisseur ;
  - la démonstration dépose deux documents fictifs : une charte qualité dans l'espace de
    l'entreprise et une procédure d'entretien dans celui du service technique.
- **Alternatives écartées** :
  - rappeler les passages proches à chaque tour, comme les mémoires : un document est
    plus long qu'une mémoire, et l'agent sait chercher quand la demande le justifie ;
  - un modèle d'embeddings propre aux documents : un second choix à faire et à payer pour
    un même usage ;
  - un index approché (HNSW) sur les passages : il filtrerait après coup et en
    manquerait ; à reconsidérer si les volumes le justifient ;
  - mettre le titre du document dans le plein texte des passages : il ne figure que dans
    le texte envoyé au modèle d'embeddings, la colonne générée restant celle du lot 1b.
- **Conséquences** :
  - le texte de tous les passages, quelle que soit leur sensibilité, part vers le modèle
    d'embeddings choisi par l'entreprise, comme celui des mémoires. Les passages trouvés
    partent vers le modèle de langage de l'entreprise ;
  - `platform_tools` reçoit le port des documents (`DocumentStore`), `EnginePorts.documents`
    aussi. `index_pending` compte séparément les mémoires et les passages ;
  - la page « Documents », les sources de type document dans la conversation et la
    progression de l'indexation des passages dans « Modèle d'IA » viennent avec le
    lot 3 (ADR-097) ;
  - tests :
    - `crates/agent/tests/document_tools.rs` : ce que trouve et lit un run de chacun, un
      document cité une fois même quand plusieurs de ses passages répondent, l'absent et
      l'inaccessible confondus, un document de plus de 2 000 passages lu jusqu'au bout,
      entrées strictes et bornées ;
    - `crates/db/tests/document_search.rs` : trouvé par le sens hors de tout mot commun,
      jamais hors de portée (équipe voisine, habilitation), ni d'un autre modèle, ni trop
      loin ; ce que trouvent mots et sens en tête ; lecture dans l'ordre ; file des
      passages à indexer ; rien d'un passage modifiable par l'application que son
      vecteur ; `memory_search.rs` : l'entreprise attend tant que ses passages n'ont pas
      leur vecteur ;
    - `crates/api/tests/document_search.rs` : un run trouve les passages de ses droits, pas
      ceux de l'équipe voisine ni au-dessus de son habilitation ; un passage ne ferme
      pas son enveloppe ; la réponse cite le document ; lecture dans l'ordre et reprise ;
      document hors de portée, d'une autre entreprise ou inexistant confondus ; un
      document de 2 050 sections lu jusqu'au bout ; sans `document:read`, ni offerts ni
      exécutés ;
    - `crates/api/tests/document_meaning.rs` : trouvé par le sens une fois indexé, dans
      les droits de chacun ; panne montrée quand seuls des passages attendent ; un dépôt
      réveille l'indexeur ; retour aux mots, vecteurs des passages effacés ; `admin_embedding.rs`, `memory_indexing.rs` : effacement tracé,
      passages indexés avec les mémoires.

## ADR-097 — Page « Documents » et sources de type document dans la conversation

- **Date** : 2026-10-25 · **Statut** : acceptée (lot 3 des documents)
- **Contexte** : les documents se déposaient, se lisaient et se cherchaient par l'API
  (ADR-095, ADR-096), mais aucun écran ne permettait de les déposer ni de les consulter. La
  conversation montrait une source de type document sans libellé ni moyen de l'ouvrir. Et
  l'administrateur ne voyait pas où en était l'indexation des passages.
- **Décision** :
  - **page « Documents »** dans le menu, pour qui tient `document:read` :
    - les documents lus, du plus récent au plus ancien, avec leur type, leur sensibilité,
      leur espace, leur taille et leur date ;
    - les 500 plus récents au plus (`LISTED`) : au-delà, la liste le dit (`truncated`),
      l'agent consultant aussi les plus anciens ;
    - chacun se télécharge tel que déposé, en pièce jointe ;
    - qui peut écrire son espace le supprime, après confirmation ;
  - **dépôt** depuis la même page, montré seulement à qui dépose quelque part :
    - un fichier, l'un de ses espaces et une sensibilité ;
    - l'usage du quota de l'entreprise est affiché ;
    - le fichier part tel quel (`api.upload`, `application/octet-stream`), décrit par la
      chaîne de requête stricte de l'API ;
    - au-delà de la borne de l'API, qu'elle donne avec la liste (`maxFileBytes`, 10 Mio),
      il n'est pas envoyé ;
    - un refus de l'API est montré avec sa raison ;
  - **relais `/api`** : Next.js garde le corps d'une requête jusqu'à
    `experimental.proxyClientMaxBodySize` et coupe au-delà sans le dire (10 Mo par
    défaut). La limite est portée à 11 Mo, une marge au-dessus de la borne de l'API :
    aucun fichier que l'API accepte n'est coupé. Un fichier de 10 Mio exactement est
    vérifié de bout en bout. La page n'envoie jamais un fichier plus gros. Un client qui
    en enverrait un par le relais peut recevoir l'erreur du relais plutôt que le 413 de
    l'API : celle-ci refuse sur la taille annoncée, sans lire le corps. Le relais attend
    l'API 60 s sans réponse (`proxyTimeout`, 30 s par défaut, le temps que peut prendre la
    lecture d'un fichier). Il ne sert qu'en développement et aux tests de bout en bout :
    dans les piles Docker, Caddy envoie `/api` directement à l'API ;
  - **conversation** : une source de type document s'affiche « Document » suivi de son
    titre, et se télécharge en un clic ;
  - **« Modèle d'IA »** : le nombre de passages de documents indexés s'affiche à côté de
    celui des mémoires (contrat : `embeddedPassages`, `passages`) ;
  - **parcours de bout en bout** :
    - le serveur e2e construit `mimorya-extract` (`MIMORYA_EXTRACTOR`) ;
    - il joue à part une question sur les documents (`documents.rs`) : le modèle cherche
      dans les documents, puis répond d'après la charte de démonstration.
- **Alternatives écartées** :
  - envoi `multipart` (déjà écarté par ADR-095) ;
  - aperçu du texte dans le navigateur : le fichier téléchargé tel quel suffit, sans
    rendre de contenu non fiable dans la page ;
  - une source qui mène à la page « Documents » plutôt qu'au fichier : la page ne
    surligne pas encore un document ;
  - laisser la limite du relais par défaut, juste à la borne de l'API : sans marge, le
    moindre écart entre les deux comptes couperait un fichier accepté.
- **Conséquences** :
  - le dépôt par glisser-déposer et le remplacement d'un document par une nouvelle
    version restent à faire ;
  - tests :
    - `apps/web/e2e/documents.spec.ts` :
      - Julien dépose une note dans son équipe ;
      - Lucas la lit et la télécharge à l'identique, sans pouvoir déposer ni supprimer ;
      - Julien la supprime ;
      - un fichier `.doc` est refusé avec sa raison ;
      - un fichier de plus de 10 Mo n'est pas envoyé ;
      - le relais porte un fichier de 10 Mio exactement jusqu'à l'API ;
      - l'agent répond d'après la charte et la cite, son fichier téléchargé depuis la
        conversation ;
    - `crates/api/tests/document_list.rs` : au-delà de 500 documents, la liste dit
      qu'elle s'arrête ; la borne de taille donnée avec elle ;
    - `semantic-memory.spec.ts` : les passages indexés comptés et affichés ;
    - `navigation.spec.ts` : la page s'ouvre sans erreur.

## ADR-098 — Sous-agents : délégation de tâches à des runs enfants

- **Date** : 2026-10-25 · **Statut** : acceptée (lot 1 des sous-agents : moteur et API)
- **Contexte** : une demande large mêle plusieurs recherches ou analyses (un état des
  interventions, puis la charte qui s'y applique, puis une relecture). Faites dans un seul
  run, elles allongent son contexte et s'enchaînent une à une. Le schéma réservait
  `parent_run_id` et `agent_profile`, et l'ADR-014 prévoyait qu'un sous-agent n'ait
  jamais plus de droits que son parent.
- **Décision** :
  - **outil `delegate_task`** de la plateforme, en lecture, pour qui tient `agent:use`
    (permission que le plafond d'un run peut retirer) :
    - il confie une tâche (2 000 caractères au plus) et un contexte facultatif (8 000) à un
      sous-agent d'un profil `research`, `analysis`, `planning` ou `quality` ;
    - le PEP le juge comme tout autre (schéma strict, autonomie, validation imposée par
      l'administrateur) ;
    - le moteur exécute ensuite chaque appel autorisé : la méthode `run` de l'outil
      répond par un refus si quoi que ce soit d'autre l'appelle ;
  - **run enfant** :
    - enregistré dans `agent_runs` (`parent_run_id`, `agent_profile`,
      `autonomy_level = read_only`), pour l'entreprise et l'utilisateur de son parent,
      repris de la ligne du parent ;
    - sans conversation ni mission ; son journal, ses appels d'outils et sa consommation
      sont les siens : ses outils de plateforme (mémoire, documents, compétences) passent
      par ses propres ports ;
    - ses consignes sont celles du parent, suivies de son rôle ; son tour porte la tâche,
      qui ne peut ouvrir ni fermer d'enveloppe, puis le contexte transmis, comme donnée
      non fiable (`<donnees_outil source="delegate_task">`) ;
  - **ce que peut l'enfant** :
    - il hérite du plafond de permissions du parent (`allowed_permissions`) et relit les
      droits de l'utilisateur à chaque étape, comme lui ;
    - il ne reçoit que les outils de risque « lecture » qui ne demandent pas de
      validation, tels que l'entreprise les a réglés ; `delegate_task` en est exclu ;
    - il ne peut donc ni écrire, ni mémoriser, ni communiquer vers l'extérieur, ni
      déléguer à nouveau ;
    - une demande de validation lui est refusée par ses ports ;
    - il ne tire pas de leçon ;
  - **résultat** :
    - complet, le parent reçoit le résumé de l'enfant et ses sources, comme données non
      fiables ; ses citations rejoignent celles du parent, dont la réponse peut les citer ;
    - sinon, le parent reçoit la raison de l'échec ;
    - l'appel est journalisé et audité avec le run enfant (`subRunId`) et son profil ;
  - **bornes** :
    - au plus 3 délégations par étape, autorisées ou soumises à validation ; au-delà,
      l'appel est refusé pour cette étape ;
    - celles d'une étape s'exécutent en parallèle ; celles validées par un humain
      s'exécutent ensemble, en parallèle, à la reprise ;
    - dans une étape qui délègue, une action à valider n'est demandée qu'une fois les
      délégations exécutées : une demande se décide dès qu'elle est montrée ;
    - chaque enfant reçoit une part égale de ce qui reste au parent (tokens et coût
      divisés, durée restante partagée, l'appel au modèle de l'étape compté), dans le
      plafond d'un sous-agent : 8 étapes, 200 000 tokens lus, 20 000 écrits, 1 USD,
      5 minutes ;
    - sans reste, l'enfant n'est pas ouvert ;
    - ce que l'enfant dépense (tokens, coût) compte dans le budget du parent ; ses
      étapes ne comptent pas ;
  - **bail et reprise** :
    - l'enfant s'exécute dans l'exécution de son parent et écrit sous le bail de
      celui-ci ; parent annulé ou repris ailleurs, l'enfant s'arrête avec lui ;
    - il n'est jamais repris seul : `system_expired_run_leases` l'écarte ;
    - parent repris après un arrêt, en panne ou arrêté avec sa mission : ses enfants
      encore en cours échouent, et leur fin est enregistrée et annoncée dans la même
      transaction ;
    - un déclencheur fait échouer les enfants encore en cours d'un parent qui se termine
      par un autre chemin. Le balayage des runs bloqués, par exemple, balaie l'enfant
      bloqué avec son parent et annonce leur fin ;
    - l'appel interrompu étant une lecture, le parent reprend et peut redemander ;
  - **API** :
    - `GET /runs` ne liste que les runs principaux ;
    - `GET /runs/{id}` donne le profil, le parent et les sous-agents (`subRuns`) ;
    - seul l'utilisateur du run le voit.
- **Alternatives écartées** :
  - **enfants mis en file pour un worker, le parent en pause jusqu'à leur fin** : un état
    de run de plus, des reprises croisées et un parent à réveiller, pour des tâches de
    quelques minutes que le parent attend de toute façon ;
  - **délégation comme outil ordinaire** : l'outil ne voit ni le budget restant du
    parent, ni ses autres délégations de l'étape, et son délai de 30 s ne tient pas un
    run ;
  - **permissions choisies par le modèle** : le plafond est hérité, jamais décidé par le
    modèle ;
  - **enfant qui demande des validations** : la validation reste une affaire du run que
    l'utilisateur suit.
- **Conséquences** :
  - migration `0041_sub_agents.sql` : profils permis, index des enfants, fonction des
    baux expirés, déclencheur de fin ;
  - la vue « Opérations » des sous-agents et l'affichage d'une délégation dans la
    conversation restent à faire (lot 2) ;
  - tests :
    - `crates/agent/tests/runtime_delegation.rs` :
      - résumé et sources rendus, cités par le parent ;
      - plafond hérité ; sans `agent:use`, pas de délégation ;
      - ni écriture, ni délégation, ni outil à validation pour l'enfant ;
      - mémoire lue par le run de l'enfant ;
    - `runtime_delegation_bounds.rs` :
      - trois délégations par étape, exécutées en parallèle ; celles à valider comptent
        et s'exécutent ensemble une fois validées ;
      - une validation demandée une fois les délégations exécutées ;
      - part du budget, dépense comptée chez le parent ;
      - échec expliqué ;
      - pas d'enfant sans reste de budget ;
      - délégation exécutée une fois validée ;
    - `crates/api/tests/sub_agents.rs` :
      - run enfant enregistré et visible du seul utilisateur, sources citées par la
        réponse, absent de la liste des runs ;
      - jamais repris seul ; échoue à la reprise, à la panne ou à la fin de son parent,
        ou avec sa mission arrêtée, sa fin annoncée ;
      - aucune entreprise n'ouvre d'enfant sous le run d'une autre.

## ADR-099 — Sous-agents dans la vue « Opérations »

- **Date** : 2026-10-25 · **Statut** : acceptée (lot 2 des sous-agents)
- **Contexte** : depuis l'ADR-098, l'API donne les sous-agents d'une demande
  (`subRuns`), leur profil et leur parent, mais l'interface ne les montrait pas. Qui
  consultait une demande voyait un appel à `delegate_task` sans pouvoir suivre ce que le
  sous-agent avait lu.
- **Décision** :
  - **détail d'une demande** (« Opérations ») : une section « Sous-agents » liste ceux qu'elle
    a lancés, dans l'ordre. Chacun montre son profil, sa tâche, son statut, ses outils et
    son coût, et mène à son propre détail. La section précise qu'une fois sa tâche rendue,
    le coût d'un sous-agent est compris dans celui de la demande, qui l'ajoute alors au sien
    (ADR-098) : qui additionnerait les lignes compterait deux fois. Celui d'un sous-agent en
    cours, ou arrêté avec sa mission, n'y est pas ajouté : il reste dans son propre détail ;
  - **détail d'un sous-agent** :
    - titre « Sous-agent « Recherche » » (son profil) ;
    - même résumé et mêmes outils que toute demande ;
    - un lien « Demande d'origine » vers son parent, à la place du lien vers la
      conversation, qu'il n'a pas ;
  - **liste des opérations** : inchangée. Les sous-agents n'y figurent pas, l'API ne
    listant que les demandes principales ;
  - **conversation** :
    - la délégation apparaît dans l'activité de la demande, comme tout outil : son titre
      et le résumé de son issue (« Sous-agent « Recherche » : résumé rendu ») ;
    - les sources lues par le sous-agent figurent avec la réponse ;
  - **serveur e2e** : il joue à part une question confiée à un sous-agent
    (`sub_agents.rs`). Le modèle délègue une recherche, le sous-agent cherche dans les
    documents et rend son résumé, puis le modèle répond d'après lui.
- **Alternatives écartées** :
  - lister les sous-agents parmi les opérations : l'utilisateur n'a demandé qu'une chose,
    et chaque sous-agent reste joignable depuis sa demande ;
  - suivre un sous-agent en direct depuis la conversation : son résumé suffit à la
    conversation, et son détail se consulte une fois la demande traitée.
- **Conséquences** :
  - libellés des profils dans `apps/web/lib/labels.ts` (`AGENT_PROFILES`) ; une ligne de
    run commune à la liste des opérations et à celle des sous-agents (`run-row.tsx`) ;
  - tests : `apps/web/e2e/sub-agents.spec.ts` :
    - réponse issue du résumé du sous-agent, charte qu'il a lue citée ;
    - demande listée seule ;
    - sous-agent nommé dans son détail ;
    - outils du sous-agent visibles ;
    - retour à la demande d'origine.

## ADR-100 — L'agent propose des compétences, validées par la personne puis par les administrateurs (premier lot : l'API)

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 1)
- **Contexte** : depuis l'ADR-052, une compétence ne s'écrit que dans l'administration.
  Une conversation où l'agent et la personne trouvent une meilleure façon de faire, ou
  une procédure que personne n'a écrite, n'en laissait rien aux autres : la leçon de
  l'ADR-076 reste personnelle. Le schéma prévoyait déjà des versions `candidate` et le
  run qui les écrit (`created_by_run_id`).
- **Décision** :
  - **permission `skill:propose`** (« Proposer des compétences », délégable) : donnée aux
    rôles « Collaborateur » et « Gestionnaire des connaissances » de la plateforme, y
    compris ceux des entreprises déjà là (migration 0042). La retirer d'un rôle suffit
    à ce que l'agent ne propose plus rien pour ses titulaires ;
  - **outil `skill_propose`** de la plateforme :
    - il propose soit la nouvelle version complète d'une compétence active (sans
      description), soit une nouvelle compétence (avec sa description). Une note d'une
      ligne dit pourquoi et devient celle de la version ;
    - il exige aussi `skill:read` : on ne réécrit pas ce qu'on ne peut pas lire. Une
      politique qui retire la lecture des compétences à l'agent lui retire donc
      l'outil, qui sinon lui apprendrait quels noms sont ceux de compétences actives ;
    - risque `draft`, et **validation toujours exigée** : le texte quitte la
      conversation pour des personnes qui ne pouvaient pas la lire (l'« Administrateur
      technique » n'a pas accès aux données métier). La personne servie, ou le
      responsable si l'entreprise le veut (ADR-059), lit la proposition exacte avant
      qu'elle parte, et elle part telle quelle ;
    - avant toute validation, ce qui est envoyé passe les règles de l'administration
      (ADR-052) : un nom que l'agent peut demander, une description (10 à 300
      caractères) et une note (3 à 300) d'une ligne visible, débarrassées de leurs
      blancs autour comme l'administration le fait, un contenu en texte visible d'au
      plus 20 000 caractères, normalisé comme une version écrite à la main ;
    - un sous-agent ne la propose jamais : il n'a que les outils de lecture sans
      validation (ADR-098) ;
  - **à l'exécution**, sous le verrou d'administration (ADR-020) et le bail du run
    (ADR-086), la règle pure `proposal_target` (`mimorya_core::skill_proposals`)
    décide :
    - une version d'une compétence active devient sa version suivante, `candidate` ;
    - une nouvelle compétence est créée en brouillon, sa première version `candidate` ;
    - **une compétence inactive** (brouillon ou retirée) n'est connue que de ses
      administrateurs : rien de ce que lit l'agent ne la distingue d'un nom qu'aucune
      compétence ne porte. Une version proposée sous son nom est refusée comme pour un
      nom inconnu ; une nouvelle compétence de son nom en devient une version
      `candidate`, que l'agent lit comme toute nouvelle compétence proposée (aucun
      numéro de version ne lui est rendu). Ses administrateurs en décident ;
    - une description donnée pour une compétence active, ou un nom inconnu sans
      description, sont refusés, et l'agent lit pourquoi ;
    - au plus 20 propositions attendent les administrateurs d'une entreprise, dont 5 au
      plus pour une même personne : au-delà, l'agent n'en propose plus avant qu'ils en
      aient décidé, et personne ne remplit seul la part de l'entreprise ;
  - **ce qu'une proposition garde** :
    - son auteur (la personne servie), son run, et qu'elle vient de l'agent (`proposed`,
      gardé une fois la décision prise) ;
    - pour une nouvelle compétence, la description proposée, dans l'en-tête de la
      version (`frontmatter`, comme l'écrit agentskills.io), montrée aux administrateurs
      (`proposedDescription`) : celle d'une compétence existante ne change pas sans eux ;
    - l'audit `skill.proposed` (acteur : l'agent, pour la personne, avec le run) nomme la
      compétence, la version et s'il s'agit d'une nouvelle compétence, jamais le texte ;
  - **les administrateurs** (`skill:manage` sur toute l'entreprise) :
    - voient le nombre de propositions en attente de chaque compétence (`proposals`) et,
      pour chaque version, si elle vient de l'agent (`proposed`) ;
    - rendent une proposition active comme toute version (ADR-052) ;
    - ou la rejettent (`POST /admin/skills/{id}/rejection`, `{ version }`, audit
      `skill.proposal_rejected`). Seule une proposition en attente se rejette. Rejetée,
      elle reste parmi les versions, telle qu'écrite, et peut encore être rendue active ;
  - rien n'atteint un run avant l'activation : les runs ne lisent que la version active.
- **Alternatives écartées** :
  - proposer sans validation de la personne, comme une mémoire partagée proposée
    (ADR-076) : un détail de la conversation partirait vers des administrateurs qui
    n'auraient pas pu le lire ;
  - une table de propositions à part : les versions `candidate` et leur run existaient
    déjà, et une proposition rendue active doit rester une version comme les autres ;
  - un réglage par entreprise : la permission suffit, et chaque proposition demande déjà
    deux décisions humaines ;
  - dire qu'un nom est pris par une compétence inactive : un membre, qui ne lit que les
    compétences actives, apprendrait l'existence de celles en préparation ou retirées ;
  - toujours faire valider la proposition par la personne servie, même quand
    l'entreprise fait valider les actions de son risque par le responsable (ADR-059) :
    le responsable, qui voit déjà les actions faites pour son équipe, lit le texte
    exact et décide comme pour toute action de ce risque.
- **Conséquences** :
  - le texte proposé est un contenu de l'entreprise, comme une version écrite par un
    administrateur. Il dure avec la compétence et disparaît avec elle, rejeté compris ;
    le lien à son auteur et à son run est oublié avec eux. L'outil demande un texte
    général, sans donnée personnelle, d'un client ou d'un tiers, et la personne le relit
    avant envoi ;
  - restent à faire :
    - lot 2 : l'écran « Compétences » (propositions à relire, activer ou rejeter) et son
      test de bout en bout ;
    - un avis aux administrateurs quand une proposition arrive ;
    - les évaluations et la non-régression avant activation ;
  - tests :
    - `crates/core/src/skill_proposals.rs` : où va une proposition, refus ;
    - `crates/agent/tests/skill_proposals.rs` :
      - validation demandée avant tout envoi, texte normalisé et débarrassé de ses
        blancs ;
      - nouvelle compétence ;
      - nom refusé une fois validé ;
      - texte invalide ou vide une fois les blancs retirés, refusé sans validation ;
      - rien sans la permission, sans la lecture des compétences, ni pour un
        sous-agent ;
    - `crates/api/tests/skill_proposals.rs` :
      - version candidate après validation, lue par aucun run avant l'activation ;
      - audit sans le texte ;
      - activation ;
      - nouvelle compétence en brouillon avec sa description proposée, proposition
        rejetée une seule fois puis encore activable ;
      - compétence inactive répondue comme un nom inconnu ;
    - `crates/api/tests/skill_proposal_bounds.rs` :
      - plafonds de l'entreprise et de la personne ;
      - validation par le responsable quand l'entreprise le veut ;
    - `crates/api/tests/skill_access.rs` : rejet refusé à un membre, compétence d'une
      autre entreprise introuvable ;
    - `crates/api/tests/policy_effects.rs` : la lecture des compétences refusée à
      l'agent lui retire aussi l'outil ;
    - `crates/db/tests/skill_proposal_roles.rs` : rôles des entreprises déjà là.

## ADR-101 — Les propositions de l'agent dans l'écran « Compétences »

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 2)
- **Contexte** : depuis l'ADR-100, l'API garde les compétences que l'agent propose et
  laisse les administrateurs les activer ou les rejeter. L'écran « Compétences » les
  montrait comme des versions parmi d'autres (libellé « Proposée »), sans dire qu'elles
  attendaient une décision, d'où elles venaient, ni comment les rejeter.
- **Décision** :
  - **en tête de l'onglet**, une ligne dit combien de propositions attendent une
    décision et pour quelles compétences ;
  - **chaque compétence** qui en a porte un badge « N proposition(s) à relire » : une
    nouvelle compétence proposée apparaît en brouillon avec le sien ;
  - **une version proposée** :
    - dit pour qui l'agent l'a proposée (« proposée par l'agent pour … ») au lieu de
      « écrite par » ;
    - montre sa note (pourquoi) et, pour une nouvelle compétence, la description
      proposée ;
    - en attente, s'ouvre avec son contenu déplié et porte, à côté de « Activer la
      version N », un bouton « Rejeter la version N » ;
  - une nouvelle version écrite à la main part de la dernière version qu'un
    administrateur a écrite, jamais d'une proposition (ADR-100) : vide quand la
    compétence n'a que des propositions, comme une nouvelle compétence que l'agent
    propose ;
  - **serveur e2e** : il joue à part une question dont l'agent tire une nouvelle
    compétence à proposer (`skill_proposals.rs`).
- **Alternatives écartées** : un onglet à part pour les propositions. Une proposition se
  décide à côté des versions de sa compétence, qu'elle remplacerait.
- **Conséquences** :
  - restent à faire : un avis aux administrateurs quand une proposition arrive ; les
    évaluations et la non-régression avant activation ;
  - test : `apps/web/e2e/skill-proposals.spec.ts` :
    - Lucas lit le texte exact et le valide ;
    - Paul trouve la nouvelle compétence en brouillon, sa proposition en attente, pour
      qui et pourquoi elle a été proposée, sa description ;
    - Paul lit son contenu déplié, la rejette, la relit rejetée après rechargement ;
    - une nouvelle version qu'il écrit part vide, non de la proposition ;
    - puis il supprime la compétence.

## ADR-102 — Les administrateurs des compétences sont prévenus des propositions par e-mail

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 3) ·
  complète ADR-094 et ADR-100
- **Contexte** : une proposition de l'agent (ADR-100) n'atteint aucun run avant qu'un
  administrateur des compétences l'active. Il ne l'apprenait qu'en ouvrant l'onglet
  « Compétences » (ADR-101).
- **Décision** :
  - **chaque minute**, un processus qui exécute les runs, un seul à la fois (ADR-086),
    rassemble les propositions de chaque entreprise en attente d'une décision et jamais
    annoncées (`system_unannounced_skill_proposals`, puis lecture dans l'entreprise) ;
  - **destinataires** : chaque compte qui peut en décider, actif et détenteur de
    `skill:manage` sur toute l'entreprise selon ses règles du moment, politiques de refus
    comprises, par la vérification même qui précède chaque modification d'administration
    (`holds_company_wide`, partagée avec `begin_change`, ADR-020). Un compte qui ne l'a
    que sur une unité, dont l'habilitation a expiré ou dont le compte est désactivé n'est
    pas prévenu ;
  - **un e-mail par administrateur et par passe** : combien de nouvelles propositions
    attendent, pour quelles compétences (chacune nommée une fois), que chaque texte a été
    validé avant son envoi (ADR-100), qu'aucune n'est utilisée avant d'être activée, et le
    lien vers l'administration. Il ne contient jamais la procédure, ni la personne pour qui
    l'agent l'a proposée, ni qui l'a validée : l'e-mail quitte la plateforme, ils se lisent
    une fois connecté ;
  - **chaque proposition n'est annoncée qu'une fois** (`skill_versions.announced_at`),
    même si le relais refuse le message, qui n'est pas renvoyé. Une proposition décidée
    avant la passe n'est pas annoncée. Sans relais, rien n'est envoyé ni enregistré :
    les propositions seront annoncées une fois un relais configuré, puisqu'elles
    attendent encore. Sans administrateur, elles attendent qu'il y en ait un. Réserve
    assumée : les propositions sont enregistrées comme annoncées après les envois ; un
    arrêt du service entre les deux les fait annoncer de nouveau à la passe suivante (un
    e-mail en double plutôt qu'un avis perdu). L'échec d'un administrateur (son compte
    illisible, l'audit refusé) n'empêche pas de prévenir les autres, et ne lui est pas
    renvoyé ;
  - **choix de chacun** (ADR-094) : l'avis suit le choix « Les demandes qui attendent ma
    décision », renommé depuis « Les demandes de validation qui m'attendent ». Il couvre
    les validations et les propositions de compétences. Un administrateur qui ne le veut
    pas n'est pas prévenu, et l'avis est tracé refusé (`optedOut`). Il compte comme
    donné et n'est pas renvoyé ;
  - **audit** : `skill.proposals_announced`, une entrée par administrateur, pour lui, avec
    le nombre de propositions et les noms des compétences, jamais l'adresse ni la réponse
    du relais.
- **Alternatives écartées** :
  - un e-mail par proposition, envoyé au moment où elle arrive : plusieurs propositions
    d'une même minute feraient autant de messages, et un envoi perdu par un arrêt ne
    serait pas rattrapé ;
  - un quatrième choix d'e-mails : une proposition est une demande qui attend la décision
    de qui l'administre, comme une validation attend celle du responsable, et trois
    choix restent lisibles (ADR-094) ;
  - prévenir les détenteurs de `skill:manage` sur une unité : seul qui l'a sur toute
    l'entreprise peut activer ou rejeter une proposition.
- **Conséquences** :
  - reste à faire : les évaluations et la non-régression avant activation ;
  - tests : `crates/api/tests/skill_proposal_notices.rs` :
    - une proposition annoncée une fois à l'administrateur, sans la procédure ni son
      auteur, tracée ;
    - plusieurs en un seul e-mail, la proposition décidée avant la passe non annoncée ;
    - une autre entreprise prévenue de la sienne seule ;
    - un administrateur qui ne veut pas de ces e-mails n'en reçoit pas, même plus tard ;
    - rien sans relais ;
    - un message refusé par le relais tracé en échec, non renvoyé ;
    - rien dans un processus qui n'exécute pas les runs, ni pendant qu'un autre fait la
      passe ;
  - `crates/api/tests/skill_proposal_notice_recipients.rs` :
    - seul qui peut décider sur toute l'entreprise est prévenu : ni un administrateur
      d'une unité, ni un compte désactivé, ni une habilitation expirée ;
    - sans personne qui puisse décider (une politique de refus), les propositions
      attendent sans être enregistrées, puis sont annoncées une fois la politique levée ;
  - `crates/api/src/services/skill_proposal_notice/mail.rs` : la formulation pour une
    proposition et pour plusieurs ;
  - `crates/db/tests/definer_functions.rs` : la nouvelle fonction de lecture entre
    entreprises.

## ADR-103 — Cas d'évaluation des compétences, écrits par leurs administrateurs

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 4a) ·
  complète ADR-052 et ADR-100
- **Contexte** : une version d'une compétence, écrite par un administrateur ou proposée
  par l'agent (ADR-100), est activée sur la seule lecture de son texte. Rien ne montre
  ce qu'elle change dans les réponses de l'agent, ni ce qu'elle casse de ce que faisait
  la version active. Le schéma initial réservait une table `skill_evals` (`input`,
  `success_criteria` en JSON), jamais écrite.
- **Décision** :
  - **un cas d'évaluation** est une demande, telle qu'une personne la ferait à
    l'assistant, et de 1 à 8 critères de réussite, une ligne visible chacun, d'au plus
    300 caractères (« Cite chaque intervention en retard. »). Le cas a un nom, unique
    dans sa compétence. La demande, d'au plus 4 000 caractères, ne contient que du texte
    visible, des retours à la ligne et des tabulations, comme le contenu d'une
    compétence (ADR-052) : elle parviendra au modèle comme le tour d'une personne, et
    les critères au modèle qui juge la réponse. Les textes sont gardés en forme
    canonique (NFC : deux noms identiques à l'œil sont le même), les critères vides
    écartés (une zone de texte découpée à ses retours à la ligne en envoie au plus 50) ;
  - **au plus 10 cas par compétence** : chaque évaluation les rejouera tous, pour la
    version évaluée et pour la version active ;
  - **qui** : les administrateurs des compétences (`skill:manage` sur toute
    l'entreprise), comme le reste de l'écran (ADR-052), sous le verrou
    d'administration de l'entreprise. Un cas est écrit pour une compétence
    (`POST /admin/skills/{id}/cases`), puis réécrit en entier ou supprimé par son propre
    identifiant (`PUT`/`DELETE /admin/skill-cases/{id}`). Le détail d'une compétence
    liste ses cas dans l'ordre où ils ont été écrits ;
  - **données** : `skill_eval_cases` remplace `skill_evals`, supprimée. Les cas partent
    avec leur compétence ; leur auteur parti, ils restent à l'entreprise ;
  - **audit** : `skill.eval_case_created`, `skill.eval_case_updated` et
    `skill.eval_case_deleted`, sur la compétence, avec son nom, l'identifiant et le nom
    du cas (comme un refus sur la route du cas le nomme) et le nombre de critères,
    jamais la demande.
- **Alternatives écartées** :
  - garder `skill_evals` et son JSON libre : la plateforme ne pourrait vérifier ni la
    demande ni les critères avant de les donner à un modèle ;
  - des critères en un seul texte : le modèle qui juge les reçoit numérotés, et le
    verdict de chacun sera montré à part ;
  - des cas partagés entre compétences : chaque cas éprouve la procédure d'une
    compétence, que ses administrateurs connaissent.
- **Conséquences** :
  - reste à faire : l'exécution des cas, pour la version évaluée et la version active
    (non-régression), jugée sur les critères, dans un budget plafonné (lot 4b) ; l'écran
    « Compétences » (lot 4c) ;
  - tests :
    - `crates/core/src/skill_eval_cases.rs` : ce qu'un cas contient, en forme
      canonique, compté après normalisation ;
    - `crates/api/tests/skill_eval_cases.rs` : cas écrits, listés, réécrits et
      supprimés, tracés avec leur identifiant et sans leur demande ; ce qui est envoyé
      vérifié d'abord, les lignes vides d'une zone de texte écartées ; au plus 10 par
      compétence, partis avec elle, leurs noms propres à chaque compétence ; un membre refusé ; le cas d'une autre
      entreprise répond comme absent et reste tel quel ;
    - `crates/api/tests/skill_access.rs` : la route d'écriture d'un cas refusée à un
      membre, et sur la compétence d'une autre entreprise sans rien écrire ;
    - `crates/api/tests/first_login.rs` : les nouvelles routes retenues par un mot de
      passe provisoire.

## ADR-104 — Une version de compétence jouée sur ses cas, à côté de la version active

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 4b) ·
  complète ADR-098, ADR-100 et ADR-103
- **Contexte** : les cas d'évaluation d'une compétence sont écrits (ADR-103). Il reste à
  les jouer pour une version, à comparer avec la version active et à éclairer la
  décision d'activer, sans que rien ne soit modifié dans l'entreprise.
- **Décision** :
  - **demande** (`POST /admin/skills/{id}/evaluations`, la version) : par un
    administrateur des compétences (`skill:manage` sur toute l'entreprise) qui peut
    utiliser l'assistant, l'assistant lisant les compétences pour lui, l'entreprise ayant
    un modèle, pour une version de la compétence ayant au moins un cas. Les cas sont
    copiés tels qu'ils sont ; la version active est jouée à côté quand une autre est
    évaluée. Une évaluation en file ou en cours par entreprise ;
  - **exécution** : par un processus qui exécute les runs, à chaque minute et dès la
    demande, une à la fois entre les processus, sous le verrou de sa passe (ADR-086) tant
    qu'elle joue. Une évaluation trouvée en cours par la passe qui tient ce verrou est
    celle qu'un arrêt a laissée : elle échoue, « interrompue », à demander de nouveau.
    Aucun bail. Un résultat ne s'écrit que dans une évaluation encore en cours. Une
    passe dont la session du verrou s'est perdue (PostgreSQL redémarré) s'arrête, sans
    toucher à ce qu'une autre joue : elle vérifie avant chaque évaluation que cette
    session vit encore, et cesse au cas suivant quand une autre passe a close la sienne ;
  - **chaque cas** : l'assistant répond à la demande pour la personne qui a demandé
    l'évaluation, avec ses droits relus avant chaque étape, et ce qu'on donne à un
    sous-agent (ADR-098) : les outils qui ne font que lire et s'exécutent sans validation,
    l'autonomie « lecture seule », un budget propre (8 étapes, 0,50 USD, 3 minutes).
    Prévenu que les actions ne sont pas disponibles, il décrit celle qu'il proposerait.
    `skill_read` sert la version jouée, que l'index liste même quand sa compétence n'est
    pas encore active. Ni mémoires rappelées d'office (la mémoire reste cherchable par
    son outil, sans être marquée utilisée), ni sous-agent, ni validation, ni livrable.
    Avant chaque cas, la personne doit encore administrer les compétences de toute
    l'entreprise et pouvoir utiliser l'assistant : sinon l'évaluation s'arrête, échouée,
    avec les résultats acquis ;
  - **jugement** : un modèle juge la réponse sur les critères du cas, comme le contrôle
    qualité des réponses (`judge_answer`) : réussi, manqué (avec les manquements
    relevés) ou en erreur (pas de réponse à juger, pas de verdict, ou le juge en échec).
    Le résultat garde la réponse, les outils appelés et si la version a été chargée ; ce
    qu'a coûté la réponse compte même quand le juge échoue ;
  - **non-régression** : un cas réussi par la version active et pas par la version
    évaluée est une régression. Activer une version contre les régressions de sa dernière
    évaluation, comparée à la version active du moment, demande une confirmation
    (`confirmed`) ; `skill.activated` garde le bilan de cette évaluation et la
    confirmation ;
  - **ce n'est pas un run** : rien n'est écrit dans les tables des runs, aucun événement
    diffusé ; la consommation est comptée au nom de la personne (`usage_records` sans
    run), les entrées d'audit du moteur portent l'identifiant de l'évaluation, et le coût
    total s'additionne sur l'évaluation ;
  - **lecture** : la liste et le bilan de chaque évaluation pour tous les administrateurs
    des compétences ; les réponses, les manquements relevés et les outils appelés pour la
    seule personne pour qui elle a tourné : ils ont été lus avec ses droits ;
  - **conservation** : les cinq dernières évaluations d'une compétence, la plus ancienne
    supprimée à la demande suivante, et 90 jours au plus après leur fin (une purge
    chaque minute) : leurs réponses peuvent tenir ce que leur auteur seul peut lire.
    Elles partent avec leur compétence ; leur auteur parti, plus personne ne lit leurs
    réponses ;
  - **audit** : `skill.evaluation_requested` (version, version de référence, nombre de
    cas) et `skill.evaluated` (bilan, coût, raison d'un échec), jamais une réponse.
- **Alternatives écartées** :
  - jouer les cas comme des runs (file, bail, reprise) : un run porte une conversation
    ou une mission, sa reprise, sa leçon, son suivi en direct ; un cas n'en a pas
    besoin, et ses traces auraient rempli « Opérations » ;
  - exécuter les actions de la version évaluée : une évaluation ne modifie rien ;
  - montrer les réponses à tous les administrateurs : elles ont été lues avec les droits
    d'une personne ;
  - bloquer l'activation : la décision reste humaine (ADR-100), désormais éclairée et
    confirmée ;
  - plusieurs essais par cas : le coût double à chaque essai. Réserve assumée : un seul
    essai, des réponses de modèle qui varient d'un essai à l'autre.
- **Réserves assumées** :
  - une version proposée par l'agent (ADR-100) est remise comme procédure de
    l'entreprise pendant son évaluation, avant toute activation, avec les droits de
    l'administrateur, et les outils des serveurs MCP que l'entreprise a classés en
    lecture restent offerts : une proposition malveillante pourrait faire lire ce que
    l'administrateur peut lire et le passer en paramètre à un serveur MCP de
    l'entreprise. L'administrateur lit la proposition avant de l'évaluer ; l'écran le
    rappellera (lot 4c) ;
  - une seule passe pour la plateforme : une évaluation la tient jusqu'à une heure
    environ, celles des autres entreprises attendent ;
  - rien ne borne les évaluations demandées l'une après l'autre (10 USD environ chacune
    au plus), sinon une à la fois par entreprise.
- **Conséquences** :
  - coût au plus : 10 cas, 2 versions, 0,50 USD chacun, et les jugements ;
  - reste à faire : l'écran « Compétences » (lot 4c), qui saura confirmer une
    activation contre des régressions (il n'envoie pas encore `confirmed`) ;
  - tests :
    - `crates/core/src/skill_evaluations.rs` : régression, bilan, confirmation à
      l'activation ;
    - `crates/api/tests/skill_evaluations.rs` : chaque cas joué pour la version évaluée
      et la version active, ce que l'assistant reçoit (la demande, la lecture seule, les
      outils, la version jouée), le jugement, la régression, aucun run, la consommation
      au nom de la personne, l'audit sans réponse, l'activation confirmée ; les réponses
      lues par la seule personne ; une compétence pas encore active listée dans l'index ;
      une réponse sans verdict du juge ou que le juge n'a pu contrôler, en erreur,
      gardée avec ce qu'elle a coûté ;
    - `crates/api/tests/skill_evaluation_rules.rs` : ce qui est vérifié avant la file
      (cas, version, droits, modèle, lecture des compétences, une à la fois, autre
      entreprise) ; l'arrêt quand la personne ne peut plus (compte désactivé, rôle
      restreint) ; une évaluation laissée en cours par un arrêt ; une passe dont une
      autre a close l'évaluation n'écrit plus rien et s'arrête, de même qu'une passe dont
      la session du verrou s'est perdue ; une passe à la fois, seulement dans un
      processus qui exécute les runs ; les cinq dernières gardées, 90 jours au plus ;
    - `crates/api/tests/several_workers.rs` : le verrou d'une passe que son détenteur
      trouve perdu avec sa session ;
    - `crates/api/tests/skill_access.rs` : les routes refusées à un membre, le détail
      d'une évaluation compris, et la compétence d'une autre entreprise sans rien
      écrire ;
    - `crates/db/tests/definer_functions.rs` : les nouvelles fonctions entre
      entreprises (évaluations à jouer, purge).

## ADR-105 — Cas d'évaluation et évaluations dans l'écran « Compétences »

- **Date** : 2026-10-25 · **Statut** : acceptée (cycle de vie des compétences, lot 4c)
- **Contexte** : depuis les ADR-103 et ADR-104, l'API garde les cas d'évaluation d'une
  compétence, joue une version sur ses cas à côté de la version active et demande une
  confirmation pour activer une version qui fait régresser des cas. L'écran
  « Compétences » n'en montrait rien : une activation refusée pour régression n'y
  laissait qu'une erreur.
- **Décision** : une compétence ouverte montre, sous ses versions, deux sections :
  - **« Cas d'évaluation »** : chaque cas (nom, demande, critères), écrit, modifié ou
    supprimé sur place, dix au plus, ce que l'écran dit une fois la limite atteinte. Les
    critères s'écrivent un par ligne ; les lignes vides sont écartées par l'API ;
  - **« Évaluations »**, les plus récentes d'abord :
    - **lancement** : la version à évaluer se choisit dans une liste ; par défaut, la
      dernière qui n'est ni active ni rejetée. Tant qu'il n'y a aucun cas, la section
      le dit à la place du formulaire ;
    - **proposition de l'agent** : est concernée toute version que l'agent a écrite et
      qu'aucune personne n'a activée, en attente d'une décision ou rejetée. L'évaluer la
      fait suivre à l'assistant avec les droits de l'administrateur, en lecture seule.
      L'écran le rappelle sous la liste, puis demande une confirmation (« Avez-vous relu
      son contenu ? ») avant de la lancer ;
    - **suivi** : une évaluation en file ou en cours est relue 3 secondes après la fin
      de chaque lecture, jusqu'à sa fin (`useRefreshWhile`). Une lecture lente ne
      s'empile donc pas. La page « Missions » suit ses étapes de la même façon ;
    - **bilan** : état, cas joués, cas réussis par la version évaluée et par la
      « version de référence » (celle qui était active à la demande, ce qu'elle n'est
      plus forcément), régressions en rouge, raison d'un échec, qui l'a demandée, coût ;
    - **« Voir les cas »** : chaque cas avec son issue pour la version évaluée et pour
      la version de référence, et « Régression » quand la première a manqué ce que la
      seconde réussissait. Les réponses, les manquements et les outils appelés ne sont montrés
      qu'à la personne qui a demandé l'évaluation ; aux autres administrateurs, l'écran
      dit pourquoi ils ne les voient pas ;
  - **activation** : refusée pour régression (409), elle montre le message de l'API
    et demande « Activer quand même la version N ? ». Sur confirmation, elle est
    renvoyée avec `confirmed: true`, que l'audit garde (ADR-104) ;
  - **serveur e2e** : il joue à part la demande d'un cas (`skill_evaluations.rs`).
    L'agent charge la compétence, puis répond selon la version jouée ; le juge manque la
    réponse qui ne cite aucun retard.
- **Alternatives écartées** :
  - un bouton « Évaluer » sur chaque version : la liste des évaluations, leur suivi et
    le rappel pour les propositions se retrouvent ensemble dans une seule section ;
  - un flux d'événements (SSE) pour le suivi : une évaluation dure quelques minutes au
    plus, et une relecture toutes les 3 secondes, limitée à la compétence ouverte et à
    une évaluation en cours, suffit.
- **Conséquences** :
  - pendant chaque relecture, la section « Évaluations » est verrouillée comme toute
    liste rechargée (ADR-042) ;
  - tests :
    - `apps/web/e2e/skill-evaluations.spec.ts` : Paul écrit un cas puis le renomme ;
      il évalue la version 2 jusqu'à la fin de l'évaluation ; il lit cas par cas la
      régression, les réponses, le manquement et les outils appelés. Il refuse d'abord
      d'activer la version 2 malgré sa régression (la version 1 reste active), puis
      l'active après confirmation, et supprime le cas. Le test relit ensuite tout cela
      par l'API ;
    - `apps/web/e2e/skill-proposals.spec.ts` : rappelé de relire une proposition avant
      de l'évaluer, Paul décline. L'API, relue après rechargement, ne garde aucune
      évaluation ;
  - non couvert par un e2e : le message montré à un autre administrateur à la place des
    réponses. Le retrait des réponses par l'API est testé
    (`crates/api/tests/skill_evaluations.rs`).

## ADR-106 — Critères de qualité d'une compétence, jugés sur chaque réponse qui la suit

- **Date** : 2026-10-26 · **Statut** : acceptée (contrôle qualité avancé, lot 1)
- **Contexte** : le contrôle d'une réponse par un modèle (§6 d'AGENT_ENGINE) ne jouait
  que pour les missions (ADR-089) et, sur la plateforme qui l'active
  (`MIMORYA_LLM_JUDGE`), pour les runs qui ont des critères explicites. Une conversation
  n'en a pas : une procédure de l'entreprise suivie par l'agent n'était jamais
  vérifiée. Les cas d'évaluation (ADR-103) ne valent qu'avant l'activation.
- **Décision** :
  - **une version porte ses critères de qualité** (`skill_versions.quality_criteria`,
    migration 0046), écrits avec elle par un administrateur des compétences et
    immuables comme son contenu :
    - de 0 à 8, une ligne visible chacun, 300 caractères au plus, en NFC ;
    - lignes vides écartées, comme les critères d'un cas
      (`core/src/skill_quality.rs`) ;
    - envoyés un par ligne (`qualityCriteria` de `POST /admin/skills` et de
      `POST /admin/skills/{id}/versions`), montrés avec la version ;
    - l'audit compte les critères sans les citer ;
    - une version écrite sans critères (`qualityCriteria` absent) reprend ceux de la
      version active, aucun sans version active ; `[]` les retire. Une proposition de
      l'agent reprend aussi ceux de la version active : l'agent n'en écrit aucun et ne
      change pas la barre. Activer une proposition ne retire donc rien en silence ;
  - **dans un run**, `skill_read` les rend avec le contenu de la version active
    (`ToolResult::quality_criteria`), et l'agent les lit à la fin de la procédure
    (« Critères de qualité de la réponse ») pour les satisfaire dès sa première
    réponse :
    - seul ce champ de l'outil de la plateforme compte. Les noms d'outils sont uniques
      (ADR-052), aucun autre outil ne peut relever la barre ;
    - ils s'ajoutent à ceux du run, chacun une fois, 16 au plus, et sont gardés dans
      le point de reprise pendant une attente de validation ;
    - la réponse finale est jugée sur eux par un modèle (le juge de vérification du
      routeur), même sans `MIMORYA_LLM_JUDGE`. Si elle échoue, l'agent la corrige
      comme pour tout contrôle (« Contrôle qualité automatique », deux essais au plus),
      et les manquements restants sont montrés ;
  - **le résumé d'un sous-agent** n'est pas jugé sur les critères des compétences qu'il
    a chargées (`VerificationConfig::skill_criteria`, faux pour un enfant), et ces
    critères ne passent pas à son parent. Seule la réponse finale est jugée, sur ceux
    des compétences que le run principal a chargées lui-même ;
  - **une évaluation** (ADR-104) juge la réponse sur les critères du cas, puis sur ceux
    des deux versions comparées (la même barre pour les deux : un critère retiré ou
    ajouté ne fait pas à lui seul une régression), puis sur ceux des autres
    compétences que l'agent a chargées.
- **Alternatives écartées** :
  - des critères propres à la compétence, modifiables sans nouvelle version : une
    version activée doit rester ce que ses administrateurs ont relu, critères compris ;
  - un réglage de l'entreprise pour activer ce contrôle : écrire des critères est ce
    choix, compétence par compétence ;
  - les critères rendus par n'importe quel outil : un serveur MCP pourrait faire
    échouer toutes les réponses.
- **Conséquences** :
  - chaque réponse qui suit une compétence à critères coûte un appel au juge, deux ou
    trois s'il faut corriger. Ce coût compte dans le budget du run ;
  - restent à faire : les critères dans l'écran « Compétences », qui jusque-là écrit
    ses versions sans critères, donc avec ceux de la version active ; un modèle de
    contrôle distinct choisi par l'entreprise (lot 2) ;
  - tests :
    - `crates/core/src/skill_quality.rs` : critères normalisés, 8 au plus, une ligne
      visible chacun ;
    - `crates/agent/tests/runtime_skill_criteria.rs` : réponse jugée et corrigée sur les
      critères d'une compétence chargée, que l'agent lit avec la procédure ; aucun
      contrôle pour une compétence sans critère, ni pour un run qui ne les applique pas
      (celui d'une évaluation), ni pour le résumé d'un sous-agent qui l'a chargée et la
      réponse de son parent ; aucun critère ajouté par un autre outil que
      `skill_read` ; critères gardés pendant une attente de validation ;
    - `crates/agent/src/runtime/tool_calls/record.rs`, `answer_review.rs` : chaque
      critère une fois, 16 au plus, ceux du run d'abord ;
    - `crates/api/tests/skill_quality_criteria.rs` : critères écrits, vérifiés et
      montrés avec leur version, repris de la version active quand une version n'en
      donne pas, retirés par `[]` ; proposition de l'agent reprenant ceux de la version
      active ; réponse d'une conversation jugée et corrigée sur eux ; évaluation jugeant
      les deux versions sur les critères de l'une et de l'autre.

## ADR-107 — Critères de qualité écrits depuis l'écran « Compétences »

- **Date** : 2026-10-26 · **Statut** : acceptée (contrôle qualité avancé, lot 2a)
- **Contexte** : depuis l'ADR-106, une version porte ses critères de qualité, mais
  l'écran « Compétences » (ADR-052, ADR-101) écrivait ses versions sans eux : elles
  reprenaient en silence ceux de la version active, et aucun écran ne les montrait.
- **Décision** :
  - **le formulaire d'une compétence et celui d'une version** ont un champ « Critères
    de qualité de la réponse », un critère par ligne, 8 au plus, facultatif. Il est
    toujours envoyé (`qualityCriteria`) : une version écrite depuis l'écran porte ce
    que le formulaire montrait, et un champ vide n'en donne aucun (`[]`). La reprise
    des critères de la version active, quand le champ est absent, reste pour les
    clients de l'API et les propositions de l'agent (ADR-106) ;
  - **une nouvelle version part de la dernière qu'un administrateur a écrite**, son
    contenu (ADR-101) et ses critères ensemble, vide sans elle. « Enregistrer la
    version » s'active dès que l'un ou l'autre change, les critères comparés tels
    que le serveur les garde : changer les critères seuls fait une version,
    puisqu'une version activée reste ce qui a été relu ;
  - **chaque version montre ses critères**, tels que le serveur les a gardés (lignes
    vides écartées, chacun rogné). Ceux d'une proposition de l'agent sont dits
    « repris de la version alors active » : l'agent n'en écrit aucun.
- **Alternatives écartées** :
  - partir des critères de la version active plutôt que de ceux de la version dont
    part le contenu : le formulaire est une modification de cette version, contenu et
    critères ensemble ;
  - ne pas envoyer un champ vide, pour reprendre les critères de la version active :
    l'administrateur verrait un champ vide et la version en porterait.
- **Conséquences** :
  - reste à faire : un modèle de contrôle distinct choisi par l'entreprise (lot 2b) ;
  - tests : `apps/web/e2e/skills.spec.ts` (critères écrits avec la compétence, gardés
    rognés et sans ligne vide, repris par la version suivante ; une version dont seuls
    les critères changent, vidés alors que la version active en a, enregistrée sans
    aucun ; relus dans l'API après rechargement). Le libellé des critères d'une
    proposition de l'agent n'a pas de test de bout en bout : aucun scénario ne propose
    de version d'une compétence qui a des critères (le fait est testé par l'API,
    `crates/api/tests/skill_quality_criteria.rs`).

## ADR-108 — Modèle de contrôle des réponses choisi par l'entreprise

- **Date** : 2026-10-26 · **Statut** : acceptée (contrôle qualité avancé, lot 2b)
- **Contexte** : une réponse qui a des critères à tenir (ceux d'une compétence suivie,
  ADR-106, d'une mission, ADR-089, ou d'un cas d'évaluation, ADR-104) est jugée par un
  modèle. Ce juge était toujours le modèle qui avait écrit la réponse : il juge son
  propre travail, au prix du modèle le plus capable. Le routeur des modèles prévoyait
  une classe de tâche « vérification » qu'aucun réglage ne remplissait.
- **Décision** :
  - **l'entreprise peut choisir un modèle de contrôle**, à part de celui de ses
    conversations, dans « Modèle d'IA » (`settings:manage`, sous le verrou
    d'administration) : fournisseur, modèle et adresse, comme pour ses conversations
    (ADR-024), avec les mêmes règles (fournisseurs connus, adresses de la liste de
    l'exploitant, vérifiées de nouveau à chaque chargement) :
    - une ligne de `tenant_llm_settings` à côté de celle des conversations, distinguée
      par `purpose` (`conversations`, `verification`, migration 0047) ;
    - sa clé est un secret à part (`verification.api_key`), écrite seulement, jamais
      rendue ni tracée. Une clé ne passe ni d'une adresse à l'autre, ni d'un usage à
      l'autre : la clé des conversations ne sert pas au modèle de contrôle ;
    - `PUT`, `DELETE` et `POST …/test` sur `/admin/llm/verification`, montré dans
      `GET /admin/llm` (`verification`) ; audit `verification_model.configured`,
      `.reset` et `.tested`, comme pour les conversations ;
    - il est possible sans fournisseur propre pour les conversations : une entreprise
      sur le modèle de la plateforme peut faire juger par le sien ;
  - **ce modèle juge à la place de celui des conversations** :
    - les réponses des runs (conversations, missions, tâches planifiées), par la
      classe « vérification » du routeur ;
    - les réponses des évaluations, les deux versions comparées par le même juge ;
    - il ne répond ni ne corrige : l'agent corrige sa réponse avec le modèle des
      conversations. Son coût compte dans le run, à son propre prix ;
  - **s'il ne peut pas servir** (clé illisible, adresse retirée de la liste…), le modèle
    des conversations juge en attendant, et l'écran le dit avec la raison. Les
    conversations ne s'arrêtent pas : le modèle des conversations lit déjà tout ce
    qu'un contrôle lit, aucune donnée ne part ailleurs ;
  - **si un appel au modèle de contrôle échoue dans un run** (nom de modèle inconnu du
    fournisseur, clé révoquée chez lui, quota épuisé…), le modèle des conversations
    juge cette réponse à sa place (`ModelRouter::providers_for`), et l'échec est
    journalisé sans contenu. Une réponse n'est laissée sans jugement que si les deux
    échouent. Une faute de frappe dans le nom ne désactive donc pas le contrôle en
    silence ; « Tester le modèle de contrôle » la révèle ;
  - **une évaluation garde un seul juge** pour les deux versions de chaque cas : un
    appel en échec y laisse le cas non jugé (« Le contrôle de la réponse a échoué. »),
    compté comme régression s'il est réussi par la version de référence (ADR-104),
    plutôt que de comparer deux versions jugées par deux modèles.
- **Alternatives écartées** :
  - un autre modèle du même fournisseur que les conversations, sans clé à lui : une
    entreprise sur le modèle de la plateforme n'aurait rien pu choisir, et un juge
    d'un autre fournisseur n'aurait pas été possible ;
  - arrêter les conversations quand le modèle de contrôle ne peut pas servir, comme
    pour leur propre fournisseur : le choix de l'entreprise sur ses données reste
    tenu sans cela, et une clé révoquée ne doit pas couper l'assistant ;
  - laisser les réponses sans contrôle dans ce cas : les critères ne seraient plus
    tenus, sans que personne ne le voie dans la conversation ;
  - un modèle de contrôle de la plateforme : le choix appartient à l'entreprise, qui
    paie et dont les données partent.
- **Conséquences** :
  - le modèle de contrôle reçoit la demande, les critères, les sources consultées
    (leurs identifiants) et la réponse : l'écran demande de vérifier qu'il convient
    aux données de l'entreprise ;
  - tests : `crates/api/tests/verification_model.rs` (choisi, montré, testé, retiré ;
    il juge les réponses d'un run et le modèle des conversations les corrige, puis le
    modèle des conversations juge de nouveau ; il juge les deux versions d'une
    évaluation ; inutilisable, le modèle des conversations juge et les conversations
    continuent ; un appel qui échoue laisse le modèle des conversations juger dans un
    run, et laisse le cas non jugé dans une évaluation ; aucune clé ne passe de l'un à l'autre, chacun retiré
    seul ; réservé aux administrateurs des réglages de toute l'entreprise, un
    administrateur partiel refusé), `crates/agent/src/llm/router.rs` (ordre des juges),
    `crates/agent/tests/runtime_verification.rs` (le modèle de l'agent juge quand le
    modèle de contrôle échoue, la réponse reste non jugée si les deux échouent),
    `crates/api/tests/admin_llm.rs`, `crates/api/tests/company_model_runs.rs` (chaque
    entreprise n'est jugée que par le sien), `apps/web/e2e/review-model.spec.ts`.

## ADR-109 — Déclencheurs : une demande à l'agent quand un autre système envoie des données (premier lot : l'API)

- **Date** : 2026-10-26 · **Statut** : acceptée (connecteurs, lot 1)
- **Contexte** : l'agent travaillait quand on lui écrivait, à heure fixe (ADR-080) ou
  pour une mission (ADR-089), jamais sur un événement d'un autre système (une panne
  signalée par la supervision, un formulaire reçu, un ticket créé). C'est le premier lot
  du chantier « connecteurs » de la phase 2, le plus autonome : il réutilise le lancement
  des tâches planifiées, sans infrastructure nouvelle.
- **Décision** :
  - **chacun écrit ses déclencheurs** (`GET`, `POST /inbound-hooks`, `PATCH`, `DELETE
/inbound-hooks/{id}`) : un intitulé (une ligne, 120 caractères au plus) et la demande
    envoyée comme la sienne (4 000 au plus), avec les règles de texte des tâches
    planifiées (`checked_request`, caractères cachés retirés). Dix au plus par personne.
    Un déclencheur se met en pause et reprend ; ses changements sont tracés
    (`inbound_hook.created`, `updated`, `token_replaced`, `deleted`), jamais le texte de
    la demande ni le jeton ;
  - ce sont ses ressources : celles d'un autre répondent comme absentes (404). Créer,
    reprendre un déclencheur ou en renouveler le jeton demande le droit d'utiliser
    l'agent (`agent:use`) ; le mettre en pause ou le supprimer, non ;
  - **un jeton secret par déclencheur** (`mhk_` suivi de 256 bits aléatoires), rendu une
    seule fois, à la création ou au renouvellement (`POST /inbound-hooks/{id}/token`,
    l'ancien cesse aussitôt de servir). Seule son empreinte SHA-256 est gardée
    (`inbound_hooks.token_hash`), jamais dans les journaux ni l'audit ;
  - **l'autre système envoie** `POST /api/hooks/{id}` avec `Authorization: Bearer
<jeton>`, avant toute session :
    - l'entreprise se trouve par la fonction minimale `auth_inbound_hook_tenant`
      (déclencheur, empreinte du jeton → entreprise), puis la ligne est verrouillée dans
      l'entreprise ;
    - un déclencheur inconnu, un jeton faux ou absent et un déclencheur en pause
      répondent de même (404, « Déclencheur introuvable ») : rien n'est gardé ni tracé
      pour lui ;
    - le corps est du texte UTF-8 de 16 Kio au plus (413, 415 sinon) ;
    - trente envois par heure au plus et par déclencheur (fenêtre fixe d'une heure) ;
      au-delà, 429 avec `Retry-After` jusqu'à la fin de l'heure. Le premier refus de
      l'heure est gardé et tracé, les suivants n'écrivent rien : un expéditeur qui insiste
      ne remplit pas l'audit ;
    - accepté, il répond 202, sans rien nommer ;
  - c'est la **seconde exception** à l'en-tête anti-CSRF et au contrôle d'origine (après
    la réception SAML, ADR-068). Aucune session ni cookie n'autorise cette requête : le
    jeton seul le fait, qu'un site tiers ne peut pas joindre. Le chemin est comparé en
    entier, l'identifiant sous sa forme canonique (minuscules, avec tirets) : toute autre
    écriture (`/hooks/{id}/`, majuscules, sans tirets, `/hooks/{id}/token`…) exige
    l'en-tête comme le reste. Des tests tentent de la contourner ;
  - **la demande part comme celle de son auteur**, dans une conversation nouvelle à lui
    (intitulée comme le déclencheur), avec ses droits tels qu'ils sont alors, comme une
    tâche planifiée : rien n'est ouvert si son compte est désactivé, s'il doit remplacer
    un mot de passe provisoire, s'il a perdu `agent:use`, si son entreprise est
    suspendue ou sans modèle. L'expéditeur reçoit 409 ou 503, d'un message qui n'en dit
    pas plus : la raison, qui parle du compte de l'auteur ou de son entreprise, est
    gardée pour l'auteur seul. La conversation s'ouvre dans la transaction du tour
    (`RunService::start_for`, partagé avec les tâches planifiées) : un run qui ne démarre
    pas n'en laisse aucune ;
  - le tour envoyé fait au plus 4 000 caractères de demande et 16 Kio de données.
    L'auteur peut répondre ensuite dans la conversation : ses tours sont les siens (canal
    `web`, autonomie de l'entreprise), les données reçues y restant comme un résultat
    d'outil, encadrées ;
  - **les données reçues sont des données, jamais des consignes** : elles suivent la
    demande, encadrées comme un résultat d'outil non fiable (`<donnees_outil
source="declencheur" fiabilite="non_fiable">`, la règle que le prompt système donne
    déjà), sans pouvoir ouvrir ni fermer une enveloppe ;
  - **un run lancé par un déclencheur n'agit jamais seul** : son autonomie est plafonnée
    à « exécution avec validation », même si l'entreprise laisse ses runs agir seuls.
    Ce qu'il écrirait sur la foi de données venues d'ailleurs attend la validation de
    l'auteur ;
  - **ni ne retient rien seul** : le run porte son canal (`agent_runs.channel = 'hook'`,
    `web` sinon), relu quand il reprend après une validation ou un arrêt du service, et
    transmis à ses sous-agents. Sur ce canal, `memory_remember` n'écrit pas directement,
    même dans la mémoire personnelle (`can_write_memory` le refuse) : c'est une
    proposition que l'auteur garde ou non, comme une leçon (ADR-076). Sans cela, un
    brouillon, qui n'attend aucune validation, laisserait les données reçues s'installer
    dans sa mémoire et guider ses runs suivants. Les politiques d'accès lisent ce canal
    (`context.channel`) : une entreprise peut retirer à ces runs ce qu'elle veut ;
  - ce qu'est devenu le dernier envoi est gardé pour l'auteur (lancé, limité, refusé ou
    échoué, et pourquoi en français), avec la conversation du dernier run lancé. Il est
    tracé (`inbound_hook.received` avec le run, `inbound_hook.not_run` avec l'issue ;
    acteur système, au nom de l'auteur). Le démarrage du run l'est comme celui d'une
    tâche planifiée (`agent.run.started`, avec `inboundHookId`).
- **Alternatives écartées** :
  - une signature HMAC du corps : plus exigeante pour les systèmes qui envoient (beaucoup
    ne savent qu'ajouter un en-tête fixe) ; sous TLS, le jeton suffit à authentifier
    l'envoi ;
  - le jeton dans l'adresse : il finirait dans les journaux des intermédiaires ;
  - des déclencheurs de l'entreprise, écrits par l'administration : un run agit au nom
    d'une personne, avec ses droits ; c'est elle qui choisit ce qu'elle demande ;
  - un seul run à la fois par déclencheur, comme une tâche planifiée : des événements
    distincts se perdraient ; la limite horaire borne le coût ;
  - garder les données reçues à part : elles sont dans la conversation, seul endroit où
    l'auteur les lit ;
  - déduire le canal de la demande du message qui a ouvert le run : il faudrait le relire
    à chaque reprise ; la colonne le garde avec le run ;
  - dire à l'expéditeur pourquoi rien n'est lancé : quiconque tient le jeton apprendrait
    l'état du compte de l'auteur.
- **Conséquences** :
  - donnée personnelle nouvelle : le déclencheur et le texte de sa demande, gardés tant
    que son auteur le garde, supprimés avec lui ou avec son compte, lisibles en entier
    par lui (`GET /inbound-hooks`), à joindre à l'export de ses données ; les données
    reçues sont des messages de ses conversations, avec leur rétention ;
  - restent à faire : l'écran des déclencheurs (lot 2) ; l'avis par e-mail à l'auteur
    quand le run d'un déclencheur attend sa validation (comme ADR-082) ; les autres
    connecteurs du chantier (e-mail, agendas, REST générique) ;
  - tests : `crates/api/tests/inbound_hooks.rs` (écrits, changés, mis en pause, jeton
    montré une fois et gardé en empreinte, renouvelé, supprimés ; ceux d'un autre absents ;
    dix au plus ; `agent:use` pour créer, reprendre, renouveler, pas pour arrêter ;
    l'audit sans la demande ni le jeton), `crates/api/tests/inbound_hook_deliveries.rs`
    (envoi lançant un run de l'auteur avec les données encadrées, autonomie plafonnée,
    rien de reçu dans l'audit ; mémoire personnelle proposée, canal `hook` ; inconnu,
    jeton faux ou absent, en pause ou jeton d'un autre déclencheur répondant de même ;
    corps borné et en UTF-8 ; seul le chemin exact d'un envoi se passe de l'en-tête,
    pas même encodé), `crates/api/tests/inbound_hook_refusals.rs` (trente par heure puis
    429 et `Retry-After`, le refus gardé et tracé une fois ; auteur désactivé, à mot de
    passe provisoire, sans `agent:use`, entreprise suspendue : 409 sans la raison,
    gardée pour l'auteur, rien d'ouvert ; sans modèle ou run impossible à enregistrer :
    503, aucune conversation), `crates/core/tests/memory.rs` (rien d'écrit directement
    sur le canal `hook`), `crates/api/src/plugins/csrf.rs` et `services/inbound_hooks/`
    (tests unitaires), `crates/api/tests/first_login.rs` (liste des routes).

## ADR-110 — Écran des déclencheurs (connecteurs, deuxième lot)

- **Date** : 2026-10-26 · **Statut** : acceptée
- **Contexte** : les déclencheurs (ADR-109) ne s'écrivaient que par l'API. Il fallait un
  écran pour les créer, recevoir leur jeton, lire ce qu'est devenu le dernier envoi et
  les arrêter. Il fallait aussi qu'une conversation ouverte par un envoi distingue la
  demande de son auteur des données reçues.
- **Décision** :
  - une page « Déclencheurs » dans la navigation, pour chacun, après « Tâches
    planifiées ». Une carte « Nouveau déclencheur » : intitulé et demande à l'agent,
    avec le rappel que les données reçues ne sont jamais des consignes, et les limites
    lues de l'API (trente envois par heure, 16 Kio). Sans le droit d'utiliser l'agent,
    ou à dix déclencheurs, la carte le dit à la place du formulaire ;
  - **le jeton est montré une fois**, à la création ou au renouvellement, dans un encadré
    qui prend le focus. Il contient :
    - l'adresse complète à appeler ;
    - le jeton, avec un bouton qui le copie ;
    - un exemple d'envoi (`curl`) ;
    - l'avertissement qu'il ne sera plus affiché.

    Il n'est gardé que dans l'état de la page : un rechargement le perd, et la liste
    rendue par l'API ne le contient jamais ;

  - **l'adresse est donnée par l'API** (`url`, en plus de `path`), à partir de
    l'origine publique de l'application (`WEB_ORIGIN`, sous `/api`), comme les liens
    des e-mails : celle du navigateur peut différer derrière un intermédiaire ;
  - chaque déclencheur montre son état (« Actif » ou « En pause »), son adresse, sa
    demande et ce qu'est devenu le dernier envoi (un badge, la raison gardée pour
    l'auteur seul). Il mène aussi à la conversation du dernier envoi lancé, qui attend
    peut-être une validation. « Modifier », « Mettre en pause » ou « Reprendre »,
    « Nouveau jeton » et « Supprimer » sont nommés par son intitulé. Renouveler le jeton
    et supprimer demandent une confirmation : l'un comme l'autre coupe le système qui
    s'en sert ;
  - **le message envoyé par un déclencheur se reconnaît** : ses métadonnées portent le
    déclencheur (`inboundHookId`). La conversation l'affiche avec un badge « Envoyé par
    un déclencheur », la demande dans la bulle et les données reçues à part, repliées
    (« Données reçues »), telles que le modèle les a lues. Le texte du message reste
    celui que le modèle a reçu : l'historique n'est jamais réécrit ;
  - le serveur des tests de bout en bout répond à part à ce qu'envoie un déclencheur
    (`e2e_server/hooks.rs`), reconnu par l'enveloppe des données reçues : les scénarios
    de référence gardent leur ordre.
- **Alternatives écartées** :
  - garder les données reçues à part dans les métadonnées du message : elles y seraient
    en double. Le texte se découpe à la dernière ouverture d'enveloppe, la seule que la
    plateforme écrit (celles des données sont neutralisées) ;
  - composer l'adresse dans le navigateur : elle serait fausse derrière un relais qui
    change l'hôte ;
  - réafficher le jeton plus tard : seule son empreinte est gardée (ADR-109).
- **Conséquences** :
  - reste l'avis par e-mail à l'auteur quand le run d'un déclencheur attend sa
    validation, puis les autres connecteurs (e-mail, agendas, REST générique) ;
  - tests :
    - `apps/web/e2e/inbound-hooks.spec.ts` : un déclencheur créé, son jeton montré une
      fois et absent de la liste ; un autre système qui envoie sans session, le dernier
      envoi « Lancé », la conversation ouverte avec la demande, les données reçues et la
      réponse ; puis la mise en pause (404 à l'envoi), la reprise, un nouveau jeton
      (l'ancien refusé), une modification (le focus rendu à « Modifier ») et la
      suppression, chaque changement relu par l'API ;
    - `apps/web/e2e/navigation.spec.ts` : la page se charge sans erreur ;
    - `crates/api/tests/inbound_hooks.rs` et `inbound_hook_deliveries.rs` : l'adresse
      complète et le déclencheur dans les métadonnées du message ;
      `contract/tests/records.rs` : ces métadonnées relues.

## ADR-111 — L'auteur d'un déclencheur prévenu quand un run qu'il a lancé attend sa validation (connecteurs, troisième lot)

- **Date** : 2026-10-26 · **Statut** : acceptée
- **Contexte** : un run lancé par l'envoi à un déclencheur (ADR-109) part sans que
  personne le suive, et son autonomie est plafonnée : ce qu'il écrirait attend la
  validation de son auteur. Sans avis, la demande attendait sans qu'on la voie jusqu'à son
  échéance. Les tâches planifiées (ADR-082) et les missions (ADR-091) prévenaient déjà
  leur auteur.
- **Décision** :
  - **le même avis** : quand un tel run se met en pause sur des demandes que son auteur
    décide lui-même, un e-mail le lui dit, une fois par pause. Il nomme le déclencheur
    (« Votre déclencheur « … » attend votre validation »), le nombre d'actions et leurs
    risques, l'échéance, et mène à la conversation du run. Il ne contient ni les actions,
    ni leurs paramètres, ni rien des données reçues : le message quitte la plateforme ;
  - **un run de déclencheur se reconnaît à son canal** (`agent_runs.channel = 'hook'`,
    ADR-109), pas au dernier run du déclencheur : plusieurs envois peuvent avoir chacun un
    run en attente. Le titre est celui de sa conversation, intitulée comme le déclencheur
    à l'envoi. Une conversation que la plateforme ouvre avec son propre titre (celui
    d'une tâche planifiée ou d'un déclencheur) n'est plus renommée d'après son premier
    message, comme celle d'une personne : un déclencheur intitulé « Nouvelle
    conversation » aurait vu sa conversation, puis l'e-mail, porter le début des données
    reçues. Une réponse que l'auteur écrit ensuite dans cette conversation part sur le
    canal `web` : il la suit, personne n'est prévenu ;
  - les demandes routées vers un responsable lui sont annoncées à lui (ADR-060), comme
    pour tout run ;
  - un avis qu'un arrêt du service a perdu est envoyé par le rattrapage périodique, dix
    minutes après la pause (ADR-070, ADR-086). Il s'appuie sur la fonction minimale
    `system_unnotified_hook_runs` (entreprise et run seulement), comme
    `system_unnotified_mission_runs` ;
  - l'avis suit le choix de chacun pour « Mes tâches planifiées, missions et
    déclencheurs qui attendent ma validation » (ADR-094), dont le libellé s'élargit. Un
    avis non voulu est tracé comme refusé, et non retenté ;
  - chaque demande annoncée est tracée (`approval.notified`), sans l'adresse.
- **Alternatives écartées** :
  - retrouver le run par `inbound_hooks.last_run_id` : un envoi plus récent le
    remplacerait, et le run précédent, toujours en attente, ne serait plus annoncé ;
  - un réglage d'e-mail à part pour les déclencheurs : les trois cas sont le même
    besoin, un run que personne ne suit attend sa décision.
- **Conséquences** :
  - restent les autres connecteurs : e-mail, agendas, REST générique ;
  - tests :
    - `crates/api/tests/inbound_hook_notices.rs` : l'auteur prévenu une fois, l'e-mail
      sans l'action ni les données reçues, tracé, pas renvoyé au redémarrage ni au
      rattrapage ; un avis perdu envoyé dix minutes après la pause, pas avant ni deux
      fois ; un run que l'auteur suit dans une autre conversation n'est jamais listé ;
      un déclencheur intitulé « Nouvelle conversation » garde ce titre, l'e-mail ne
      contient rien de ce qui a été posté, et la réponse de l'auteur dans cette
      conversation, en pause à son tour, n'est annoncée à personne ;
    - `approval_notice/author_mail.rs` (test unitaire) : le message d'un déclencheur ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction listée ;
    - `apps/web/e2e/notification-settings.spec.ts` : le libellé élargi.

## ADR-112 — API REST décrites par l'entreprise (connecteurs, quatrième lot)

- **Date** : 2026-10-26 · **Statut** : acceptée
- **Contexte** : beaucoup d'outils métier d'une entreprise n'ont ni serveur MCP ni
  connecteur dédié, mais exposent une API REST. Jusqu'ici, l'agent ne pouvait les
  atteindre qu'à travers un serveur MCP écrit pour l'occasion. Or une API choisie par
  une entreprise est appelée par le serveur de la plateforme, avec un jeton : les mêmes
  précautions que pour un serveur MCP (ADR-025, ADR-026) s'appliquent, et ce que l'agent
  envoie doit rester dans ce que l'administrateur a décrit.
- **Décision** :
  - **un type de connecteur « API REST »** (`rest`) : depuis l'administration
    (`connector:manage`, sur toute l'entreprise, sous le verrou d'administration), une
    entreprise ajoute une API par son adresse (`https://…`), l'en-tête où va son jeton
    (`Authorization: Bearer` par défaut, ou un en-tête nommé comme `X-API-Key`), et ses
    opérations (`POST /admin/connectors/rest`). Elle la modifie, la teste, l'active, la
    désactive et la supprime comme un serveur MCP (`PATCH`, `test`, `DELETE`) ;
  - **une opération, un outil** : chaque opération (nom, intitulé, description, méthode
    `GET`/`POST`/`PUT`/`PATCH`/`DELETE`, chemin, paramètres typés `string`, `integer`,
    `number` ou `boolean`, placés dans le chemin, la requête ou le corps JSON) devient
    l'outil `rest_<préfixe>_<opération>`. Le modèle lit « [API REST « préfixe »] » puis
    la description écrite ; sa description se change dans la déclaration, pas par une
    politique (contrairement à un outil MCP, ADR-057). Comme un outil MCP, il exige
    `connector:use`, a le risque `write` (validation humaine) tant qu'un administrateur
    ne l'a pas associé à des permissions et à un risque, et ses résultats sont des
    données non fiables ;
  - **ce qu'une déclaration peut contenir** (`connectors::rest::checks`, vérifié à
    l'enregistrement et à chaque démarrage) : préfixe et adresse comme un serveur MCP ;
    50 opérations, 30 paramètres par opération, intitulés d'une ligne, descriptions
    visibles (ADR-057), 256 Kio en tout ; un en-tête de jeton qui n'appartient pas au
    protocole ni au client (ni `Authorization`, `Host`, `Cookie`, `Accept`,
    `Accept-Encoding`, `User-Agent`, `Content-*`, `Proxy-*`, `Sec-*`…) ;
    un chemin qui commence par `/`, sans `%`, `?`, `#` ni segment `.` ou `..`, dont
    chaque `{paramètre}` est un paramètre de chemin déclaré, toujours requis ; pas de
    paramètre de corps pour `GET` ni `DELETE` ;
  - **ce qu'un appel envoie** (`connectors::rest::request`) : l'entrée du modèle n'a que
    les paramètres déclarés, chacun de son type (texte de 4 000 caractères au plus).
    Un paramètre de chemin est encodé entier (`/`, `%`, `?`, `#` compris) et ne peut être
    vide. Un serveur peut décoder `%2F` (ou `%5C`) en séparateur avant de résoudre le
    chemin (nginx, Tomcat, IIS), voire décoder deux fois : une valeur dont un segment,
    entre deux `/` ou `\`, vaut `.` ou `..` une fois décodée est refusée (`../admin`,
    `..\x`, `%2e%2e`), et un chemin obtenu avec un tel segment, tel qu'envoyé, décodé
    une fois, ou décodé deux fois en entier (deux valeurs voisines, une valeur collée
    au chemin écrit, un échappement à cheval sur deux valeurs comme `.%2` puis `e`),
    est refusé : la requête ne sort jamais de l'adresse de l'API ni du chemin écrit par
    l'administrateur, ce qu'un dernier contrôle de l'adresse obtenue vérifie encore ;
  - **exécutée telle que validée** : la personne qui valide lit la requête entière
    (méthode, adresse, corps). Le moteur garde ce qu'elle a lu de l'action : une fois
    validée, une action que son outil ne décrit plus de la même façon (un
    administrateur a changé l'adresse, la méthode ou le chemin de l'opération pendant
    l'attente) n'est pas exécutée, et le modèle l'apprend (« Action modifiée depuis la
    demande de validation »). Ce qui a été lu est comparé sur l'entrée aux clés triées :
    un point d'arrêt relu de `jsonb` revient les clés réordonnées, sans que l'action ait
    changé. Cela vaut pour tout outil ; un run mis en pause avant
    cette règle n'a pas gardé ce qui a été lu, et s'exécute comme avant. La politique
    d'une opération retirée, ou dont la méthode ou le chemin change, part avec elle
    (tracé : `droppedPolicies`) : une lecture associée sans validation ne devient pas
    une suppression sans validation ;
  - **le réseau** : le client HTTP des connecteurs (ADR-025) — adresses publiques
    seulement, résolues à la connexion, aucune redirection suivie (une réponse `3xx`
    est un échec dit tel quel), pas de mandataire de l'environnement — sauf déploiement
    privé (`MIMORYA_MCP_ALLOW_PRIVATE_NETWORK`). Le code partagé par les serveurs MCP et
    les API REST (politique réseau, préfixe, jeton, masquage, explication des échecs,
    `connector:use`) passe de `connectors::mcp` à `connectors::outbound` ;
  - **ce que le modèle lit** : `{ "statut", "corps" }`. Le corps est lu jusqu'à
    256 Kio : en JSON quand il en annonce et le contient entier, en texte s'il est du
    texte, sinon nommé sans être lu (« [contenu image/png de … octets, non lu] ») ; les
    en-têtes ne sont pas transmis. Un statut hors `2xx` est un appel en échec dont le
    modèle lit quand même le corps. L'appel est borné à 20 s, sous le délai d'un outil ;
  - **le jeton** : scellé (`connector.<id>.token`, ADR-024), au format de la RFC 6750
    (ADR-026), marqué sensible dans l'en-tête, jamais renvoyé ni audité. Il ne suit pas
    l'API vers une autre adresse. Il est masqué dans tout ce que l'API renvoie et dans
    les échecs, avant la troncature : le corps est lu un peu au-delà de la borne (la
    plus longue graphie du jeton), masqué, puis coupé en retirant d'abord cette
    longueur, afin qu'aucun morceau d'un jeton coupé par la lecture ne reste ; un jeton
    de chiffres renvoyé comme un nombre JSON est masqué lui aussi ;
  - un changement d'adresse, d'en-tête ou de jeton remet l'API « Non testé » ; un
    changement d'opérations est vérifié comme une déclaration entière. Démarrer l'API
    ne l'appelle pas : le test de l'administration vérifie la déclaration et liste les
    outils avec leur politique effective, le premier appel dit si l'API répond ;
  - les préfixes de deux API ne se recouvrent jamais (un serveur MCP peut porter le
    même, ses outils s'appelant `mcp_…`) ; l'audit garde le type, l'adresse, l'en-tête,
    le nom des opérations et si un jeton est enregistré.
- **Alternatives écartées** :
  - importer une description OpenAPI : utile, mais elle décrit souvent des centaines
    d'opérations et des schémas imbriqués que le modèle n'a pas à voir ; la déclaration
    opération par opération garde l'administrateur maître de ce qui est exposé.
    L'import pourra la préremplir ;
  - suivre les redirections vers la même origine : une redirection mène où l'API
    décide, et l'adresse obtenue devrait être contrôlée de nouveau ; l'échec est dit ;
  - des objets et des listes en paramètres : le corps plat couvre l'essentiel des
    écritures simples sans laisser le modèle composer des structures que personne n'a
    décrites.
- **Conséquences** :
  - l'écran de l'administration pour décrire une API vient au lot suivant ; d'ici là,
    une API ajoutée par l'API d'administration s'affiche comme un connecteur sans
    détail, et son test, qui ne l'appelle pas, se lit comme une connexion réussie :
    l'écran dira qu'il vérifie la déclaration ;
  - restent : l'écran des API REST, l'e-mail et les agendas ;
  - tests :
    - `crates/connectors/tests/rest_connector.rs` : ce qu'une opération envoie
      (chemin encodé entier, requête, corps, jeton en `Bearer` ou dans son en-tête),
      l'entrée refusée hors déclaration, la requête montrée à la validation, les
      statuts, redirections et corps non lus, le jeton masqué même coupé par la borne,
      le délai, l'API injoignable, l'adresse privée refusée ;
    - `crates/connectors/tests/rest_declarations.rs` : ce qu'une déclaration peut
      contenir, et une requête qui reste sous le chemin écrit, même lue par un serveur
      qui décode `%2F` ;
    - `crates/agent/tests/runtime_approvals.rs` : une action que son outil ne décrit
      plus comme elle a été validée n'est pas exécutée ;
    - `crates/api/tests/connector_runs.rs` : un appel MCP validé s'exécute, quel que
      soit l'ordre dans lequel les clés de son entrée reviennent du point d'arrêt ;
    - `crates/api/tests/rest_connectors.rs` : l'API ajoutée, listée sans sa clé,
      testée, associée, auditée sans clé, supprimée avec son secret ; un run qui lit
      directement, puis écrit une fois la requête validée, la clé masquée dans ce que le
      modèle lit ; une écriture validée qui n'est pas envoyée, son opération changée
      pendant l'attente ;
    - `crates/api/tests/rest_connector_rules.rs` : les droits, la déclaration vérifiée,
      les préfixes, la clé perdue au changement d'adresse, la description réservée à la
      déclaration, la politique d'une opération retirée ou changée de méthode partie
      avec elle, un serveur MCP sans opérations, les adresses d'une plateforme
      publique, une déclaration devenue illisible supprimée plutôt que modifiée.

## ADR-113 — Écran des API REST (connecteurs, cinquième lot ; complète ADR-112)

- **Date** : 2026-10-26 · **Statut** : acceptée
- **Contexte** : l'ADR-112 permet de décrire une API REST par l'API d'administration
  seulement : l'écran « Connecteurs » l'affichait comme un connecteur sans détail, et son
  test, qui n'appelle pas l'API, se lisait comme une connexion réussie.
- **Décision** :
  - dans « Administration › Connecteurs », « Ajouter une API REST » ouvre un formulaire :
    nom, préfixe (définitif), adresse, en-tête du jeton (vide : `Authorization: Bearer`),
    jeton (jamais réaffiché ; laissé vide à la modification, il est conservé), puis les
    opérations. Chaque opération (nom, intitulé, méthode, chemin, description lue par le
    modèle) a ses paramètres (nom, emplacement : chemin, requête ou corps JSON, type,
    requis, description) ; un paramètre de chemin est toujours requis. Le formulaire
    rappelle le nom de l'outil (`rest_<préfixe>_<opération>`). La plateforme vérifie la
    déclaration entière et dit ce qu'elle refuse, que le formulaire montre tel quel ;
  - la carte d'une API montre son adresse, le préfixe de ses outils, le nombre
    d'opérations et l'en-tête où va le jeton. Son bouton de test s'intitule « Vérifier
    la déclaration » ; réussi, il dit « Déclaration vérifiée : n outil(s). L'API n'est
    pas appelée ; le premier appel d'un run dira si elle répond », et l'état de la carte
    se lit « Déclaration vérifiée » plutôt qu'« Opérationnel » (« Non vérifiée » avant).
    Elle se modifie et se supprime comme un serveur MCP ajouté ici ; une API dont la
    déclaration est devenue illisible se supprime encore ;
  - les règles des outils d'une API se règlent comme celles d'un serveur MCP (risque,
    permissions, activation), sans description à remplacer : celle d'une opération se
    change dans sa déclaration. Les règles montrées par une vérification sont celles de
    la déclaration d'alors : ouvrir « Modifier » les retire de l'écran, et une
    modification enregistrée ne les laisse pas revenir, faute de quoi une association
    que la modification vient de retirer serait réécrite telle quelle. Le serveur refuse
    aussi une politique qui ne nomme aucune opération déclarée (« Opération inconnue de
    cette API »), celles de la requête si elle en remplace : aucune association n'attend
    qu'une opération de ce nom revienne ;
  - la politique d'une opération part aussi quand ses paramètres changent, et non
    seulement sa méthode ou son chemin (ADR-112) : une lecture filtrée associée sans
    validation ne devient pas, par un paramètre ajouté, une autre action sans
    validation.
- **Conséquences** :
  - restent : l'import d'une description OpenAPI pour préremplir les opérations, l'e-mail
    et les agendas ;
  - tests : `apps/web/e2e/rest-connectors.spec.ts` (une API décrite avec sa clé et deux
    opérations, un chemin qui remonterait refusé et dit, la déclaration vérifiée, une
    lecture associée, l'API modifiée puis supprimée, chaque étape relue dans l'API, la clé
    jamais listée ; puis la lecture devenue une suppression : les règles d'avant ne
    restent pas à l'écran et son association est retirée) ;
    `crates/api/tests/rest_connector_rules.rs` (la politique d'une opération dont les
    paramètres changent retirée ; une politique pour une opération non déclarée
    refusée, acceptée quand la même requête la déclare) ;
  - une politique écrite depuis un autre onglet resté ouvert sur une déclaration
    ancienne n'est pas détectée : comme les autres formulaires de l'administration,
    celui des règles ne porte pas la version qu'il modifie.

## ADR-114 — Opérations d'une API REST lues d'une description OpenAPI (connecteurs, sixième lot ; complète ADR-112)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : décrire une API REST opération par opération (ADR-112, ADR-113) est
  long quand l'éditeur en publie déjà la description OpenAPI, et recopier ses textes à
  la main expose aux erreurs. Mais une description vient d'un tiers : ses textes
  seraient lus par le modèle, ses références peuvent pointer ailleurs ou s'imbriquer
  sans fin, et elle peut décrire des centaines d'opérations, dont des formes que la
  déclaration ne sait pas porter.
- **Décision** :
  - `POST /admin/connectors/rest/import` (`connector:manage`) lit une description
    OpenAPI 3 (3.0 ou 3.1) **en JSON**, donnée telle quelle (`document`), et répond par
    les opérations qu'elle propose, l'adresse de l'API si elle en donne une complète
    (premier serveur, variables remplacées par leur valeur par défaut) et des remarques.
    Rien n'est enregistré ni appelé, et une lecture n'est pas auditée (une tentative
    sans la permission l'est, comme toute modification d'administration refusée,
    ADR-038) : les administrateurs relisent, gardent ce qu'ils veulent, puis ajoutent
    ou modifient l'API comme avant (ADR-112), où la déclaration est vérifiée de
    nouveau, entière. La lecture se fait hors de l'exécuteur des requêtes, sur un fil
    du réservoir des tâches bloquantes ;
  - la plateforme ne télécharge aucune adresse : seul le document donné est lu. Une
    description YAML se convertit d'abord en JSON, une description Swagger 2.0 d'abord
    en OpenAPI 3 ; aucune bibliothèque YAML n'entre côté serveur ;
  - lecture bornée, quelle que soit la forme du document :
    - 768 Kio au plus, pour que le document tienne, échappé, dans le corps d'une
      requête (1 Mo) ;
    - chaque partie lue se paie sur un budget de 200 000 unités, une par partie et une
      de plus par 64 octets lus (16 pour un texte, dont chaque caractère est examiné),
      **chaque fois** qu'elle est lue : une partie que
      d'autres partagent par `$ref` (un résumé, une énumération, une liste `required`,
      les noms des champs d'un corps) coûte autant de fois qu'elle est relue. Le budget
      épuisé, plus rien n'est lu et les chemins suivants sont comptés « non lus » ;
    - références internes seulement (`#/…`), de 512 octets au plus, dix sauts au plus
      pour une même partie (une boucle ou une longue chaîne s'arrête là), `allOf`
      imbriqués sur quatre niveaux ; 200 paramètres listés et 200 champs lus par
      opération ; un texte n'est lu que jusqu'à ce que sa borne peut demander (quatre
      fois sa longueur, plus 64 caractères) ; une énumération montre ses 50 premières
      valeurs, chacune lue tant qu'il reste de la place pour la montrer ; les méthodes
      d'un chemin sont cherchées, jamais ses autres clés parcourues ;
    - 1 000 opérations converties, proposées ou non, puis les chemins suivants sont
      comptés « non lus » ; 200 opérations proposées pesant ensemble 2 Mio au plus en
      JSON, les suivantes comptées ; 100 remarques, les suivantes comptées, celles qui
      concernent toute la description en tête. Une remarque cite un chemin ou un nom
      sur 80 caractères, un caractère caché y devenant « � » : deux noms qui diffèrent
      ne se lisent jamais pareil ;
  - correspondance : chaque opération GET, PUT, POST, DELETE ou PATCH devient une
    opération nommée d'après son `operationId` en `snake_case` (sinon d'après sa méthode
    et son chemin), rendue unique par `_2`, `_3`… dans l'ordre où elles sont
    proposées : les chemins dans l'ordre écrit, les méthodes de chacun dans l'ordre
    GET, PUT, POST, DELETE, PATCH. Intitulé : son résumé, sinon son
    `operationId`, sinon « MÉTHODE chemin » ; description : sa description, sinon son
    résumé, sinon « MÉTHODE chemin » ; un texte ne compte qu'une fois ses caractères
    cachés retirés, et celui qui ne montre plus rien cède la place au suivant. Les
    textes perdent leurs caractères cachés (et la remarque le dit) et sont coupés à
    leur limite par « … ». Paramètres : ceux de chemin
    et de requête (ceux du chemin, sauf si l'opération les redéfinit) et les propriétés
    d'un corps JSON (parties `allOf` fusionnées, champs en lecture seule omis, requis
    seulement si le corps l'est), de type texte, entier, nombre ou booléen ; les valeurs
    d'une énumération rejoignent la description du paramètre ;
  - ce que la déclaration ne porte pas (en-tête, cookie, tableau, objet, variantes
    `oneOf`/`anyOf`, corps non JSON) est laissé de côté et dit quand c'est facultatif ;
    requis, cela écarte l'opération, que son outil ne saurait pas appeler. Il en va de
    même d'un corps requis dont aucun champ ne se déclare (objet libre, `{}`, champs
    en lecture seule) : l'outil n'enverrait aucun corps ; facultatif, il est laissé de
    côté et dit. Un champ facultatif dont la référence ne se lit pas est laissé de côté
    et dit, sans écarter l'opération. Au-delà de 30 paramètres, les requis (et le
    premier champ d'un corps requis dont aucun champ ne l'est, pour que l'outil envoie
    toujours un corps) puis les premiers facultatifs sont gardés. Une opération
    dépréciée est écartée, de même que HEAD, OPTIONS et TRACE ;
  - chaque opération proposée passe les vérifications d'une déclaration : un chemin
    qu'elles refuseraient (`%`, segment `..`…) écarte l'opération, raison donnée.
- **Conséquences** :
  - les textes d'un tiers ne deviennent ce que lit le modèle qu'une fois relus et
    enregistrés par un administrateur ; ses outils restent `write` jusqu'à leur
    association (ADR-112) ;
  - l'écran de l'import (choisir parmi les opérations proposées, cinquante au plus par
    API) est le lot suivant ; restent l'e-mail et les agendas ;
  - tests : `crates/connectors/tests/rest_openapi.rs` (une description d'assistance
    réaliste : noms, textes, paramètres redéfinis par l'opération, `allOf`, références,
    corps facultatif, ce qui est laissé de côté et pourquoi ; documents refusés ;
    adresse incomplète ; une remarque qui cite un caractère caché sans le garder ;
    aucune opération que son outil ne saurait appeler, un champ facultatif illisible
    laissé de côté, un résumé invisible remplacé) ;
    `crates/connectors/tests/rest_openapi_bounds.rs` (bornes des paramètres, des
    opérations et des remarques ; références en boucle, trop enchaînées ou hors du
    document ; `allOf` trop imbriqués ; un petit document aux `allOf` imbriqués lu
    vite, le reste compté ; le premier champ d'un corps requis gardé au-delà de 30
    paramètres) ; `crates/connectors/tests/rest_openapi_cost.rs` (des
    documents proches de 768 Kio, chacun lu en moins de 5 s même sans optimisation :
    liste `required`, résumé, énumération, valeurs longues d'une énumération, noms de
    champs ou de paramètres et types de contenu relus par `$ref`, pointeur trop long, entrée de chemin aux nombreuses
    clés, plafond des conversions, poids des opérations proposées) ;
    `crates/api/tests/rest_openapi_import.rs` (réservé aux administrateurs des
    connecteurs, rien enregistré ni audité, les opérations proposées acceptées telles
    quelles à l'ajout ; descriptions refusées et dites).

## ADR-115 — Écran de l'import OpenAPI (connecteurs, septième lot ; complète ADR-114)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : l'ADR-114 lit les opérations d'une description OpenAPI par l'API
  d'administration seulement, en JSON. Beaucoup de descriptions s'écrivent en YAML, et
  une description en propose souvent plus que les cinquante opérations qu'une API peut
  déclarer : il faut choisir.
- **Décision** :
  - dans le formulaire d'une API REST (ajout ou modification), « Importer une
    description OpenAPI » lit un fichier JSON ou YAML de 768 Kio au plus. Une
    description YAML est convertie en JSON **dans le navigateur** de l'administrateur,
    par la bibliothèque `yaml` chargée seulement quand un tel fichier est choisi, ses
    alias (clés de fusion `<<` comprises) répétés au plus cent fois ; le serveur ne lit
    toujours que du JSON (ADR-114). Un fichier illisible est dit pour ce qu'il a : la
    ligne en cause pour du YAML, des alias trop nombreux (ou une table qui se fusionne
    elle-même, qui se déplie de même), un alias sans son ancre, une clé `<<` qui ne
    fusionne pas une table (ou dont l'alias n'a pas d'ancre), un alias qui se contient
    lui-même (qu'aucun JSON n'écrit). Le champ du fichier est vidé, non remplacé, et
    garde le focus (ADR-043) ;
  - les opérations proposées s'affichent (méthode, chemin, intitulé, nom) avec une case
    chacune, et les remarques de la plateforme dans « Remarques (n) ». Une opération
    dont le nom est déjà dans le formulaire ne se coche pas ; on en coche au plus autant
    qu'il reste de place dans les cinquante d'une API, et le clic le revérifie : ce que
    le formulaire a pris entre-temps (une opération saisie, un nom tapé) n'est pas
    ajouté une seconde fois, et une case cochée qui n'y tient plus se montre décochée ;
  - « Ajouter les opérations cochées » les place dans le formulaire, après les siennes
    (à la place de l'opération vide qu'il ouvre), où elles se relisent et se modifient
    comme les autres ; l'adresse de la description remplit le champ s'il est vide. Rien
    n'est enregistré avant « Ajouter l'API » ou « Enregistrer », où la plateforme
    vérifie la déclaration entière.
- **Conséquences** :
  - une description Swagger 2.0 reste à convertir en OpenAPI 3 avant l'import ;
  - restent l'e-mail et les agendas ;
  - tests : `apps/web/e2e/rest-openapi-import.spec.ts` (un YAML illisible dit ; une
    description YAML lue, ses opérations proposées et sa remarque montrées ; une opération
    cochée ajoutée au formulaire à la place de l'opération vide, l'adresse remplie ;
    relue, l'opération déjà ajoutée ne se coche plus ; une seconde ajoutée ; l'API
    enregistrée et relue dans l'API avec ses deux opérations ; un alias qui se contient
    lui-même, un alias sans ancre, une clé `<<` qui ne fusionne pas une table et des
    alias répétés plus de cent fois dits, cent lus ; puis une API de 49 opérations modifiée : une description JSON en
    propose deux, une seule se coche, une opération saisie entre-temps ôte la place,
    retirée la rend ; l'API enregistrée en tient cinquante) ;
  - les bornes de 768 Kio et de cinquante opérations sont rappelées par l'écran ; la
    plateforme seule les fait respecter.

## ADR-116 — Microsoft 365 relié par chacun : déclaration et liaison (connecteurs, huitième lot)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : les collaborateurs veulent que l'agent lise leurs e-mails et leur agenda.
  Microsoft offre deux voies. Les **permissions d'application** donnent à l'application
  toutes les boîtes de l'annuaire : la plateforme seule empêcherait alors l'agent de lire
  celle d'un autre. Les **permissions déléguées** donnent à l'application ce que la
  personne peut lire elle-même, rien de plus, avec son accord.
- **Décision** :
  - **Permissions déléguées seulement.** La plateforme ne demande jamais de permission
    d'application. Chaque personne relie elle-même son compte ; Microsoft lui fait
    respecter ses propres droits, quoi que fasse le modèle ;
  - **la déclaration** : un administrateur des connecteurs (`connector:manage`, sur toute
    l'entreprise) déclare l'application de l'entreprise dans son annuaire Microsoft Entra
    (`POST /admin/connectors/microsoft365`) :
    - l'identifiant de l'annuaire et celui de l'application, des GUID ;
    - son secret, scellé comme le jeton d'un connecteur, jamais renvoyé ni audité ;
    - une seule par entreprise (index unique), car ses outils porteront des noms fixes ;
    - l'émetteur (celui de l'annuaire) et l'adresse de Microsoft Graph sont ceux de la
      plateforme : l'administration ne les écrit pas et ne les change pas ;
    - la vue des connecteurs montre l'adresse de retour à inscrire dans l'application
      (`/api/account/links/callback`) et combien de personnes ont relié leur compte ;
    - son nom, son activation et son secret changent comme pour un autre connecteur ;
      son secret se remplace mais ne se retire pas ;
  - **la liaison** : une personne qui peut utiliser les connecteurs (`connector:use`)
    relie son compte (`POST /account/links/{connecteur}`) :
    - le navigateur part vers l'annuaire avec son adresse comme indication et les
      permissions `openid`, `email`, `profile`, `offline_access`, `Mail.Read` et
      `Calendars.Read` ; une tentative est scellée dans le cookie `mimorya_link`
      (`state`, `nonce`, PKCE, la personne, dix minutes) ;
    - le retour (`GET /api/account/links/callback`) exige la session de cette personne et
      sa propre tentative, échange le code et vérifie le jeton d'identité ;
    - ce jeton doit nommer **son propre compte** : l'identifiant d'objet de l'annuaire,
      et son nom de connexion (ou une adresse dont l'annuaire a vérifié le domaine) égal
      à l'adresse de son compte, à la casse près. Le compte d'un collègue, même
      accessible à la personne, n'est jamais relié ;
    - sans jeton de renouvellement (accès hors connexion refusé), rien n'est relié ;
  - **ce qui est gardé** : la liaison (`account_links`) garde le sujet, l'adresse, les
    permissions accordées, leurs dates, et le jeton de renouvellement :
    - ce jeton est scellé par la clé maîtresse, lié à l'entreprise, au connecteur et à la
      personne (`account_link.{connecteur}.{personne}`) ;
    - il n'est jamais montré, ni journalisé, ni audité ;
    - le jeton d'accès reçu avec le code n'est pas gardé ;
  - **le retour au navigateur** : il revient sur `/account` en disant seulement
    `liaison=ok`, `autre-compte` ou `echec`. Chaque refus est audité avec sa raison,
    sans jeton ni code, et une nouvelle liaison remplace l'ancienne ;
  - **la fin d'une liaison** : la personne la retire quand elle veut
    (`DELETE /account/links/{connecteur}`) et la voit encore si le connecteur est
    désactivé ou si son droit lui est retiré. La liaison disparaît aussi avec son
    connecteur et avec un compte désactivé, même pendant l'aller-retour chez
    Microsoft : la ligne du compte est tenue au moment de garder la liaison, et un
    connecteur désactivé ou supprimé entre-temps ne relie rien ;
  - **la déclaration de Microsoft** est lue à chaque étape, jamais gardée : une liaison
    est rare. Les retours sont comptés avec les connexions par un fournisseur ;
  - les outils (recherche et lecture des e-mails, agenda) viennent au lot suivant.
    Jusque-là, le connecteur démarre sans outil.
- **Conséquences** :
  - une personne doit se reconnecter à Microsoft pour relier son compte et peut révoquer
    son accord depuis Microsoft ; ce qui est relié ne sert ensuite qu'aux runs faits pour
    elle (lot suivant) ;
  - un compte dont l'adresse diffère de son nom de connexion Microsoft ne peut pas être
    relié : l'administration corrige alors l'adresse du compte ;
  - à l'écran, la carte de l'application déclarée par l'API n'a encore ni « Modifier »
    ni « Supprimer » : ils viennent avec l'écran ;
  - restent : les outils de lecture, l'écran (« Mon compte » et « Connecteurs »),
    l'envoi d'e-mails et la création d'événements sous validation, puis Google Workspace ;
  - tests : `crates/core/src/account_links/tests.rs` (déclaration, identité, permissions
    gardées), `crates/api/tests/account_links.rs` (liaison faite et retirée, jeton scellé
    et absent de l'audit, compte d'un autre refusé, retour d'une autre personne ou
    tentative refusé, sans `connector:use` ni connecteur actif, liaison partie avec le
    connecteur et le compte désactivé), `crates/api/tests/account_link_returns.rs`
    (retour après la désactivation ou la suppression de l'application, application
    d'une autre entreprise), `crates/api/tests/microsoft365_connector.rs`
    (déclaration vérifiée, une par entreprise, secret remplacé jamais retiré, adresses
    fixes, outil inconnu refusé).

## ADR-117 — Outils de lecture Microsoft 365 avec le jeton de la personne servie (connecteurs, lot 8b)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : chacun a relié sa boîte et son agenda Microsoft 365 avec des permissions
  déléguées (ADR-116). L'agent doit maintenant y lire, pour la personne qu'un run sert,
  ce qu'elle peut lire elle-même, et rien d'autre.
- **Décision** :
  - **trois outils, aux noms fixés par la plateforme** (`MICROSOFT365_TOOLS`) :
    - `m365_mail_search` cherche des e-mails par mots (la recherche de Microsoft, par
      pertinence) ou donne les derniers reçus, depuis un jour si indiqué. Les deux ne se
      combinent pas, car Microsoft ne filtre ni ne trie ce qu'il trouve par mots. Il
      renvoie au plus 25 e-mails, 10 par défaut, et dit quand il y en a d'autres ;
    - `m365_mail_read` lit un e-mail : son texte (Graph le convertit du HTML), tronqué
      au-delà de 20 000 caractères, ses destinataires (20 au plus par liste) et s'il a
      des pièces jointes, qui ne sont pas lues ;
    - `m365_calendar_events` lit les rendez-vous de 31 jours au plus (premier et dernier
      compris), 50 au plus, le plus tôt d'abord ;
  - **ce que les outils demandent** :
    - chacun est une lecture (`read`), sans validation, et exige `connector:use`, qu'une
      politique de l'administration garde comme pour un serveur MCP ou une API REST ;
    - leurs entrées sont strictes. L'identifiant d'un e-mail est limité aux caractères
      de Microsoft et encodé entier dans le chemin, `/` compris : il ne peut viser
      qu'un e-mail de la boîte de la personne ;
    - les jours donnés sont ceux du fuseau de la plateforme (`PLATFORM_TIME_ZONE`,
      Paris), celui de la date donnée au modèle ; Graph rend les rendez-vous dans ce
      fuseau ;
    - les identifiants renvoyés sont cités (`email`, `event`) avec l'objet pour libellé.
      Ils sont opaques, donc sans motif de vérification dans les réponses. Le résumé d'un
      appel, gardé au journal, ne contient pas d'objet d'e-mail ;
  - **le jeton est celui de la personne servie, jamais choisi par l'outil** :
    - le contrat d'outil reçoit un port `LinkedAccounts` dans `ToolContext`, qui ne donne
      que le jeton d'accès de la personne du run, pour un type de connecteur ;
    - un sous-agent sert la même personne ;
    - une évaluation de compétence ne sert personne : elle ne lit aucun compte relié ;
  - **le jeton d'accès** (`services/account_links/access.rs`, renouvellement dans
    `renewal.rs`) :
    - à chaque appel, la plateforme relit la liaison et le secret de l'application ;
    - un jeton gardé de cette liaison (même connecteur, même version `refreshed_at`)
      sert jusqu'à deux minutes avant son expiration ;
    - sinon, le jeton de renouvellement est échangé chez Microsoft, avec les seules
      permissions de la plateforme que la personne a accordées. Une permission ajoutée
      depuis par la plateforme n'est pas demandée, car Microsoft refuserait tout. Une
      permission est reconnue à son dernier segment (Microsoft peut nommer une
      permission de Graph par son adresse), et l'accès hors connexion est toujours
      demandé ;
    - aucune transaction n'est tenue pendant l'échange ;
    - les jetons d'accès sont gardés en mémoire du processus seulement, par entreprise et
      par personne, en nombre borné ; la déclaration de Microsoft est gardée une heure ;
  - **la rotation** : un jeton de renouvellement que Microsoft remplace est scellé dans la
    liaison, si elle n'a pas changé depuis sa lecture. Sinon, la liaison refaite garde
    le sien ;
  - **la révocation** : un jeton de renouvellement que Microsoft n'honore plus
    (`invalid_grant`) retire la liaison, sauf si elle a été refaite entre-temps (l'outil
    dit alors de réessayer). Le retrait est audité (`account.link.revoked`, au nom de la
    personne, sans jeton), et l'outil dit à la personne de relier de nouveau son
    compte ;
  - **un jeton d'accès que Graph refuse** (401, révoqué avant son expiration) est oublié :
    l'appel suivant en renouvelle un, et l'outil dit de réessayer ;
  - **les erreurs de Graph n'atteignent jamais le modèle** (le contenu lu l'atteint,
    encadré comme toute donnée d'outil) :
    - Graph est appelé par le client durci des connecteurs, sans redirection et aux
      adresses publiques ;
    - chaque appel est borné en temps (celui d'un appel de connecteur) et à 1 Mo de
      réponse ;
    - pour une erreur, seul le sens de son statut est dit (accès refusé, permission non
      accordée, introuvable, trop de demandes), jamais son texte ;
  - **ce qui n'est jamais montré ni gardé** : le jeton d'accès n'est ni dans le journal
    des appels, ni dans les événements, ni dans l'audit.
- **Conséquences** :
  - ce que lisent les outils dépend des droits de la personne chez Microsoft, que la
    plateforme ne peut pas élargir ;
  - un jeton d'accès révoqué chez Microsoft peut encore servir dans un autre processus
    jusqu'à son expiration (une heure au plus), ou jusqu'à ce que Graph le refuse ;
  - deux renouvellements simultanés pour la même personne sont possibles (sous-agents en
    parallèle). Les deux jetons sont valides et la liaison garde un seul jeton de
    renouvellement ;
  - ce que lisent les outils reste dans l'historique du run (son point de reprise), comme
    tout résultat d'outil, avec la conservation des runs ; le journal des appels n'en
    garde que le résumé ;
  - le contenu d'une boîte est écrit par n'importe quel expéditeur : c'est une donnée
    non fiable, encadrée comme telle. Comme tout résultat d'outil, le modèle peut le
    reprendre dans une proposition de mémoire d'équipe ou d'entreprise, toujours relue
    par un validateur avant d'être lue par d'autres ; seule la mémoire personnelle est
    écrite directement, et jamais depuis un déclencheur (ADR-109) ;
  - un run lancé par une tâche planifiée, une mission ou un déclencheur sert son auteur :
    les outils lisent son compte. Ce qui en sortirait de la plateforme passe par un
    outil d'écriture, sous validation (et l'autonomie d'un déclencheur est plafonnée,
    ADR-109) ;
  - tant que l'écran « Mon compte » n'existe pas (lot suivant), une personne relie son
    compte par l'API seulement ;
  - restent : l'écran (« Mon compte » et la carte de l'application dans « Connecteurs »,
    avec un parcours de bout en bout), l'envoi d'e-mails et la création de rendez-vous
    sous validation, Google Workspace ;
  - tests :
    - `crates/connectors/tests/microsoft365_mail.rs` et `microsoft365_access.rs` :
      requêtes envoyées à un Graph simulé, résultats, refus sans les mots de Graph,
      période bornée, compte de la personne seule, sans liaison, jeton refusé signalé ;
    - `crates/api/tests/microsoft365_tools.rs` : run de la personne avec un jeton
      renouvelé puis gardé, absent du journal et de l'audit ; isolation entre personnes ;
      sans liaison ; politique gardant `connector:use` ; évaluation sans compte relié ;
    - `crates/api/tests/microsoft365_renewals.rs` : liaison révoquée retirée et auditée,
      jeton tourné gardé, jeton refusé renouvelé ;
    - `crates/api/tests/microsoft365_reach.rs` : un sous-agent lit le compte de la
      personne de son run ; un jeton gardé ne sert plus une liaison retirée, refaite ou
      dont l'application est désactivée ; un renouvellement ou une révocation lus avant
      une liaison refaite la laissent intacte ;
    - `crates/api/src/services/account_links/tokens.rs` : jetons gardés.

## ADR-118 — Écrans Microsoft 365 : comptes reliés dans « Mon compte », application dans « Connecteurs » (connecteurs, lot 9)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : l'application Microsoft 365 d'une entreprise (ADR-116) et les outils qui
  lisent avec le jeton de chacun (ADR-117) existaient par l'API seulement. Une personne
  ne pouvait pas relier son compte depuis l'interface, alors que les outils lui disent de
  le faire dans « Mon compte ».
- **Décision** :
  - **« Mon compte » : section « Comptes reliés »**, montrée dès que l'entreprise a
    déclaré son application, ou tant que la personne y garde une liaison :
    - pour chaque application : relié ou non, l'adresse reliée, sa date et les
      permissions accordées ;
    - « Relier mon compte » (« Relier de nouveau » si déjà relié) quand la personne peut
      relier : le navigateur part chez Microsoft, qui le renvoie sur « Mon compte » ;
    - « Retirer la liaison », après confirmation, qui rappelle que l'accord donné à
      l'application se retire aussi chez Microsoft ;
    - une liaison qui ne sert pas (application désactivée, connecteurs retirés à la
      personne) le dit, et se retire ;
    - l'issue du retour (`?liaison=ok|autre-compte|echec`) est dite une fois, puis
      retirée de l'adresse : une page rechargée ne la redit pas ;
  - **« Connecteurs » : l'application de l'entreprise** :
    - « Déclarer l'application Microsoft 365 » n'est proposé que tant qu'aucune n'est
      déclarée (une par entreprise) : nom, ID de l'annuaire, ID de l'application et secret ;
    - sa carte montre ses identifiants, ses outils (`m365_…`), si le secret est enregistré,
      combien de personnes ont relié leur compte, et l'adresse de retour à inscrire dans
      l'application ;
    - « Vérifier la déclaration » ne joint pas Microsoft, et le dit ; les règles des trois
      outils se règlent ensuite comme celles d'une API REST ;
    - « Modifier » change le nom et remplace le secret (vide : le secret enregistré est
      gardé) ; « Supprimer » dit combien de liaisons partent avec l'application ;
  - **les outils nomment de nouveau « Mon compte »** dans leurs messages (ADR-117) ;
  - **parcours de bout en bout** : le serveur des tests envoie les personnes à son
    fournisseur simulé au lieu de l'émetteur de l'annuaire chez Microsoft
    (`BuildOptions::microsoft365_issuer`, jamais réglé en production). Le remplacement
    s'applique quand le fournisseur est joint (départ, retour, renouvellement), jamais à
    la déclaration enregistrée, qui garde l'émetteur de Microsoft : l'application se
    vérifie comme en production. Le fournisseur simulé accepte un identifiant
    d'application en GUID, nomme le compte de l'annuaire (`oid`, nom de connexion) quand
    une liaison est demandée, et ne laisse échanger un code qu'au client à qui il l'a
    donné. Les outils n'y sont jamais appelés : ils sont testés par l'API (ADR-117) ;
  - les options de construction du serveur pour les tests (`BuildOptions`) quittent
    `app.rs` pour `build_options.rs`.
- **Conséquences** :
  - une personne relie, voit et retire son compte sans l'API ; un administrateur déclare,
    modifie et supprime l'application sans l'API ;
  - restent : l'envoi d'e-mails et la création de rendez-vous sous validation, Google
    Workspace ;
  - tests : `apps/web/e2e/microsoft365.spec.ts` (déclaration, secret jamais listé, une par
    entreprise, déclaration vérifiée sans joindre Microsoft et règle d'un outil, liaison
    par le fournisseur simulé et retour sur « Mon compte », compte relié compté et
    annoncé dans la suppression refusée, renommage, application désactivée : liaison
    dite inutile et seulement retirée, confirmation du retrait, suppression de
    l'application, issue d'un retour dite une fois) ;
    `crates/api/tests/microsoft365_connector.rs` (émetteur et Graph de Microsoft
    enregistrés).

## ADR-119 — Envoi d'e-mails et rendez-vous Microsoft 365, sous validation (connecteurs, lot 10a)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : les outils Microsoft 365 lisent la boîte et l'agenda de la personne
  servie (ADR-117). L'agent doit aussi pouvoir envoyer un e-mail et créer un rendez-vous
  en son nom, ce qu'une personne valide. Ce sont des permissions de plus chez Microsoft :
  l'entreprise décide si elle les demande à ses collaborateurs, et chacun les accorde.
- **Décision** :
  - **l'entreprise permet l'écriture**, ou la retire :
    - c'est l'option `writeTools`, donnée à la déclaration de l'application ou changée
      ensuite (`PATCH /admin/connectors/{id}`). Absente, les outils ne font que lire ;
    - le changement est audité (`connector.created`, `connector.updated`). Seule
      l'application Microsoft 365 a cette option ;
    - les outils d'écriture ne sont proposés qu'une fois l'écriture permise. Leurs règles
      peuvent être écrites à tout moment : `MICROSOFT365_TOOLS` nomme les cinq outils ;
  - **les permissions demandées** :
    - quand l'écriture est permise, une liaison demande en plus `Mail.Send` et
      `Calendars.ReadWrite` ;
    - une liaison faite avant ne les accorde pas. « Mon compte » reçoit celles qui
      manquent (`toGrant`), et l'outil dit à la personne de relier de nouveau son
      compte ;
    - le renouvellement demande toutes les permissions de la plateforme que la personne
      a accordées, écriture comprise, que l'écriture soit encore permise ou non :
      la permettre de nouveau n'oblige pas à relier. L'accord reste de toute façon chez
      Microsoft tant que la personne ne le retire pas, et sans les outils le jeton
      n'écrit rien ;
  - **chaque outil dit les permissions dont il a besoin** :
    - c'est `LinkedAccounts::access_token(type, besoins)` ;
    - la plateforme les compare à celles que la liaison a reçues et refuse
      (`NotGranted`) avant d'atteindre Microsoft ;
    - les outils de lecture demandent `Mail.Read` ou `Calendars.Read` ;
  - **deux outils** :
    - `m365_mail_send` : 1 à 10 destinataires, 10 en copie au plus, un objet sur une ligne
      (255 caractères), un texte brut (20 000). Le message est gardé dans les éléments
      envoyés de la personne ;
    - `m365_calendar_event_create` : objet, début (heure de Paris, celle des outils de
      lecture, entre l'an 2000 et l'an 2100, à une heure qui existe ce jour-là), durée de
      5 minutes à 24 heures, lieu, texte et 20 participants au plus, que Microsoft invite.
      Le début et la fin sont envoyés à Graph en UTC : le rendez-vous dure ce qui a été
      validé, même la nuit d'un changement d'heure (une heure répétée est prise à sa
      première occurrence). Le rendez-vous créé est cité (`event`) ;
    - chacun a le risque `external` et exige toujours une validation, quel que soit le
      niveau d'autonomie. Aucune politique d'outil ne retire cette validation : une
      politique peut désactiver l'outil, ou changer son risque et les permissions qu'il
      exige (`connector:use` toujours gardé) ;
    - la description de l'action donne d'abord les destinataires (et, pour un rendez-vous,
      la date et la durée), puis l'objet, le lieu et le début du texte : ce que le modèle
      écrit vient en dernier, et un objet ne peut pas se faire passer pour les
      destinataires. Les paramètres exacts sont montrés entiers à qui valide, et exécutés
      tels quels ;
  - **ce qui part ne cache rien** :
    - les adresses doivent être valides, en ASCII (une lettre d'une autre écriture,
      comme le « е » cyrillique, ou une lettre pleine chasse passerait pour la nôtre), et
      chacune une fois par liste ;
    - l'objet et le lieu tiennent sur une ligne visible ;
    - le texte ne contient aucun caractère caché (retours à la ligne et tabulations
      permis) ;
    - une entrée qui ne respecte pas ces règles est refusée avant la demande de
      validation, jamais nettoyée après : ce que voit qui valide est ce qui part ;
  - **envoyé une fois** :
    - l'écriture est un `POST` à Graph, jamais répété ;
    - quand Graph ne répond pas (délai, réseau) ou répond par une erreur de son côté
      (5xx), l'écriture a pu avoir lieu : l'outil le dit, et demande de vérifier dans
      Microsoft 365 avant de la redemander ;
    - une erreur 4xx dit que rien n'a été fait ;
    - un appel d'outil qui n'est pas une lecture et dépasse le délai de l'exécuteur
      (30 secondes, renouvellement du jeton compris) dit lui aussi que l'action a peut-être
      eu lieu, pour tout connecteur ;
    - un run interrompu pendant l'envoi n'est jamais relancé à l'aveugle
      (ADR-053, ADR-086) ;
    - comme pour la lecture, le texte des erreurs de Graph n'atteint jamais le modèle.
- **Conséquences** :
  - l'agent n'écrit dans un compte Microsoft 365 qu'avec la validation de la personne que
    le circuit des validations désigne : la personne servie, ou son responsable si
    l'entreprise l'a réglé ainsi (ADR-059) ;
  - un e-mail lu peut chercher à faire envoyer quelque chose (injection) : la validation
    en est la garde, et un texte caché ne peut pas la tromper ;
  - le texte d'un e-mail envoyé reste, comme toute entrée d'outil, dans le journal des
    appels du run et dans sa demande de validation, avec leur conservation ; ni l'audit
    ni les journaux techniques ne le gardent, et les avis aux responsables ne reprennent
    pas la description de l'action ;
  - les permissions ne sont vérifiées qu'à l'exécution. Une personne dont la liaison ne
    les accorde pas valide l'action, puis apprend qu'elle doit relier de nouveau son
    compte ; rien n'est parti ;
  - pas encore de réponse dans un fil (l'outil crée un e-mail nouveau, dont le modèle
    reprend l'objet), ni de pièce jointe, ni de rendez-vous sur une journée entière ou
    en ligne ;
  - restent : les écrans (l'option dans le formulaire de l'application, les permissions à
    accorder dans « Mon compte »), lot 10b ; Google Workspace ;
  - tests :
    - `crates/connectors/tests/microsoft365_writes.rs` : outils proposés si l'entreprise
      le permet, toujours validés ; e-mail et rendez-vous envoyés une fois, exactement
      tels que validés ; entrées cachant quelque chose refusées avant la validation, champ
      par champ (destinataires, copie, objet, texte, lieu, participants, début) ; écriture
      peut-être faite dite à vérifier ; jeton refusé signalé ;
    - `crates/connectors/tests/microsoft365_permissions.rs` : chacun des cinq outils
      exige sa permission, refusée sans appel à Graph quand la liaison ne l'accorde pas ;
    - `crates/agent/tests/tool_timeout.rs` : délai dépassé, dit peut-être exécuté pour
      une écriture ;
    - `crates/api/tests/microsoft365_writes.rs` : écriture permise puis retirée,
      auditée ; option refusée à une API REST ; liaison d'avant à refaire, permissions
      manquantes listées, envoi validé puis refusé sans rien écrire chez Microsoft, puis
      permissions accordées, renouvellement avec l'écriture ; e-mail en
      attente de validation (risque, paramètres exacts, description), puis envoyé tel
      que validé avec le jeton de la personne ;
    - tests unitaires : `compose.rs` (adresses, ASCII, ligne visible, texte),
      `calendar_create.rs` (instants, changement d'heure, durée, début borné), `graph.rs`
      (écriture peut-être faite),
      `reach.rs` (permissions dites en mots), `core/src/account_links/tests.rs`
      (permissions demandées, renouvelées et manquantes).

## ADR-120 — Écrans de l'écriture Microsoft 365 : option de l'application, permissions à accorder (connecteurs, lot 10b)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : l'écriture Microsoft 365 (ADR-119) se réglait par l'API seulement.
  L'administrateur ne pouvait pas la permettre depuis « Connecteurs ». Une personne dont
  la liaison n'accorde pas l'envoi n'en était avertie que par l'outil, après avoir
  validé.
- **Décision** :
  - **formulaire de l'application** :
    - une case « Permettre l'envoi d'e-mails et la création de rendez-vous, chacun
      validé », à la déclaration comme à la modification ;
    - à la modification, l'option n'est envoyée que si elle change : l'audit ne dit que
      ce qui change ;
    - le texte d'aide nomme les permissions demandées (lecture, et envoi et écriture
      dans l'agenda si l'envoi est permis). Il rappelle qu'aucun envoi ne part sans
      validation, et que ceux qui ont relié leur compte avant le relient de nouveau ;
  - **carte de l'application** : « Envoi d'e-mails et création de rendez-vous permis,
    chacun validé », ou « Lecture seule » ;
  - **« Mon compte »** :
    - ce que l'agent peut faire est calculé par le serveur (`can`, `serving_scopes`
      dans le cœur) : les permissions dont les outils ont besoin, l'écriture seulement si
      l'application la permet, parmi celles que la personne a accordées (la lecture quand
      Microsoft ne les a pas listées), et rien quand la liaison ne sert pas (application
      désactivée, connecteurs retirés). L'interface le dit en mots (lire vos e-mails, lire
      votre agenda, envoyer des e-mails en votre nom, écrire dans votre agenda), une
      permission inconnue par son nom ; une permission accordée mais que les outils
      n'utilisent pas n'est pas annoncée ;
    - avant la liaison, le texte dit si l'application permet l'envoi, chaque fois après
      validation. La vue d'une liaison donne pour cela `writeTools` ;
    - quand l'application demande des permissions que la liaison n'accorde pas
      (`toGrant`), elles sont dites en mots, et « Relier de nouveau » est mis en avant ;
      rien n'est demandé tant que l'application n'est pas disponible ;
    - le retour d'une liaison et la confirmation de son retrait parlent de lecture et
      d'écriture ;
  - **validation d'un envoi** : rien de neuf. La carte de validation montre la
    description (destinataires d'abord) et les paramètres exacts (ADR-119).
- **Conséquences** :
  - l'écriture Microsoft 365 se règle et s'accorde sans l'API ;
  - le serveur de bout en bout n'a pas de Graph simulé : l'envoi lui-même est testé par
    l'API (`crates/api/tests/microsoft365_writes.rs`, ADR-119), pas par le navigateur ;
  - reste : Google Workspace ;
  - tests :
    - `apps/web/e2e/microsoft365.spec.ts` : application en lecture seule ; Léa voit ce que
      l'agent peut lire ; un renommage n'envoie pas l'option inchangée ; application
      désactivée : plus rien d'annoncé ;
    - `apps/web/e2e/microsoft365-writing.spec.ts` : application déclarée avec l'écriture ;
      Léa, pas encore reliée, en est avertie ; écriture retirée (carte « Lecture seule »),
      Léa relie son compte en lecture ; écriture permise de nouveau : invitation à
      l'accorder, absente tant que l'application est désactivée ; nouvelle liaison qui
      l'accorde ; écriture retirée une fois accordée : la page n'annonce plus l'envoi ;
      chaque étape relue par l'API ;
    - `crates/api/tests/microsoft365_writes.rs` : `writeTools` et `can` dans la vue de la
      liaison (lecture, écriture accordée, écriture retirée, application désactivée) ;
    - `crates/core/src/account_links/tests.rs` : ce qu'une liaison sert.

## ADR-121 — Google Workspace relié par chacun : déclaration et liaison (connecteurs, lot 11a)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : les entreprises sous Google Workspace veulent, comme celles sous
  Microsoft 365 (ADR-116), que l'agent lise les e-mails et l'agenda de chacun. Google
  offre aussi la délégation à l'échelle du domaine : un compte de service qui ouvre toutes
  les boîtes du domaine. Elle est écartée pour la raison qui a écarté les permissions
  d'application de Microsoft : la plateforme seule empêcherait alors l'agent de lire la
  boîte d'un autre.
- **Décision** :
  - **un type de liaison dans le cœur** : `LinkKind` (`core/src/account_links/kind.rs`)
    porte ce qui change d'un fournisseur à l'autre :
    - les permissions demandées à la liaison (identité, lecture, écriture si
      l'entreprise la permet) ;
    - celles dont les outils ont besoin et celles renouvelées ;
    - les paramètres propres à la connexion chez le fournisseur ;
    - ce qu'un jeton doit prouver du compte (`microsoft365.rs`, `google.rs`) ;
    - tout le reste est commun aux deux : tentative scellée, retour, jeton de
      renouvellement scellé, liste, retrait, renouvellement, jetons gardés, audit ;
    - une permission se compare sur son dernier segment, sans tenir compte de la casse :
      Microsoft nomme parfois une permission de Graph par son adresse, Google toujours ;
  - **la déclaration** : un administrateur des connecteurs (`connector:manage`, sur toute
    l'entreprise) déclare le client OAuth de l'entreprise dans Google Cloud
    (`POST /admin/connectors/google-workspace`) :
    - son identifiant, tel que Google Cloud le montre (terminé par
      `.apps.googleusercontent.com`, lettres, chiffres, tirets et points, 255 caractères
      au plus) ;
    - son secret, scellé, jamais renvoyé ni audité ;
    - un seul par entreprise, à côté de l'application Microsoft 365 : index unique
      `connector_instances_one_google_workspace`, migration `0051` ;
    - l'émetteur (`https://accounts.google.com`) et les adresses de Gmail et de l'agenda
      sont ceux de la plateforme ;
    - la vue des connecteurs (`googleWorkspace`) montre l'adresse de retour à inscrire
      (la même que pour Microsoft 365), combien de personnes ont relié leur compte, et si
      ses outils peuvent écrire ;
    - son nom, son activation, son secret (remplacé, jamais retiré) et l'écriture se
      changent comme pour l'application Microsoft 365 (ADR-116, ADR-119) ;
    - la déclaration est commune aux deux applications
      (`services/admin/link_applications.rs`) ; chacune vérifie seulement ce qui lui est
      propre ;
    - ses outils seront nommés `google_<outil>`. Il n'y en a encore aucun : une politique
      qui en nomme un est refusée ;
  - **la liaison** : même route, même retour et mêmes règles que pour Microsoft 365 :
    - les permissions demandées sont `openid`, `email`, `profile`, `gmail.readonly` et
      `calendar.readonly`, plus `gmail.send` et `calendar.events` si l'écriture est
      permise ;
    - Google ne donne un jeton de renouvellement que si l'accès hors connexion est
      demandé (`access_type=offline`), et ne le redonne qu'avec `prompt=consent` : les deux
      sont demandés ;
    - `hd` (le domaine de l'adresse de la personne) lui propose les comptes de son
      domaine. C'est une indication, pas une preuve ;
  - **la preuve du compte** : le jeton d'identité doit porter :
    - le sujet Google ;
    - une adresse que Google a vérifiée (`email_verified`) ;
    - le domaine Workspace du compte (`hd`), égal au domaine de cette adresse. Ce sont
      ainsi les administrateurs du domaine qui garantissent l'adresse, pas quiconque a
      ouvert un compte Google avec elle ;
    - cette adresse doit être celle du compte de la personne, à la casse près ;
    - un compte Google personnel (sans `hd`) n'est jamais relié, même à son adresse ;
  - **le renouvellement** : Google ne reçoit le nom d'aucune permission et renouvelle
    celles que la personne a accordées. Microsoft, lui, reçoit celles de la plateforme que
    la personne a accordées (ADR-117, ADR-119) ;
  - **les jetons d'accès gardés** le sont par entreprise, personne et connecteur, car une
    personne peut relier un compte chez chacun des deux. Un jeton refusé par un
    fournisseur fait oublier tous ceux de la personne ;
  - **les messages** donnés au modèle (application inactive, secret absent, accès
    impossible, renouvellement en échec) nomment le produit concerné ;
  - **les raisons d'audit des refus** :
    - « Compte Google d’une autre adresse » ;
    - « Jeton sans compte vérifié du domaine Workspace » ;
    - le refus d'un fournisseur se dit désormais « Refusée par le fournisseur (code) »,
      pour Microsoft comme pour Google ;
  - **`toGrant`** est vide tant que le connecteur n'est pas disponible : l'écran faisait
    déjà ainsi (ADR-120), le serveur le fait maintenant aussi ;
  - **l'émetteur remplacé** du serveur des tests de bout en bout s'appelle désormais
    `BuildOptions::account_link_issuer` (ADR-118). Il vaut pour les deux fournisseurs.
- **Conséquences** :
  - le connecteur démarre sans outil, comme Microsoft 365 au lot 8a ;
  - pas encore d'écran Google. Jusqu'aux écrans (lot 11d), une application Google
    déclarée par l'API apparaît :
    - dans « Mon compte », avec les textes écrits pour Microsoft et ses permissions par
      leur adresse ;
    - dans « Connecteurs », comme un connecteur ordinaire, sans « Supprimer », alors que
      l'API la supprime ;
  - un compte dont l'adresse n'est pas dans le domaine Workspace de son compte Google ne
    peut pas être relié : un alias d'un autre domaine, par exemple ;
  - restent :
    - les outils de lecture Gmail et Agenda (lot 11b) ;
    - l'envoi et la création de rendez-vous sous validation (lot 11c) ;
    - les écrans (lot 11d) ;
  - tests :
    - `crates/core/src/account_links/google_tests.rs` : déclaration, preuve du compte,
      permissions, paramètres de connexion, type d'un connecteur ;
    - `crates/core/src/account_links/tests.rs` : Microsoft, inchangé, par `LinkKind` ;
    - `crates/api/tests/google_workspace_connector.rs` : déclaration vérifiée, une par
      entreprise à côté de Microsoft 365, adresses fixes, secret remplacé jamais retiré,
      outil inconnu refusé, écriture permise ;
    - `crates/api/tests/google_workspace_links.rs` :
      - permissions et paramètres demandés ;
      - liaison faite et jeton de renouvellement scellé ;
      - renouvellement sans permission nommée, permission non accordée refusée ;
      - écriture permise demandée à la liaison suivante ;
      - rien demandé tant que le connecteur est désactivé ;
      - compte d'un collègue, personnel, d'un autre domaine ou non vérifié refusé ;
      - un jeton gardé par liaison, Microsoft et Google, oubliés ensemble ;
    - `crates/api/src/services/account_links/tokens.rs` : jetons gardés par liaison ;
    - `crates/api/src/services/connectors/reach.rs` : `connector:use` pour Google
      Workspace ;
    - `crates/connectors/src/google_workspace/mod.rs` : connecteur démarré avec son
      secret et sans outil, refusé sans secret, hors du réseau permis ou à la
      déclaration illisible.

## ADR-122 — Outils de lecture Google Workspace avec le jeton de la personne servie (connecteurs, lot 11b)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : chacun relie désormais son compte Google Workspace (ADR-121), mais
  l'agent n'a encore aucun outil pour y lire. Les outils de Microsoft 365 (ADR-117)
  contenaient ce qu'un outil de compte relié fait, quel que soit le fournisseur : un
  client borné, l'accès au jeton de la personne, les refus dits en français.
- **Décision** :
  - **un module commun aux comptes reliés** (`connectors/src/delegated/`) reprend de
    Microsoft 365, sans changer ce qu'il dit :
    - le client borné du fournisseur (`ProviderApi`) : jeton de la personne servie,
      délai, 1 Mo au plus, aucune redirection, aucune répétition ;
    - le jeton pris à la plateforme et les refus dits en français, nommant le produit
      (`reach`). `AsPerson` prend le jeton une fois pour un outil qui lit plusieurs fois ;
    - les champs gardés, bornés, et les jours du fuseau de la plateforme (`shape`) ;
  - **trois outils** : lecture seule (`read`), `connector:use` toujours exigé, même par
    une politique. Chacun vérifie, avant Google, que la personne a accordé sa
    permission :
    - `google_gmail_search` :
      - la recherche de Gmail, avec ses mots et opérateurs (`from:`, `subject:`…),
        sans caractère de contrôle ;
      - ou les derniers e-mails reçus, depuis un jour si indiqué, donné à Gmail comme
        l'instant où ce jour commence à Paris ;
      - 25 au plus, hors spam et corbeille ;
      - Gmail ne renvoie que des identifiants : les en-têtes de chacun (objet,
        expéditeur) sont lus avec le même jeton, cinq à la fois. Le premier refus
        arrête les lectures ; un e-mail supprimé depuis la liste est passé ;
    - `google_gmail_read` :
      - un e-mail par son identifiant (lettres, chiffres, `-` et `_`), dont le texte est
        la partie texte, sinon la partie HTML rendue en texte, dans le jeu de caractères
        qu'elle déclare (`encoding_rs`, déjà dans les dépendances de `reqwest`) ; un
        jeu inconnu, ou que le WHATWG remplace en entier, est lu comme de l'UTF-8 ;
      - le HTML rendu en texte garde ses lignes et sépare les cellules d'un tableau
        (`a | b`), pour qu'une quantité et un prix ne se collent pas ; une cellule vide
        garde sa place, chaque valeur sous sa colonne. Seules les cellules posent ce
        séparateur : un `|` du texte reste tel quel. Seules de vraies balises sont
        retirées, dont celles de Word et d'Outlook (`<o:p>`, `<st1:place>`) : une adresse
        ou un lien entre chevrons, dans un texte brut, reste (`<tel:…>`, `<mailto:…>`).
        Les entités usuelles d'un e-mail en français sont décodées. Un tableau dont le
        HTML est indenté donne une valeur par ligne plutôt que `a | b` : les valeurs
        restent séparées ;
      - texte de 20 000 caractères au plus ;
      - pièces jointes nommées (20 au plus, les autres comptées) mais jamais lues ;
      - parcours des parties borné en profondeur et en nombre ;
    - `google_calendar_events` :
      - l'agenda principal sur 31 jours au plus ;
      - chaque occurrence d'un rendez-vous récurrent à part, dans le fuseau de la
        plateforme ;
      - description rendue en texte et bornée ;
  - **les réponses gardent les champs des outils Microsoft 365** (objet, de, reçu,
    aperçu, lu, lien ; début, fin, journée entière, lieu, organisateur, participants),
    avec les entités HTML décodées ;
  - **les refus** :
    - les mots de Google ne sont jamais montrés au modèle ;
    - un 403 de Google se lit comme une permission non accordée ou une limite de
      demandes atteinte ;
    - un jeton refusé est signalé, pour être renouvelé au prochain appel ;
  - **dans une évaluation de compétence**, aucun compte relié n'est lu (ADR-104).
- **Conséquences** :
  - l'agent lit les e-mails et l'agenda Google de la personne servie, avec ses seuls
    droits ;
  - les outils de Microsoft 365 et ceux de Google partagent leur client et leurs
    règles : une correction de l'un vaut pour l'autre ;
  - restent : l'envoi et la création de rendez-vous sous validation (lot 11c), les
    écrans (lot 11d) ;
  - tests :
    - `crates/connectors/src/delegated/` : messages par produit, jours de Paris ;
    - `crates/connectors/src/google_workspace/` : texte d'un HTML, lecture MIME (jeu de
      caractères, pièces jointes, profondeur), recherche, rendez-vous ;
    - `crates/connectors/tests/google_workspace_mail.rs` et
      `google_workspace_calendar.rs`, contre un Gmail et un agenda simulés
      (`connectors/src/testing/google.rs`) :
      - ce qui est demandé avec le jeton de la personne ;
      - ce qui est gardé ;
      - refus sans les mots de Google ;
      - identifiant invalide refusé avant Google ;
      - jeton refusé signalé ;
      - permission non accordée et absence de liaison sans appel à Google ;
    - `crates/api/tests/google_workspace_tools.rs` :
      - un run lit avec un jeton renouvelé puis gardé ;
      - chacun ne lit que son compte ;
      - rien sans liaison ;
      - une politique garde `connector:use` ;
      - une évaluation ne lit aucun compte.

## ADR-123 — Envoi d'e-mails Gmail et rendez-vous Google Agenda, sous validation (connecteurs, lot 11c)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : les outils Google Workspace lisent la boîte Gmail et l'agenda de la
  personne servie (ADR-122). L'entreprise peut déjà permettre l'écriture (`writeTools`,
  ADR-121), et chaque liaison demande alors l'envoi (`gmail.send`) et l'écriture des
  rendez-vous (`calendar.events`). Il manquait les outils qui écrivent, comme ceux de
  Microsoft 365 (ADR-119).
- **Décision** :
  - **deux outils**, proposés une fois l'écriture permise (`GOOGLE_WORKSPACE_TOOLS`
    nomme les cinq outils, `GOOGLE_WORKSPACE_WRITE_TOOLS` les deux qui écrivent) :
    - `google_gmail_send` : les mêmes entrées que `m365_mail_send` (1 à 10
      destinataires, 10 en copie, un objet sur une ligne, un texte brut). Gmail garde
      le message dans les messages envoyés de la personne ; l'e-mail envoyé est cité
      (`email`) ;
    - `google_calendar_event_create` : les mêmes entrées que
      `m365_calendar_event_create`, dans l'agenda principal de la personne. Google
      envoie l'invitation aux participants (`sendUpdates=all`) ; le rendez-vous créé
      est cité (`event`) ;
    - chacun a le risque `external` et exige toujours une validation, quel que soit le
      niveau d'autonomie. Aucune politique d'outil ne retire cette validation : une
      politique peut désactiver l'outil, ou changer son risque et les permissions qu'il
      exige (`connector:use` toujours gardé) ;
  - **ce que les deux fournisseurs partagent** est écrit une fois, dans
    `connectors/src/delegated/` :
    - les règles de ce qui part (`compose.rs`) : des adresses simples, chacune une
      fois — avant l'unique `@`, les caractères que HTML5 permet (ni espace, ni
      virgule, ni parenthèse, ni guillemet, ni deux-points), après lui un nom de
      domaine d'au moins deux parties, en ASCII, sans adresse IP ; objet et lieu sur une
      ligne visible ; texte sans caractère caché. La règle des adresses ne s'appuie plus
      sur la vérification d'adresse de `validator`, qui acceptait après un seul `@` une
      suite finie par une adresse IP (`lea@exemple.fr, Direction (y[1.2.3.4]`) : un
      en-tête l'aurait lue comme une liste ou un commentaire. Les outils de
      Microsoft 365 suivent la même règle ;
    - l'e-mail (`new_mail.rs`) et le rendez-vous (`new_event.rs`) tels que le modèle
      les demande et que la personne les valide : leurs entrées, leurs limites, leur
      description (les destinataires, la date et la durée d'abord, ce que le modèle
      écrit en dernier) et les instants d'un rendez-vous (heure de Paris, à une heure
      qui existe ce jour-là, première occurrence d'une heure répétée) ;
    - une règle changée l'est pour Microsoft 365 et Google Workspace ensemble ;
  - **le message qu'envoie Gmail** est écrit par la plateforme (format RFC 5322,
    `outgoing.rs`) :
    - les destinataires et la copie, une adresse par ligne ; le message n'est pas
      écrit si l'une d'elles n'est pas une adresse simple, quoi qu'ait laissé passer la
      validation ; l'objet en mots encodés
      (RFC 2047), coupés entre deux caractères, chaque ligne dans les 76 caractères
      permis ; le texte en UTF-8, ses lignes finies
      par CRLF, en base 64 ;
    - ni un objet ni un texte ne peuvent finir un en-tête ou en ajouter un (`Bcc:`),
      même si une règle de validation venait à manquer : ils sont encodés, et seules des
      adresses simples sont écrites telles quelles ;
    - Gmail ajoute l'expéditeur (la personne dont le jeton envoie), la date et
      l'identifiant du message : la plateforme ne choisit pas d'expéditeur ;
  - **le rendez-vous** est envoyé à Google Agenda en instants UTC, avec le fuseau de la
    plateforme pour l'affichage : il dure ce qui a été validé, même la nuit d'un
    changement d'heure. Le texte de l'invitation va dans sa description : Google peut
    le montrer comme du HTML, mais ce qu'il contient est ce que la personne a validé,
    entier ;
  - **envoyé une fois**, comme pour Microsoft 365 : un `POST`, jamais répété ; sans
    réponse ou sur une erreur de Google (5xx), l'outil dit que l'action a peut-être eu
    lieu et demande de vérifier dans Google Workspace avant de la redemander ; une
    erreur 4xx dit que rien n'a été fait ; les mots de Google n'atteignent jamais le
    modèle.
- **Conséquences** :
  - l'agent n'écrit dans un compte Google Workspace qu'avec la validation de la personne
    que le circuit des validations désigne (ADR-059) ;
  - une liaison faite avant que l'écriture soit permise ne l'accorde pas : l'outil
    refuse avant Google et dit à la personne de relier de nouveau son compte
    (ADR-121) ;
  - le texte d'un e-mail envoyé reste, comme pour Microsoft 365, dans le journal des
    appels du run et dans sa demande de validation, avec leur conservation ; ni l'audit
    ni les journaux techniques ne le gardent ;
  - pas encore de réponse dans un fil, ni de pièce jointe, ni de rendez-vous sur une
    journée entière ou avec une visioconférence Meet ;
  - reste : les écrans (formulaire du client Google, « Mon compte »), lot 11d ;
  - tests :
    - `crates/connectors/tests/google_workspace_writes.rs` : outils proposés si
      l'entreprise le permet, toujours validés ; e-mail envoyé une fois, tel que
      validé (message relu : en-têtes, objet, texte) ; rendez-vous créé en instants,
      participants invités ; entrées cachant quelque chose refusées avant la
      validation ; écriture peut-être faite dite à vérifier ; jeton refusé signalé ;
    - `crates/connectors/tests/google_workspace_permissions.rs` : chacun des cinq
      outils exige sa permission, refusée sans appel à Google quand la liaison ne
      l'accorde pas ;
    - `crates/api/tests/google_workspace_writes.rs` : outils proposés tant que
      l'entreprise le permet ; e-mail en attente de validation (risque, paramètres
      exacts, description), puis envoyé tel que validé avec le jeton de la personne ;
      rendez-vous validé créé dans son agenda ;
    - tests unitaires : `outgoing.rs` (message, objet long coupé en mots relus
      entiers, lignes bornées, aucun en-tête ajouté par une entrée non validée, adresse
      qui n'est pas simple refusée), `new_event.rs` (instants, changement d'heure,
      durée, début borné, description), `compose.rs` (adresses simples : ce qui finirait
      un en-tête ou ferait une liste, avec un seul `@`), `calendar_create.rs`
      (rendez-vous envoyé à Google).

## ADR-124 — Écrans Google Workspace : client de l'entreprise et « Mon compte » selon le fournisseur (connecteurs, lot 11d)

- **Date** : 2026-10-27 · **Statut** : acceptée
- **Contexte** : le client Google Workspace se déclarait par l'API seulement (ADR-121),
  et « Mon compte » parlait de Microsoft pour tout compte relié : le retour d'une
  liaison, l'accord demandé, la confirmation d'un retrait. Les permissions de Google,
  nommées en entier par le serveur (`https://www.googleapis.com/auth/gmail.readonly`),
  n'y étaient pas dites en mots.
- **Décision** :
  - **« Connecteurs »** :
    - « Déclarer le client Google Workspace » tant que l'entreprise n'en a pas : nom, ID
      client OAuth (vérifié par le navigateur comme par le serveur, terminé par
      `.apps.googleusercontent.com`, le format attendu dit par le champ), code secret, et
      la case qui permet l'écriture (ADR-123) ;
    - à la modification : le nom, le secret (vide, il est gardé) et l'option d'écriture,
      envoyée seulement si elle change ; l'ID client reste, comme les liaisons faites ;
    - le texte d'aide dit ce que l'entreprise crée dans Google Cloud : un client
      « Application Web », les API Gmail et Google Calendar activées, un écran de
      consentement OAuth de type « Interne » (les comptes Workspace de l'entreprise
      seulement) qui déclare dans « Accès aux données » les champs d'application de
      lecture, et d'envoi si l'écriture est permise. Il rappelle qu'il n'y a jamais de
      délégation à l'échelle du domaine ;
    - la carte montre l'ID client, les outils `google_…`, le secret enregistré, le nombre
      de comptes reliés, l'écriture permise ou non, et l'URI de redirection à inscrire
      dans le client. La vérification dit que Google n'est pas appelé ; la suppression,
      que les comptes reliés s'en vont avec le client ;
  - **« Mon compte »** :
    - chaque liaison nomme son fournisseur d'après son type (`ACCOUNT_PROVIDERS`) : qui
      demande l'accord, et où il reste après un retrait (« reste chez Google ») ;
    - les permissions des deux fournisseurs sont dites en mots (`ACCOUNT_PERMISSIONS`) ;
    - le retour d'une liaison (`?liaison=`) ne dit pas le fournisseur : son texte est le
      même pour tous. Le refus d'un autre compte (celui d'un collègue, par exemple)
      est neutre ; un échec, qui est aussi la réponse à un compte Google personnel,
      sans domaine Workspace (ADR-121), rappelle que chez Google il faut le compte
      Workspace de l'entreprise, non un compte personnel ;
  - **suites de l'audit de l'ADR-123** : le refus d'une adresse dit « adresse
    électronique simple attendue (nom@domaine.fr, en caractères ASCII) », ce qu'est la
    règle ; le commentaire de la règle dit qu'elle suit HTML5 : un point mal placé
    (`.lea@…`) n'est pas refusé par la plateforme, puisqu'il ne change aucun en-tête, et
    le fournisseur accepte ou refuse l'adresse.
- **Conséquences** :
  - Google Workspace se déclare, se règle et se relie sans l'API, comme Microsoft 365 ;
  - le serveur de bout en bout envoie les personnes à son fournisseur simulé, qui joue
    Google (`BuildOptions::account_link_issuer`, ADR-121) ; Gmail et l'agenda ne sont pas
    simulés : la lecture et l'écriture sont testées par l'API (ADR-122, ADR-123) ;
  - tests :
    - `apps/web/e2e/google-workspace.spec.ts` : client déclaré avec l'écriture (secret
      jamais listé, un seul par entreprise, déclaration vérifiée sans Google, cinq
      outils) ; Léa avertie que Google demande son accord et que l'agent pourra envoyer
      après validation ; liaison au fournisseur simulé, ce que l'agent peut faire dit en
      mots, envoi compris ; écriture retirée (seule l'option envoyée) : Léa lit
      « lecture » seulement ; retrait de la liaison (« reste chez Google ») ; suppression
      du client ; chaque étape relue par l'API ;
    - `apps/web/e2e/microsoft365.spec.ts` : le retour d'une liaison, dit sans nommer le
      fournisseur ; l'échec rappelle le compte à utiliser chez Google ;
    - ce que les tests de bout en bout des deux fournisseurs partagent (liaison au
      fournisseur simulé, lecture par l'API, suppression après chaque test) est dans
      `apps/web/e2e/support/account-links.ts`.

## ADR-125 — Passerelle Slack : application déclarée par l'entreprise, événements signés, identité Slack reliée par chacun (passerelles, lot 12a)

- **Date** : 2026-10-28 · **Statut** : acceptée
- **Contexte** : les personnes ne parlent à l'agent que dans l'application web. La
  feuille de route prévoit des passerelles de conversation (Slack, Teams, Telegram) où
  l'identité de canal est liée au compte, avec les mêmes droits que sur le web
  (ARCHITECTURE, « Concepts repris »). Une passerelle n'apporte aucun outil à l'agent :
  c'est un canal, non un connecteur. Slack vient en premier : ses événements sont
  signés par l'application elle-même, son API dit l'adresse de chaque personne.
- **Décision** :
  - **une table à part**, `gateways`, plutôt qu'un type de connecteur : une passerelle
    par service et par entreprise (`kind = 'slack'`), avec ce que Slack a dit de
    l'application à sa déclaration, son espace de travail (`workspace_id`, chaque
    événement doit en venir) et son utilisateur bot (dont les messages ne sont jamais
    pris pour ceux d'une personne). Le jeton du bot (`xoxb-…`) et le secret de
    signature sont scellés dans le magasin de secrets (`gateway.<id>.bot_token`,
    `gateway.<id>.signing_secret`), jamais rendus ni audités ;
  - **la déclaration** (`POST /admin/gateways/slack`, `connector:manage` sur toute
    l'entreprise, sous le verrou d'administration, auditée) : le jeton et le secret
    sont vérifiés tels que Slack les écrit, puis le jeton est essayé auprès de Slack
    (`auth.test`) avant que rien ne soit gardé ; un jeton qui n'est pas celui d'un bot
    est refusé. Les refus de Slack sont dits en français, ses mots réduits à son code
    d'erreur. Un nouveau jeton doit être celui d'une application du même espace de
    travail : les identités reliées sont celles de cet espace. La suppression emporte
    les identités et les secrets. La liste donne l'adresse à donner à Slack
    (« Event Subscriptions »), les permissions de l'application (`chat:write`,
    `im:history`, `users:read`, `users:read.email`) et l'événement auquel elle
    s'abonne (`message.im`), sans rien d'autre : elle ne lit ni canal, ni fichier ;
  - **la réception** (`POST /api/gateways/slack/{id}`, avant toute session) : la
    requête nomme sa passerelle, dont l'entreprise est lue par une fonction
    `SECURITY DEFINER` minimale (`auth_gateway_tenant`), puis prouve qu'elle vient de
    l'application par sa signature (HMAC-SHA256 de `v0:<horodatage>:<corps>` avec le
    secret de signature, comparée en temps constant, à cinq minutes près). Une
    passerelle inconnue, une signature absente, fausse ou trop ancienne, et une
    passerelle dont ce serveur ne peut ouvrir le secret (journalisé pour
    l'exploitant) répondent de même (404). Le corps est borné (256 Kio). La vérification de l'adresse par Slack
    reçoit son défi ; un événement d'un autre espace de travail, d'une passerelle en
    pause, déjà pris (Slack le renvoie s'il n'a pas eu de réponse à temps : son
    identifiant est gardé un jour, `gateway_events`), et tout ce qui n'est pas le
    message d'une personne dans sa conversation directe avec l'application (une
    modification, un fichier, le message d'un bot, celui de l'application) est pris
    et ignoré. Ce que dit un message n'est jamais journalisé ni audité ;
  - **troisième exception à l'en-tête des requêtes du navigateur**, après la réception
    SAML (ADR-068) et l'envoi à un déclencheur (ADR-109) : ce chemin exact seulement,
    l'identifiant sous sa forme canonique, sa signature seule l'autorisant ;
  - **la liaison de l'identité Slack** :
    - une personne qui écrit à l'application sans avoir relié son identité reçoit, dans
      sa conversation (dans son fil s'il y en a un), un lien vers son compte
      (`/account/gateway-link?code=…`, le code passé depuis dans le fragment,
      ADR-126), valable quinze minutes. Le lien porte la
      passerelle et l'utilisateur Slack, scellés pour l'entreprise par la clé maîtresse
      du serveur ; rien n'est enregistré. Le message ne reprend rien de ce qu'elle a
      écrit, et le nom de l'assistant y est échappé pour Slack (`&`, `<`, `>`) ;
    - ouvert dans sa propre session, le lien est montré
      (`GET /account/gateways/link`, devenu `POST /account/gateways/link/preview`,
      ADR-126), puis confirmé (`POST`, compté avec les connexions
      par un fournisseur) : Slack est alors interrogé de nouveau (`users.info`), et la
      liaison n'est faite que si cet utilisateur Slack est de l'espace de travail de
      l'entreprise, ni désactivé ni bot, et a l'adresse du compte. L'adresse est la
      preuve, comme pour les comptes reliés (ADR-116) : l'entreprise gère l'une et
      l'autre, et un lien ouvert par quelqu'un d'autre (Léa envoie son lien à Paul) ne
      relie rien. Un refus est audité avec sa raison (`gateway.identity_refused`),
      jamais une adresse ; la personne n'est jamais montrée l'adresse Slack d'un autre.
      Une passerelle en pause ne montre ni ne relie rien ; quand Slack refuse
      l'application elle-même (jeton révoqué, permission manquante), la personne est
      invitée à prévenir son administrateur ;
    - une liaison nouvelle remplace la précédente de la personne, et la liaison de cet
      utilisateur Slack à quelqu'un d'autre (Slack lui a donné l'adresse d'une autre
      personne depuis) ; l'audit le dit (`replacedOwn`, `replacedOtherPerson`) ;
    - la liaison exige `agent:use` ; la personne la voit et la retire dans « Mon
      compte » (`GET /account/gateways`, `DELETE /account/gateways/{id}`) ; un compte
      désactivé perd ses identités, comme ses comptes reliés ; les administrateurs ne
      voient que leur nombre ;
  - **lots suivants** : les écrans (12b), dont la page du lien
    (`/account/gateway-link`, aperçu puis confirmation), la passerelle dans
    « Connecteurs » et l'identité dans « Mon compte ». D'ici là, le lien que Slack
    transmet mène à la page « introuvable » de l'application : la liaison se fait par
    l'API, et la passerelle ne se déclare que par l'API. Puis une personne reliée parle
    à l'agent dans Slack (12c : une conversation par fil, les réponses écrites dans le
    fil, l'adresse vérifiée à chaque message, les validations renvoyées vers
    l'application web) ; d'ici là, ses messages sont pris et ignorés. Puis Telegram et
    Teams.
- **Conséquences** :
  - données personnelles : l'identifiant Slack et l'adresse que Slack a donnée sont
    gardés tant que la personne ne retire pas la liaison, que son compte n'est pas
    désactivé ni la passerelle supprimée ; les identifiants d'événements, un jour ;
  - le client de l'API Web de Slack est borné en temps (dix secondes) et en taille,
    ne suit aucune redirection et n'est jamais relancé (`connectors/src/slack/`) ;
    `BuildOptions::slack_api` le fait parler, dans les tests, à un Slack simulé
    (`connectors/src/testing/slack.rs`), qui signe les événements comme Slack ;
  - tests : `crates/core/src/gateways/` (signature, dont l'exemple de la
    documentation de Slack ; événements pris ; preuve de l'identité),
    `crates/api/tests/slack_gateway_admin.rs`, `slack_gateway_events.rs` (dont
    `only_a_gateway_event_goes_without_the_header`), `slack_gateway_links.rs` (lien
    ouvert par un autre, d'un autre espace de travail, sans adresse, désactivé,
    expiré, d'une autre entreprise ; `agent:use`, passerelle en pause, jeton révoqué ;
    remplacement ; compte désactivé), `slack_gateway_companies.rs` (événement signé
    pour une entreprise envoyé à la passerelle d'une autre ; purge des événements
    d'un jour dans chaque entreprise).

## ADR-126 — Écrans de la passerelle Slack : application dans « Connecteurs », identité dans « Mon compte », page du lien (passerelles, lot 12b)

- **Date** : 2026-10-28 · **Statut** : acceptée · complète ADR-125, dont elle remplace
  le code du lien dans la requête (`?code=`) et la route `GET /account/gateways/link`
- **Contexte** : la passerelle Slack ne se déclarait que par l'API, et le lien que
  l'application envoie dans Slack menait à la page « introuvable » de l'application web
  (ADR-125). Ce lien portait son code dans la requête : le code, qui suffit à relier une
  identité pendant quinze minutes à qui ouvre le lien dans la session d'une personne
  ayant l'adresse que Slack donne, partait avec la page vers le serveur et pouvait être
  écrit dans les journaux d'un proxy ou d'un serveur web.
- **Décision** :
  - **le code dans le fragment** : le lien est `/account/gateway-link#<code>`. Le
    navigateur n'envoie jamais le fragment avec la page ; la page le lit et le remet
    dans le corps d'une requête : l'aperçu devient
    `POST /account/gateways/link/preview` et la confirmation
    `POST /account/gateways/link`, tous deux avec `{ code }` (`GatewayLinkCode`, borné,
    strict). L'aperçu ne modifie rien et n'interroge pas Slack ; seule la confirmation
    reste comptée avec les connexions par un fournisseur (ADR-125) ;
  - **la page du lien** est hors du cadre de l'application (`app/account/gateway-link`,
    hors du groupe `(app)`), dans le cadre des pages de connexion : une personne non
    connectée n'est pas renvoyée vers la connexion, ce qui perdrait le fragment, mais
    invitée à se connecter puis à rouvrir le lien (il vaut quinze minutes, et
    l'application en envoie un nouveau au message suivant). Connectée, elle voit
    l'utilisateur Slack, l'espace de travail et la passerelle que le lien relierait,
    puis confirme ; un lien incomplet, expiré ou refusé est dit avec la raison que donne
    le serveur. La page suit celles des autres liens à usage unique (invitation, mot de
    passe oublié, lien de connexion de l'entreprise : `useLinkToken`, `LinkPage`), qui
    gagnent l'état « connectez-vous d'abord » d'un lien ouvert hors de la session qu'il
    demande (401) ;
  - **« Connecteurs »**, section « Passerelles de conversation », avant les types de
    connecteurs : « Déclarer l'application Slack » tant que l'entreprise n'en a pas
    (nom, jeton du bot et secret de signature, vérifiés par le navigateur comme par le
    serveur) ; un texte d'aide dit ce que l'entreprise crée sur api.slack.com (les
    quatre permissions du bot, l'onglet « Messages » de « App Home », l'abonnement à
    `message.im`). La carte montre l'espace de travail, l'utilisateur bot, le jeton et
    le secret enregistrés (jamais réaffichés), le nombre de comptes reliés, l'adresse
    des événements à donner à Slack, les permissions et l'événement ; elle se modifie
    (jeton et secret laissés vides : gardés), se désactive, se réactive et se supprime,
    la confirmation disant que les comptes reliés s'en vont avec elle ;
  - **« Mon compte »**, carte « Messageries reliées », montrée seulement si l'entreprise
    a une passerelle : reliée, l'identité Slack, l'adresse que Slack a donnée et la
    date ; sinon, comment la relier (écrire à l'application dans l'espace de travail) ;
    une passerelle en pause ou une personne sans `agent:use` est dite indisponible, et
    l'identité reste à retirer ;
  - **suite de l'audit de l'ADR-125** : l'en-tête de `services/gateways/events.rs`
    est replié à la largeur des commentaires.
- **Conséquences** :
  - le Slack simulé des tests écoute, pour les tests de bout en bout, à une adresse
    fixe (`FakeSlack::start_on`, 127.0.0.1:4103 dans `e2e_server`) et liste ce que
    l'application a écrit (`GET /test/posts`) ; il connaît l'utilisateur Slack de Léa ;
  - tests : `apps/web/e2e/slack-gateway.spec.ts` (déclaration, carte et adresse des
    événements relues dans l'API, sans jeton ni secret ; message direct signé par le
    test comme Slack le signe, lien lu chez le Slack simulé ; lien ouvert hors session
    puis dans celle de Léa, liaison relue ; passerelle désactivée, le lien rouvert
    disant pourquoi il ne relie rien, liaison retirée,
    passerelle renommée sans changer ses secrets, réactivée, supprimée, l'adresse
    répondant alors comme une passerelle inconnue) ; `slack_gateway_links.rs` et
    `slack_gateway_events.rs` suivent le code dans le fragment et l'aperçu en `POST`.

## ADR-127 — Conversations avec l'agent dans Slack : un fil par conversation, la réponse écrite dans le fil (passerelles, lot 12c)

- **Date** : 2026-10-28 · **Statut** : acceptée · complète ADR-125 et ADR-126
- **Contexte** : une personne relie son identité Slack (ADR-125, ADR-126), mais ses
  messages à l'application Slack de l'entreprise étaient pris puis ignorés. Elle doit
  pouvoir parler à l'agent dans Slack comme dans l'application, avec les mêmes droits,
  et y lire ses réponses, sans que Slack devienne une voie pour agir au nom de
  quelqu'un d'autre ni pour décider d'une validation.
- **Décision** :
  - **le message d'une personne reliée** est sa propre demande. Slack attendant une
    réponse en quelques secondes, l'événement est répondu aussitôt (200) et le message
    traité en arrière-plan ; ce qu'il dit n'est ni journalisé ni audité. Il est lu en
    texte simple pour l'agent : liens, mentions et caractères échappés de Slack
    rendus lisibles (`plain`) ;
  - **qui écrit, demandé à chaque message** : Slack est interrogé (`users.info`). S'il
    ne dit plus que l'utilisateur Slack est la personne (adresse changée, désactivé,
    d'un autre espace de travail : la même règle qu'à la liaison), l'identité est
    déliée, auditée (`gateway.identity_unlinked`, avec la raison, jamais l'adresse), la
    personne prévenue dans le fil et un nouveau lien lui est offert ; rien n'est lancé.
    S'il ne répond pas, rien n'est lancé ; l'identité reste. Un compte au mot de passe
    provisoire ne lance rien, et la raison est dite ; un compte désactivé a perdu ses
    identités : il reçoit un lien, qu'il ne peut ouvrir sans session. Un message qui
    arrive pendant l'arrêt du service ne lance rien, et le dit ;
  - **un fil par conversation** (`gateway_threads`) : un message hors de tout fil ouvre
    une conversation nouvelle de la personne, titrée de ses premiers mots, tenue dans le
    fil qu'il commence ; la conversation et le fil s'ouvrent dans la transaction du
    tour (un run qui ne démarre pas n'en laisse pas). Une réponse dans ce fil poursuit
    la conversation, avec son historique. Un fil qu'un autre message a ouvert entretemps
    est refusé comme occupé ; un fil tenu pour un autre compte (identité reliée depuis
    à quelqu'un d'autre) est refusé ;
  - **le run** est lancé au nom de la personne, avec ses droits tels qu'ils sont et
    l'autonomie de l'entreprise, comme un tour de l'application (`TurnSender::Gateway`),
    sur le canal `slack` (`agent_runs.channel`, contrainte élargie), que les politiques
    d'accès peuvent lire (`context.channel`). Ses messages portent leur service
    (`metadata.gateway = "slack"`). Les refus du démarrage (pas de modèle, `agent:use`
    retiré, entreprise suspendue, demande déjà en cours, service qui s'arrête) sont dits
    dans le fil. À l'arrêt, les messages en cours de traitement sont attendus avant les
    runs, de sorte que le run qu'ils ont lancé l'est aussi ;
  - **la réponse dans le fil** : chaque fin ou pause d'un tel run, quel que soit le
    chemin qui l'a produite (son issue, une validation décidée, le balayage des runs
    bloqués, un arrêt), est l'événement `run_finished` de son journal. Un passage de
    trois secondes, dans les processus qui exécutent les runs, un à la fois (ADR-086),
    lit celles des dernières 24 heures encore sans réponse par une fonction
    `SECURITY DEFINER` minimale (`system_unreplied_gateway_events` : entreprise et
    événement, le run et l'événement de l'entreprise du fil), puis chacune dans son
    entreprise, et l'écrit dans le fil avec le jeton de cette entreprise. La lecture part
    des fils actifs (un tour ou une réponse depuis 30 jours, `gateway_threads.active_at`,
    plus que les sept jours d'une validation) : son coût suit les conversations tenues
    dans Slack, non tous les runs. Les réponses d'un fil sont écrites dans l'ordre : une
    réponse à réessayer retient celles qui la suivent, tant qu'elle peut l'être (son
    événement des dernières 24 heures) ; plus ancienne, elle n'est plus essayée et ne
    retient plus rien. Le verrou du passage n'est pris
    que s'il y a quelque chose à répondre. Ce qui est écrit : la
    réponse telle que la conversation la montre (un échec compris), en balisage Slack
    (`from_markdown` : gras, titres, listes, liens web ; le code tel quel ; tout le reste
    échappé, sans mention ni commande possible), coupée au-delà de 3 000 caractères en
    disant que la suite est dans l'application ; une pause dit qu'une validation est à
    donner dans l'application, et que la suite viendra dans le fil ; un arrêt le dit.
    Chacune porte le lien de sa conversation ;
  - **ce qui devient de chaque réponse** (`gateway_replies`, une par événement, jamais
    ce qui est écrit) : `sent` ; `refused` par Slack, jamais réessayée ; Slack injoignable
    ou débordé, réessayée après 30 s, 2 min, 8 min (ou quand Slack le dit, dans l'heure),
    puis `failed` à la quatrième tentative ; `skipped` quand la passerelle est en pause
    ou que l'utilisateur Slack qui a écrit n'est plus l'identité reliée de la personne.
    Une réponse est écrite deux fois quand Slack l'a prise mais a répondu trop tard (dix
    secondes), ou que le service s'arrête entre son envoi et son enregistrement ; un
    message pris (Slack, répondu, ne le renvoie pas) est perdu sans rien dire si le
    processus disparaît avant que son run soit enregistré ;
  - **les validations** restent dans l'application : Slack ne permet pas d'en décider.
    Des boutons de validation dans Slack demanderaient l'interactivité de
    l'application (une autre adresse signée et une autre permission) : un lot à venir.
- **Conséquences** :
  - données personnelles : un fil garde l'utilisateur Slack qui a écrit et sa
    conversation directe avec l'application, tant que la conversation existe ; il part
    avec la passerelle, la personne ou la conversation. Les réponses ne gardent que des
    identifiants et leur sort ;
  - la plateforme n'apporte pas de nouvelle permission : une conversation dans Slack
    est un tour de la personne, et les règles de l'application s'y appliquent ;
  - aucune limite propre aux messages de Slack, comme pour les tours de l'application :
    la file des runs borne ce qui s'exécute à la fois (ADR-085), et la limite globale par
    adresse (600 requêtes par minute et par processus) vaut pour les événements de Slack,
    qu'il envoie de ses propres adresses pour toutes les entreprises ; une limite par
    passerelle viendra si l'usage le demande ;
  - le Slack simulé des tests peut être rendu indisponible (503) ; les tests écrivent
    les réponses à la demande (`BuildOptions::manual_gateway_replies`) ;
  - tests : `crates/core/src/gateways/slack/mrkdwn.rs` (échappement, balisage, texte
    simple, coupe), `services/gateways/reply_text.rs`, `replies.rs` (tentatives),
    `crates/api/tests/slack_gateway_conversations.rs` (conversation dans un fil, suite
    dans le fil, autre conversation, pause puis réponse, demande occupée, Slack muet,
    identité déliée, `agent:use` retiré, mot de passe provisoire, fil tenu pour un autre
    compte, fil ouvert entretemps, service qui s'arrête),
    `slack_gateway_replies.rs` (passerelle en pause, identité retirée, refus, Slack
    injoignable puis rejoint, abandon, réponses d'un fil dans l'ordre, d'un passage à
    l'autre et dans un même passage, réponse trop ancienne qui ne retient plus son fil),
    `slack_gateway_companies.rs` (deux entreprises, chacune répondue dans ses fils) ;
  - reste à venir : la conversation montrée « depuis Slack » dans l'application, avec
    son parcours de bout en bout (12d), puis Telegram et Teams.

## ADR-128 — Conversation tenue dans Slack montrée dans l'application (passerelles, lot 12d)

- **Date** : 2026-10-28 · **Statut** : acceptée · complète ADR-127
- **Contexte** : une conversation ouverte dans Slack (ADR-127) figurait dans la liste de
  la personne comme toute autre. Rien n'y disait qu'elle se tient aussi dans un fil
  Slack, ni lesquels de ses messages y ont été écrits, ni que ce qu'on y écrit dans
  l'application n'est répondu que dans l'application. La réponse écrite dans le fil y
  renvoie pourtant (« ouvrir la conversation »), en particulier pour donner une
  validation.
- **Décision** :
  - **la conversation dit où elle se tient** : `ConversationSummary.gateway` (`slack`,
    sinon `null`), dans la liste et dans le détail, lu des fils des passerelles
    (`gateway_threads`, `conversations_held`) dans la transaction de la lecture, sous
    RLS. Seul le service est donné : ni l'utilisateur Slack, ni le fil, ni l'espace de
    travail ;
  - **la liste** marque « Slack » chaque conversation tenue dans un fil ;
  - **la conversation** : chaque message écrit dans Slack porte « Écrit dans Slack »
    (`metadata.gateway`, ADR-127). Une note dit que, tant que la passerelle est active
    et le compte Slack relié, les messages écrits dans Slack y reçoivent leur réponse,
    et que ce qui est écrit dans l'application reçoit sa réponse dans l'application
    seulement. La conversation reste montrée tenue dans Slack quand la passerelle est
    en pause ou l'identité retirée : son fil existe toujours, et redevient répondu
    dès que la passerelle est réactivée ou l'identité reliée de nouveau. Un run lancé
    depuis Slack se suit en direct, et sa demande de validation se donne, comme celui
    d'un tour de l'application ;
  - **un message écrit dans l'application** dans une telle conversation la poursuit,
    avec l'historique (canal `web`). Sa réponse n'est pas écrite dans le fil, qui n'en
    montrerait pas la question. Les messages suivants de la personne dans le fil
    poursuivent la même conversation, l'historique compris ;
  - le modèle des tests de bout en bout joue à part la conversation écrite dans Slack,
    question puis suite dans le fil (`e2e_server/slack.rs`). Le Slack simulé reçoit des
    messages datés chacun d'un horodatage nouveau, hors fil ou dans un fil
    (`slackTs`, `sendDirectMessage`, `inThread`).
- **Conséquences** :
  - aucune donnée nouvelle n'est gardée ; la lecture ajoute une requête par liste ou
    détail, sur l'index unique des conversations des fils ;
  - tests : `crates/api/tests/slack_gateway_app_view.rs` (liste et détail avec leur
    service, conversation ouverte dans l'application sans service, message marqué,
    message écrit dans l'application répondu là seulement, rien pour une autre
    personne), `apps/web/e2e/slack-conversations.spec.ts` (Léa reliée pose sa question
    dans Slack ; réponse dans le fil, en balisage Slack, avec l'adresse de la
    conversation ; conversation listée « Slack », message « Écrit dans Slack », note ;
    suite dans le fil répondue là et dans la conversation) ;
  - reste à venir : validations depuis Slack, Telegram, Teams.

## ADR-129 — Validations données dans Slack : les boutons de l'action dans le fil (passerelles, lot 12e)

- **Date** : 2026-10-28 · **Statut** : acceptée · complète ADR-127 et ADR-128
- **Contexte** : un run lancé depuis Slack qui attend une validation le disait dans son
  fil, avec le lien de la conversation où la donner (ADR-127). La personne devait
  quitter Slack pour une action qu'elle seule décide, le plus souvent la sienne. Slack
  sait montrer des boutons dans un message et envoyer à l'application celui qu'une
  personne presse (« Interactivity »), signé comme ses événements.
- **Décision** :
  - **la pause montre les actions qui attendent la personne elle-même** : chacune, sa
    description (texte simple, échappé, bornée à 600 caractères) et deux boutons,
    « Valider » et « Refuser » (`approval_blocks`, dix au plus ; les autres dans
    l'application). Une action qu'un responsable valide (ADR-059) n'a pas de bouton :
    le fil dit qu'elle attend la validation de son responsable, dans l'application. Les
    actions sont lues au moment d'écrire la pause : une action déjà décidée n'a plus
    de bouton ;
  - **le bouton pressé arrive à la même adresse** que les événements : Slack l'envoie
    encodé comme un formulaire (`payload=`), signé du même secret. Le type du corps
    distingue les deux ; aucune exception nouvelle à l'en-tête du navigateur ni à
    l'origine, la signature restant la seule preuve. L'administrateur donne la même
    adresse à « Interactivity & Shortcuts », ce que disent le formulaire et la carte de
    la passerelle ;
  - **seuls les boutons de l'application sont lus** (`approval_choice`) : un seul
    bouton pressé, `approval_approve` ou `approval_reject`, sa valeur un identifiant de
    demande, l'espace de travail, la personne, la conversation et le message nommés
    comme Slack les nomme. Rien n'est décidé pour un autre espace de travail ni pour
    une passerelle en pause ;
  - **la décision est celle de la personne**, prise en arrière-plan (Slack attend sa
    réponse en trois secondes) : l'utilisateur Slack doit être relié à son compte
    (sinon c'est dit, et un lien est offert), sans mot de passe provisoire, et Slack est
    interrogé : s'il ne dit plus que c'est elle, l'identité est déliée et auditée, comme
    pour un message (ADR-127). Puis la décision passe par le même service que l'écran
    « Validations » (`RunService::decide`) : seul celui que la demande attend décide,
    `agent:use` est exigé, le run reprend avec les droits de la personne servie tels
    qu'ils sont. Elle est auditée comme toute décision, avec son service
    (`metadata.gateway = "slack"`). Un refus de la décision (demande déjà traitée,
    expirée, introuvable, attendue par un responsable) est dit dans le fil ;
  - **le message montre l'issue** à la place des boutons (« Validée par vous dans
    Slack », « Refusée… », et que la demande reprend), réécrit par `chat.update` à
    partir des blocs que Slack renvoie avec le bouton, signés avec lui. La suite du run
    vient dans le fil, comme toute fin (ADR-127) ;
  - aucune permission Slack nouvelle : `chat:write` suffit pour réécrire le message.
- **Conséquences** :
  - la description d'une action à valider est écrite dans Slack, comme l'est la
    réponse : la personne a choisi d'y parler à l'agent. Aucune donnée nouvelle n'est
    gardée ;
  - un bouton pressé deux fois, ou dans un message ancien, ne décide rien de plus : la
    demande est déjà traitée, et c'est dit ;
  - le Slack simulé des tests garde les blocs de chaque message et leurs réécritures
    (`chat.update`), aussi listés pour les tests de bout en bout ;
  - tests : `crates/core/src/gateways/slack/interactions.rs` (bouton lu, tout le reste
    écarté, description échappée et bornée, issue à la place des boutons),
    `services/gateways/reply_text.rs` (pause avec ses boutons, plusieurs, au-delà de dix,
    attendue par un responsable ou déjà décidée), `crates/api/tests/slack_gateway_validations.rs`
    (action validée par son bouton, auditée avec son service, message réécrit, suite
    dans le fil, bouton pressé deux fois ; action refusée ; sans signature, d'un autre
    espace de travail, passerelle en pause, autre personne reliée, utilisateur Slack non
    relié, utilisateur qui n'est plus la personne : rien de décidé ; action d'un
    responsable sans bouton, et son auteur ne peut que la retirer),
    `apps/web/e2e/slack-gateway.spec.ts` (la carte dit l'adresse à donner à
    « Interactivity & Shortcuts ») ;
  - reste à venir : le responsable prévenu dans Slack d'une action à valider, Telegram,
    Teams.
