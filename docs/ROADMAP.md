# Feuille de route

Légende des statuts — tenue à jour à chaque contribution :

| Statut | Signification                                                              |
| ------ | -------------------------------------------------------------------------- |
| ✅     | Opérationnel et couvert par des tests automatisés                          |
| 🧪     | Démonstration : fonctionne sur des données fictives ou un périmètre réduit |
| 🟡     | Partiel : socle en place, fonctionnalité incomplète (détail indiqué)       |
| 🔜     | Planifié, non implémenté                                                   |

Dernière mise à jour : 2026-10-18 (fondations du MVP).

## Phase 1 — Fondations et MVP fonctionnel

| Brique                                               | Statut | Détail                                                                                                                                                                                                                                                   |
| ---------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Monorepo, outillage, CI                              | ✅     | pnpm, TypeScript strict, ESLint, Prettier, Vitest, GitHub Actions (PostgreSQL + pgvector)                                                                                                                                                                |
| Interface web                                        | ✅     | Next.js : connexion, conversations avec suivi en direct (SSE), validations, opérations, mémoire, administration (lecture) ; responsive                                                                                                                   |
| Authentification multi-utilisateurs                  | ✅     | E-mail + mot de passe Argon2id, sessions opaques révocables, anti-CSRF, limitation de débit                                                                                                                                                              |
| Entreprise, établissements, services, collaborateurs | 🟡     | Modèle, profondeur illimitée, affichage ; **création/modification via l'interface à faire** (seed uniquement)                                                                                                                                            |
| Permissions effectives RBAC + ABAC                   | 🟡     | Moteur, stockage, application côté API et outils ✅ ; **édition des rôles, affectations et politiques à faire**                                                                                                                                          |
| Isolation multi-entreprises                          | ✅     | RLS sur toutes les tables, rôle applicatif sans `BYPASSRLS`, tests d'isolation                                                                                                                                                                           |
| Agent conversationnel, fournisseur configurable      | 🟡     | Anthropic (natif) et OpenAI-compatibles (OpenAI, OpenRouter, Gemini, Ollama…) ✅ ; configuration **au niveau du serveur** (variables d'environnement), pas encore par entreprise ; **non testé avec un modèle réel dans cet environnement (pas de clé)** |
| Moteur de tâches agentiques et outils                | ✅     | Boucle bornée (étapes, tokens, coût, durée), PEP sur chaque outil, niveaux d'autonomie, validations humaines avec reprise                                                                                                                                |
| Exécutions longues / reprise après incident          | 🟡     | État persisté à chaque étape et reprise après validation ✅ ; reprise après arrêt du processus 🔜 (les runs bloqués sont marqués en échec) ; worker séparé 🔜                                                                                            |
| Boucle de vérification et correction                 | ✅     | Contrôle d'ancrage des identifiants cités, revue par un modèle sur critères (optionnelle), corrections bornées, points non vérifiés affichés                                                                                                             |
| Mémoire individuelle et partagée                     | 🟡     | Quatre espaces, visibilité pré-filtrée en SQL, recherche plein texte (français), propositions à valider, suppression ✅ ; **recherche vectorielle 🔜** (colonne pgvector prête, aucun modèle d'embedding branché)                                        |
| Compétences (skills)                                 | 🟡     | Stockage versionné, index dans les consignes, chargement à la demande (`skill_read`) ✅ ; création, évaluation, activation 🔜 (phase 2)                                                                                                                  |
| Intégration MCP contrôlée                            | 🟡     | Pont MCP (stdio, HTTP), risque `write` par défaut, association permissions/risque par l'administrateur, tests avec un serveur de démonstration ✅ ; **configuration d'un serveur MCP via l'interface et secrets chiffrés 🔜**                            |
| Connecteur de démonstration « interventions »        | 🧪     | PME fictive, comptes rendus avec incohérences, disponibilités, propositions et application de planning (état en mémoire)                                                                                                                                 |
| Journalisation et audit                              | ✅     | Journal d'audit en ajout seul, appels d'outils, événements de run rejouables, consommation par modèle                                                                                                                                                    |
| Espace d'administration minimal                      | 🟡     | Consultation : organisation, utilisateurs et habilitations, rôles, audit, consommation, connecteurs, compétences ; **actions de gestion 🔜**                                                                                                             |

### Critères d'acceptation du MVP

| #   | Critère vérifiable                                                                                                                                                         | Preuve                                                                                        | Statut |
| --- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------- | ------ |
| A1  | Aucune lecture ni écriture entre entreprises, même par requête forgée                                                                                                      | `packages/db/src/isolation.int.test.ts`                                                       | ✅     |
| A2  | Technicien, chef d'équipe, responsable, direction et administratif voient exactement leur périmètre d'interventions                                                        | `core/.../engine.test.ts`, `connectors/.../connector.test.ts`                                 | ✅     |
| A3  | La position hiérarchique n'accorde rien ; l'administrateur technique n'a aucun accès métier                                                                                | idem + `scenario.test.ts`                                                                     | ✅     |
| A4  | L'agent ne peut jamais exercer une permission non délégable, ni dépasser le plafond du run                                                                                 | `engine.test.ts`, `runtime.test.ts`                                                           | ✅     |
| A5  | Toute action `write`/`external`/`irreversible` attend une validation ; les paramètres validés sont exécutés tels quels ; seul le demandeur décide ; pas de double décision | `runtime.test.ts`, `api.int.test.ts`                                                          | ✅     |
| A6  | Un identifiant cité sans source est détecté et corrigé ; les points non vérifiés restent visibles                                                                          | `runtime.test.ts`                                                                             | ✅     |
| A7  | Une injection dans un résultat d'outil ne peut ni sortir de son enveloppe ni contourner le PEP                                                                             | `runtime.test.ts`                                                                             | ✅     |
| A8  | La mémoire personnelle n'est visible que par son propriétaire (pas par son responsable) ; la mémoire d'équipe par l'équipe ; l'utilisateur peut supprimer la sienne        | `memory.int.test.ts`, `api.int.test.ts`                                                       | ✅     |
| A9  | Chaque appel d'outil est tracé avec sa décision ; le journal d'audit ne peut être ni modifié ni effacé par l'application                                                   | `isolation.int.test.ts`, `scenario.test.ts`                                                   | ✅     |
| A10 | Les budgets arrêtent proprement une exécution                                                                                                                              | `runtime.test.ts`                                                                             | ✅     |
| A11 | Scénario de référence de bout en bout                                                                                                                                      | `connectors/src/scenario.test.ts` (modèle scénarisé) ✅ — **avec un modèle réel : à valider** | 🟡     |
| A12 | MCP : validation par défaut, association par l'administrateur, désactivation                                                                                               | `connectors/src/mcp/bridge.test.ts`                                                           | ✅     |
| A13 | Parcours web complet (connexion, demande, suivi en direct, validation, résultat, opérations, mémoire, administration)                                                      | Vérifié manuellement (Playwright, modèle scénarisé) ; **tests e2e automatisés 🔜**            | 🟡     |

### Reste à faire pour clore la phase 1 (par priorité)

1. **Validation avec un modèle réel** (clé Anthropic ou autre) sur le scénario de référence, et ajustement des consignes et descriptions d'outils.
2. **Administration en écriture** : collaborateurs (invitation, désactivation), unités, rôles, affectations, politiques ABAC — avec audit et invalidation du cache d'autorisation.
3. **Configuration LLM par entreprise** (fournisseur, modèle, clé chiffrée) et **magasin de secrets chiffrés** (AES-256-GCM, `MIMORYA_MASTER_KEY`).
4. **Configuration des connecteurs MCP** via l'interface (transport, association outil → permissions/risque, test de connexion).
5. **Tests e2e** Playwright en CI avec modèle scénarisé ; images Docker (API, web) pour l'installation privée.
6. **En-têtes de sécurité** : CSP stricte (nonces Next.js) et HSTS au niveau du reverse proxy.

## Phase 2 — Intelligence avancée

| Chantier                                                                                                         | Priorité | Dépend de                                    |
| ---------------------------------------------------------------------------------------------------------------- | -------- | -------------------------------------------- |
| SSO OIDC (Entra ID, Google, Keycloak)                                                                            | Haute    | Authentification ✅                          |
| Worker séparé, missions longues, reprise par bail, tâches planifiées (cron)                                      | Haute    | Runs persistés ✅                            |
| Validations hiérarchiques (responsable, délégation, expiration, notifications)                                   | Haute    | Validations ✅, organisation ✅              |
| Mémoire sémantique : embeddings, recherche hybride RRF, capitalisation automatique des leçons                    | Haute    | Mémoire ✅, routage modèles                  |
| Sous-agents (`delegate_task`), profils spécialisés, parallélisme borné                                           | Moyenne  | Runs persistés ✅, plafond de permissions ✅ |
| Cycle de vie des compétences : propositions par l'agent, évaluations, non-régression, activation, retour arrière | Moyenne  | Compétences 🟡                               |
| Connecteurs : e-mail, agendas Google/Microsoft 365, webhooks entrants, REST générique                            | Moyenne  | Secrets chiffrés                             |
| Passerelles Slack, Teams, Telegram (identité de canal liée au compte)                                            | Moyenne  | Connecteurs, sessions                        |
| Documents : dépôt, extraction, découpage, recherche, citations                                                   | Moyenne  | Stockage objets, mémoire sémantique          |
| Contrôle qualité avancé : critères par compétence, revue par un modèle tiers                                     | Moyenne  | Vérification ✅                              |
| Partage de connaissances entre services (politiques de partage)                                                  | Basse    | Mémoire, ABAC                                |
| Compression de contexte pour les longues conversations                                                           | Basse    | —                                            |

## Phase 3 — Industrialisation

| Chantier               | Détail                                                                                                  |
| ---------------------- | ------------------------------------------------------------------------------------------------------- |
| SaaS multi-entreprises | Inscription, provisionnement, paramétrage initial guidé                                                 |
| Montée en charge       | Diffusion d'événements via `LISTEN/NOTIFY` puis Redis, workers horizontaux, cache                       |
| Facturation et quotas  | Quotas par entreprise (tokens, coût, stockage), alertes, intégration de paiement                        |
| Exploitation           | OpenTelemetry (traces), métriques, tableaux de bord, alertes de coût                                    |
| Déploiements privés    | Images signées, docker-compose et Helm, guide d'exploitation                                            |
| Sauvegarde et reprise  | Sauvegardes chiffrées, restauration testée, plan de reprise                                             |
| Mises à jour           | Migrations sans interruption, versionnage de l'API                                                      |
| Conformité             | Audit chaîné, export SIEM, SCIM, clés étrangères composites `(tenant_id, id)`, registre des traitements |
| Internationalisation   | Interface multilingue                                                                                   |
