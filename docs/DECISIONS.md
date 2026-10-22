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
    promotion à la fois, une plus récente remplaçant celle qui attend. Ces noms ne
    reviennent donc jamais à un commit plus ancien (une CI relancée, une promotion en
    retard) ;
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
    publication vérifie que ses images se téléchargent sans compte : un paquet resté
    privé fait échouer le travail ;
  - restent à faire : la signature des images ; arm64 ; l'épinglage des actions par
    empreinte. Le cache du travail de CI qui vérifie les piles n'est pas retenu : ce
    travail dure moins que celui de Rust. Une PR qui touche les images en vérifie la
    construction, pas l'envoi.

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

- **Date** : 2026-10-20 · **Statut** : acceptée
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

- **Date** : 2026-10-22 · **Statut** : acceptée
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
    déclare dans « Validations » (ADR-073) ; reste une absence déclarée par
    l'administrateur pour un collaborateur empêché ;
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
  - reste une absence déclarée par l'administrateur pour un collaborateur empêché ;
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
    parti, a pu sortir). Un texte que le modèle refuse (invalide ou trop long : 400,
    dépassement de contexte, 413, 422) alors que d'autres de son lot reçoivent leur vecteur
    n'est plus demandé (`embedding_model` posé, sans vecteur) ; un lot refusé en entier
    est une panne du modèle ou de son réglage. Toute panne est reprise au passage suivant
    et montrée aux administrateurs jusque-là. Seules les entreprises actives qui ont choisi un modèle sont parcourues
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
      ou d'autres dimensions refusé, texte trop grand distingué d'une panne (413, 422,
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
    - `crates/api/tests/memory_indexing.rs` : texte invalide ou trop long non redemandé ;
      lot refusé en entier montré comme panne, rien de marqué ; autre modèle, nouvelle
      indexation ; retour aux mots, vecteurs effacés, y compris pendant un passage (lot en
      vol jeté, aucun autre envoyé) ; un passage n'envoie rien quand le choix a changé ;
      indexation en arrière-plan ;
    - `crates/db/tests/definer_functions.rs` : la nouvelle fonction est dans la liste
      connue.
