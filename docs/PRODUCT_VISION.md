# Vision produit

> Nom de code provisoire : **Mimorya**. Le nom commercial sera défini ultérieurement ;
> le nom de code n'apparaît que dans les identifiants techniques (`@mimorya/*`) et peut
> être renommé sans impact fonctionnel.

## 1. Reformulation

Nous construisons une **plateforme SaaS de collaborateur IA d'entreprise**. Ce n'est
pas un chatbot branché sur un LLM : c'est un **agent qui travaille** au sein d'une
organisation réelle. Il :

- **comprend l'organisation** : entreprise, établissements, services, équipes, rôles
  et responsabilités de chacun ;
- **sert plusieurs personnes à la fois**, en respectant les **permissions effectives**
  de la personne pour laquelle il agit, quel que soit le canal (web, messagerie, e-mail) ;
- **agit** sur les outils de l'entreprise grâce à des connecteurs (API, MCP, webhooks),
  en planifiant, en déléguant à des sous-agents spécialisés et en **vérifiant** ses
  résultats avant de les livrer ;
- **retient** ce qui est utile et autorisé (mémoire personnelle, d'équipe, d'entreprise,
  d'expérience) et **s'améliore** à partir d'expériences validées (compétences
  versionnées, évaluées, réversibles) ;
- **prend des initiatives encadrées** selon un niveau d'autonomie choisi par
  l'entreprise, avec validation humaine pour tout ce qui est sensible.

L'utilisateur parle à **un seul collaborateur** ; l'orchestration interne (sous-agents,
outils, vérifications) lui est transparente mais **toujours consultable**.

## 2. Objectifs produit

| #   | Objectif                                                    | Indicateur vérifiable                                                                                            |
| --- | ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| O1  | Produire des résultats concrets, pas seulement des réponses | Part des missions terminées avec livrable validé par l'utilisateur                                               |
| O2  | Ne jamais divulguer une donnée hors du périmètre autorisé   | 0 fuite inter-entreprise / inter-utilisateur en tests d'isolation et en production                               |
| O3  | Garder l'humain maître des actions sensibles                | 100 % des actions à impact externe ou irréversible passent par une validation (hors liste d'autonomie explicite) |
| O4  | Devenir plus compétent avec le temps                        | Taux de réussite des évaluations de compétences en hausse, sans régression                                       |
| O5  | Être maîtrisé économiquement                                | Consommation (tokens, outils, stockage) suivie par entreprise, quotas appliqués                                  |
| O6  | Être adopté par des non-techniciens                         | Prise en main sans formation pour les usages courants (chat, documents, suivi)                                   |

## 3. Personas

- **Collaborateur·rice** (ex. technicien) : prépare ses comptes rendus, retrouve une
  procédure, délègue des tâches administratives.
- **Responsable d'équipe / de service** : synthèses de l'activité de son périmètre,
  planification, suivi des points bloquants, validations.
- **Direction** : analyses transverses dans les limites de ses habilitations.
- **Administrateur technique** : configure l'espace (comptes, rôles, connecteurs,
  compétences, politiques). **N'accède pas aux données métier par défaut.**
- **Référent conformité / DPO** : journal d'audit, rétention, export et suppression.

## 4. Principes directeurs

1. **Le collaborateur, pas l'outil** : l'expérience est conversationnelle et orientée
   missions ; l'utilisateur confie un résultat, pas une suite de commandes.
2. **Les permissions sont calculées côté serveur, jamais par le modèle.** Le LLM propose,
   le moteur d'autorisation dispose. Un titre hiérarchique n'est jamais une autorisation.
3. **Autonomie graduée et révocable** : lecture → proposition → exécution validée →
   exécution autonome limitée à une liste explicite.
4. **Apprendre de l'expérience validée** : on capitalise sur ce qui a été confirmé
   (par un test, une vérification ou un humain), pas sur tout ce qui a été dit.
5. **Traçabilité totale** : chaque action, décision d'accès, validation et source est
   journalisée et consultable.
