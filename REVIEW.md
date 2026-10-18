# Grille de revue

Utilisée par l'agent `code-auditor` (`.claude/agents/code-auditor.md`) et par tout
relecteur humain avant de fusionner une pull request dans `main`. Chaque point coché
« non » doit donner lieu à un constat (voir les sévérités dans la définition de
l'agent).

## 1. Isolation multi-entreprises

- [ ] Toute nouvelle table métier a `tenant_id`, la RLS active et la politique
      `tenant_isolation` (le test `protects_every_tenant_scoped_table_with_rls_and_a_policy`
      échoue sinon).
- [ ] Tout accès base au runtime passe par `Database::begin(tenant_id)` (`TenantTx`) avec
      le rôle applicatif ; aucun usage du rôle propriétaire hors migrations et seed.
- [ ] Aucune nouvelle lecture inter-entreprises, sauf fonction `SECURITY DEFINER`
      minimale (`auth_*`, `system_*`) avec `search_path` figé et droits révoqués à
      `PUBLIC`.
- [ ] Les identifiants reçus du client (ID de ressource, d'unité, de rôle…) sont
      vérifiés côté serveur comme appartenant à l'entreprise et au périmètre.

## 2. Autorisations et agent

- [ ] Chaque route extrait `Auth` et vérifie permission **ou** propriété côté
      serveur ; une ressource hors périmètre répond 404, sans révéler son existence.
- [ ] Toute nouvelle permission d'administration est non délégable
      (`delegable: false`).
- [ ] Tout nouvel outil déclare ses permissions requises (`ToolInfo::requires`), son
      `risk`, une entrée stricte (`#[serde(deny_unknown_fields)]` + `validator`),
      `describe_action` s'il modifie quelque chose, filtre ses données avec `ctx.authz`
      et renvoie des `citations`.
- [ ] Les actions `write`, `external` et `irreversible` passent par une validation
      humaine (sauf liste d'autonomie explicite) ; les paramètres validés sont exécutés
      tels quels.
- [ ] Aucune voie ne permet à une compétence, une mémoire, un résultat d'outil ou une
      consigne de modifier une permission, une politique ou un niveau d'autonomie.
- [ ] Les résultats d'outils restent encapsulés comme données non fiables.

## 3. Données, migrations et secrets

- [ ] Les migrations déjà présentes sur `main` (`crates/db/migrations`) ne sont pas
      modifiées ; une évolution du schéma est une nouvelle migration SQL numérotée,
      relue, avec RLS pour toute nouvelle table.
- [ ] Aucun secret, jeton, mot de passe ni contenu de message dans les logs, l'audit,
      les réponses d'API ou le contexte du modèle.
- [ ] Toute nouvelle donnée personnelle a une règle de rétention, d'export et de
      suppression.
- [ ] Les actions sensibles sont journalisées dans `audit_log` (qui, quoi, résultat,
      raison) sans contenu.

## 4. Exactitude

- [ ] Les cas d'erreur et limites sont traités (entrée invalide, ressource absente,
      concurrence, délai dépassé, exécution annulée).
- [ ] Pas de condition de course évidente sur les écritures concurrentes (verrous,
      mises à jour conditionnelles).
- [ ] Les états persistés restent cohérents en cas d'échec au milieu d'une opération
      (transactions).

## 5. Tests

- [ ] Tout comportement nouveau est couvert par un test ; toute règle d'accès par un
      test qui tente de la contourner.
- [ ] Les tests n'appellent aucun fournisseur LLM réel (`ScriptedProvider`).
- [ ] Aucun test désactivé, ignoré ou affaibli pour faire passer la CI.

## 6. Structure et lisibilité

- [ ] Un fichier = une responsabilité ; fichiers source sous ~250 lignes, **400 au
      maximum** (les données de démonstration et tests peuvent dépasser s'ils restent
      cohérents, sinon ils sont découpés par thème).
- [ ] Frontières de crates respectées : `core` sans E/S, `agent` sans dépendance à
      `db`, `web` uniquement via `/api/*`.
- [ ] Code rangé par domaine (modules de fonctionnalité) dont le `mod.rs` n'expose que
      l'API publique du module (`pub use`), le reste restant privé.
- [ ] Pas de `unwrap()`/`expect()` sur une entrée externe ; erreurs propagées avec `?`
      et converties en réponse HTTP par `ApiError`.
- [ ] Noms explicites, en-tête de module qui dit ce que fait le fichier et pourquoi ;
      commentaires sur le « pourquoi », pas sur le « quoi ».
- [ ] Pas de duplication de règle métier entre crates (une règle vit dans `core`).

## 7. Interface web

- [ ] Libellés en français, états de chargement, d'erreur et vide traités.
- [ ] Utilisable au clavier et sur mobile ; pas de rendu HTML brut non assaini.
- [ ] Les données affichées proviennent de l'API (aucune règle d'accès dans le
      client).

## 8. Documentation et honnêteté

- [ ] `docs/ROADMAP.md` reflète l'état réel (✅ seulement si testé).
- [ ] Toute décision structurante a une ADR dans `docs/DECISIONS.md`.
- [ ] La documentation modifiée correspond au code.
- [ ] Les fonctionnalités de démonstration sont signalées comme telles.