6. **L'entreprise garde la maîtrise** de ses données, de ses compétences, de ses
   modèles et de ses autorisations (export, suppression, déploiement privé possible).
7. **Honnêteté produit** : une fonction de démonstration est présentée comme telle ;
   rien n'est présenté comme opérationnel s'il ne l'est pas (voir statut dans
   [ROADMAP.md](ROADMAP.md)).

## 5. Ce que le produit n'est pas (au moins au départ)

- Un outil d'entraînement de modèles : l'amélioration passe d'abord par la mémoire,
  les procédures et les compétences (voir [AGENT_ENGINE.md](AGENT_ENGINE.md) §8).
- Un remplaçant des logiciels métiers (ERP, CRM…) : il s'y connecte.
- Un agent « tout-puissant » sur l'ordinateur d'un utilisateur : les actions passent
  par des connecteurs gouvernés, pas par un shell non contrôlé.

## 6. Cible et modèle de déploiement

- **Cible initiale** : TPE/PME (10 à 250 collaborateurs), organisations de services
  (maintenance, BTP, conseil, services techniques…).
- **SaaS multi-entreprises** en priorité ; **installation privée** (une entreprise,
  infrastructure du client) avec le même code.
- **Multi-modèles** : Anthropic, OpenAI, Gemini, OpenRouter, modèles locaux ; choix par
  type de tâche, coût, latence et exigence de confidentialité.

## 7. Cas d'usage de référence

PME de services techniques (données fictives « Néo Maintenance ») : direction,
responsable technique, deux équipes de techniciens, service administratif.

> « Analyse les comptes rendus des interventions de cette semaine, identifie les
> interventions non terminées, prépare une synthèse et propose un planning de suivi pour
> mon équipe. »

Le déroulé attendu (vérification des permissions, collecte via connecteur, analyse,
détection des incohérences, synthèse, planning proposé, vérification, validation,
capitalisation) est détaillé dans [AGENT_ENGINE.md](AGENT_ENGINE.md) §10 et sert de test
d'acceptation du MVP ([ROADMAP.md](ROADMAP.md)).

## 8. Glossaire

| Terme                   | Définition                                                                                                                 |
| ----------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Entreprise (_tenant_)   | Client isolé de la plateforme ; aucune donnée n'est partagée entre entreprises.                                            |
| Unité organisationnelle | Nœud de l'organigramme : entreprise, établissement, direction, service, équipe. Profondeur illimitée.                      |
| Rôle                    | Ensemble nommé de permissions (`domaine:action`).                                                                          |
| Affectation de rôle     | Attribution d'un rôle à une personne **sur un périmètre** : elle-même, une unité (avec ou sans sous-unités), l'entreprise. |
| Permissions effectives  | Ce qu'une personne peut réellement faire, après RBAC, politiques ABAC et plafond de l'agent.                               |
| Agent principal         | Le collaborateur IA unique avec lequel l'utilisateur interagit.                                                            |
| Sous-agent              | Agent spécialisé lancé par l'agent principal, avec un périmètre restreint.                                                 |
| Exécution (_run_)       | Une boucle de travail de l'agent (un tour de conversation ou une mission).                                                 |
| Mission                 | Travail confié, suivi dans la durée, pouvant comporter plusieurs exécutions.                                               |
| Outil                   | Capacité appelable par l'agent (lecture, brouillon, écriture, action externe…).                                            |
| Connecteur              | Module qui fournit des outils vers un système (démo, MCP, API…).                                                           |
| Compétence (_skill_)    | Procédure réutilisable versionnée (format `SKILL.md`).                                                                     |
| Mémoire                 | Connaissances conservées, rattachées à un espace (personnel, unité, entreprise, expérience).                               |
| Validation (_approval_) | Demande d'accord humain avant une action sensible.                                                                         |
| Niveau d'autonomie      | Ce que l'agent peut faire seul : lecture, proposition, exécution validée, autonomie limitée.                               |
