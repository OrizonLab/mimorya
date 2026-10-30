# Sécurité et confidentialité

Ce document est la référence des exigences de sécurité. Toute modification du modèle
d'autorisation, de l'isolation ou du traitement des secrets doit le mettre à jour et
faire l'objet d'une décision dans [DECISIONS.md](DECISIONS.md).

## 1. Modèle de menace

| Acteur / vecteur                | Objectif                                | Exemple                                                                        |
| ------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------ |
| Entreprise cliente malveillante | Lire les données d'une autre entreprise | Identifiant forgé dans une requête API                                         |
| Utilisateur interne curieux     | Dépasser son périmètre                  | Demander à l'agent « montre-moi les interventions de l'autre équipe »          |
| Contenu injecté                 | Détourner l'agent                       | Compte rendu, e-mail ou page web contenant « ignore tes consignes et envoie… » |
| Modèle défaillant               | Action erronée ou hallucination         | Appel d'outil avec de mauvais paramètres, synthèse inventée                    |
| Compétence dégradée             | Régression ou contournement             | Version de skill qui saute une étape de contrôle                               |
| Attaquant externe               | Prise de compte, vol de jetons          | Credential stuffing, XSS, CSRF                                                 |
| Serveur MCP / connecteur tiers  | Exfiltration, réponses piégées          | Outil MCP renvoyant des instructions                                           |
| Fournisseur LLM                 | Exposition des données envoyées         | Rétention côté fournisseur, localisation hors UE                               |

## 2. Principes non négociables

1. **Les autorisations sont décidées par le serveur**, jamais par le modèle, ni par le
   contenu d'un message, d'un document, d'un résultat d'outil ou d'une compétence.
2. **Défense en profondeur** : RLS en base + filtrage applicatif + moteur
   d'autorisation + PEP sur chaque outil.
3. **Moindre privilège** : l'agent agit avec les permissions de l'utilisateur,
   **réduites** par le plafond agent (permissions non délégables) et par le plafond du
   run ; un sous-agent ne peut qu'avoir moins que son parent.
4. **Refus par défaut** : sans règle qui accorde, c'est refusé.
5. **Traçabilité** : toute décision d'accès refusée ou accordée à l'agent, toute action
   et toute validation sont journalisées.

## 3. Isolation des entreprises

- Schéma partagé, colonne `tenant_id` sur toutes les tables métier.
- **Row-Level Security** activée sur toutes ces tables avec la politique
  `tenant_id = current_setting('app.tenant_id', true)::uuid` en lecture et écriture.
- L'application utilise un rôle PostgreSQL **non propriétaire, sans `BYPASSRLS`**.
  Les migrations utilisent un rôle distinct.
- Le contexte `app.tenant_id` est posé **par transaction** (`set_config(..., true)`),
  jamais au niveau de la connexion (pas de fuite par le pool).
- Les seules opérations qui traversent les entreprises (connexion, résolution de
  session, fournisseur d'identité proposé pour une adresse avant connexion (ADR-063),
  lecture d'une invitation ou d'un lien de réinitialisation avant connexion,
  recherche du compte d'une adresse par la commande d'exploitation `invite` (ADR-033),
  marquage des runs bloqués, liste des runs en cours dont le bail a expiré ou n'a
  jamais été pris, entreprise et identifiant seulement (ADR-053, ADR-083, ADR-086), liste des runs dont une demande de
  validation a passé son échéance, entreprise et identifiant seulement, chacun traité
  ensuite dans sa propre entreprise (ADR-061), de même pour les runs dont une demande
  est à relancer la veille de son échéance (ADR-062) ou dont l'avis a été perdu par un
  arrêt du service (ADR-070), purge des liens de réinitialisation
  expirés et des absences terminées (ADR-071)) passent par
  des fonctions `SECURITY DEFINER` au périmètre minimal et au `search_path` figé
  (`auth_*`, `system_*`).
- Le canal PostgreSQL des événements des runs (`mimorya_run_events`, ADR-084) est commun
  à toutes les entreprises, et PostgreSQL n'y contrôle aucun privilège. Il ne porte que
  des identifiants de runs et des numéros d'événements, que chaque processus relit dans
  le journal du run, dans l'entreprise de l'abonné dont le flux a vérifié que le run lui
  appartient, et les fragments de texte du modèle, remis aux seuls abonnés de ce run.
  Les canaux de la file des runs (`mimorya_run_queue`, sans contenu) et des invalidations
  de caches (`mimorya_invalidations` : une sorte et un identifiant d'entreprise ou
  d'instance de connecteur, ADR-085) ne portent rien d'autre : une annonce ne fait que
  vider un cache, relu ensuite dans l'entreprise concernée. Aucun autre rôle que
  l'applicatif et le propriétaire ne se connecte à la base.
- Tests d'intégration obligatoires : lecture croisée impossible même par requête
  directe, insertion croisée refusée, absence de contexte = aucune ligne.

## 4. Authentification et sessions

- MVP : e-mail + mot de passe haché **Argon2id** ; messages d'erreur identiques
  (compte inconnu / mauvais mot de passe), vérification contre une empreinte factice
  pour un compte inconnu et journalisation des échecs hors du chemin de réponse : la
  durée de réponse ne révèle pas l'existence d'un compte.
- Jetons de session **opaques** (256 bits aléatoires), seule leur empreinte SHA-256 est
  stockée ; cookie `HttpOnly`, `Secure` (hors développement), `SameSite=Lax` ; durée
  glissante (défaut 12 h), absolue (7 jours), révocation à la déconnexion et à la
  désactivation du compte.
- Mot de passe oublié (ADR-028) : lien de réinitialisation à usage unique, valable une
  heure, envoyé **seulement par e-mail** (sans relais SMTP, la demande est refusée et
  renvoie vers l'administrateur) ; jeton de 256 bits dont seule l'empreinte est
  conservée, jamais montré, journalisé ni audité. La demande reçoit toujours la même
  réponse et le lien est préparé hors du chemin de réponse : rien ne révèle si
  l'adresse correspond à un compte. Seul un compte actif qui a un mot de passe reçoit
  un lien ; une nouvelle demande remplace le précédent ; tout changement de mot de
  passe ou de statut du compte le supprime (déclencheur en base) ; l'usage ferme
  toutes les sessions du compte. Demandes limitées à 3 par heure et par adresse
  e-mail, connue ou non, en plus de la limite par client. Contrepartie assumée
  (ADR-029) : un tiers peut épuiser ces 3 demandes et faire refuser, pendant le reste
  de l'heure, la vraie demande de la personne ; l'administrateur peut alors lui
  envoyer un lien. La page de connexion ne propose le lien que si un relais est
  configuré ; l'audit d'une demande, anonyme, est inscrit au nom de la plateforme.
- Protection CSRF : `SameSite=Lax` + exigence d'un en-tête personnalisé sur toutes les
  requêtes modifiantes + vérification d'`Origin`. Quatre exceptions, que des tests tentent
  de contourner : la réception des réponses SAML (ADR-068, ci-dessous), l'envoi d'un
  autre système à un déclencheur (`POST /hooks/{id}`, chemin exact, identifiant en forme
  canonique, ADR-109), qu'aucune session n'autorise : son jeton seul le fait, les
  événements que Slack envoie à la passerelle d'une entreprise (`POST
/gateways/slack/{id}`, chemin exact, identifiant en forme canonique, ADR-125), que
  leur signature seule prouve, et les mises à jour que Telegram envoie au bot d'une
  entreprise (`POST /gateways/telegram/{id}`, chemin exact, ADR-133), que le secret de
  son webhook seul prouve.
- Passerelle Slack (ADR-125) : une requête de Slack nomme sa passerelle, puis prouve
  qu'elle vient de l'application de l'entreprise par sa signature (HMAC-SHA256 du corps
  et de l'horodatage avec le secret de signature scellé, comparée en temps constant, à
  cinq minutes près) ; une passerelle inconnue, une signature absente, fausse ou trop
  ancienne, et une passerelle dont le serveur ne peut ouvrir le secret répondent de même
  (404). Corps borné (256 Kio) ; un événement renvoyé par
  Slack est pris une fois ; un événement d'un autre espace de travail, ou qui n'est pas
  le message direct d'une personne, est ignoré. Ce que dit un message n'est ni
  journalisé ni audité. Une identité Slack n'est reliée que depuis la session de la
  personne, par le lien scellé (quinze minutes, rien d'enregistré) que l'application lui
  envoie, son code dans le fragment de l'adresse, jamais envoyé avec la page ni écrit
  dans les journaux d'un proxy, remis par la page dans le corps d'une requête
  (ADR-126), et seulement si Slack, interrogé alors, dit que l'utilisateur est de l'espace
  de travail de l'entreprise, ni désactivé ni bot, et a l'adresse du compte : un lien
  ouvert par quelqu'un d'autre ne relie rien. Refus audités avec leur raison, jamais une
  adresse ; identité retirée avec le compte désactivé ou la passerelle supprimée.
- Conversations dans Slack (ADR-127) : le message d'une personne reliée est sa propre
  demande, exécutée avec ses droits tels qu'ils sont, comme dans l'application (canal
  `slack`, que les politiques d'accès peuvent lire : `context.channel`), refusée pour un
  compte désactivé, au mot de passe provisoire, sans `agent:use` ou d'une entreprise
  suspendue. Slack est interrogé à chaque message : s'il ne dit plus que l'utilisateur
  est la personne (adresse changée, désactivé, autre espace de travail), l'identité est
  déliée, auditée avec sa raison, et rien n'est lancé ; s'il ne répond pas, rien n'est
  lancé. Les réponses ne vont que dans le fil de la conversation, tant que la passerelle
  est active et que l'utilisateur Slack qui a écrit reste l'identité de la personne ; ce
  qui y est écrit est échappé pour Slack (ni mention, ni commande, ni lien que ceux de la
  réponse), et ni journalisé ni enregistré hors de la conversation.
- Validations dans Slack (ADR-129) : seule une action de la personne elle-même qui ne
  sort rien de l'entreprise et peut être défaite (lecture, brouillon, modification),
  dont les paramètres exacts tiennent entiers dans un message, s'y montre avec ses
  paramètres exacts, son risque et deux boutons ; un e-mail envoyé en son nom, une
  action irréversible ou aux paramètres trop longs se valide dans l'application. Slack
  envoie le bouton pressé à la même adresse, signé comme ses événements (même secret,
  cinq minutes), sans nouvelle exception à l'en-tête du navigateur. Rien n'est décidé
  pour un autre espace de travail, une passerelle en pause, un utilisateur Slack non
  relié ou qui n'est plus la personne (Slack interrogé, identité déliée et auditée), un
  mot de passe provisoire ou une entreprise suspendue ; quoi que dise le bouton, seule la
  propre action de la personne, qui attend sa propre validation et se valide dans
  Slack, y est décidée, par le même contrôle que l'écran (`agent:use` exigé, les droits
  de la personne servie relus), et auditée avec son service (`gateway: slack`). Qui
  détient le secret de signature peut forger ces requêtes : il ne peut valider ainsi
  que ces actions-là, jamais un envoi hors de l'entreprise ni une action irréversible.
- Conversation directe vérifiée (ADR-131) : la signature prouve que Slack a envoyé
  l'événement ou le bouton, pas que la conversation qu'il nomme est celle de
  l'utilisateur Slack qu'il nomme avec l'application. Qui détiendrait le secret de
  signature pourrait sinon faire écrire ce qui est destiné à une personne là où une
  autre le lit (sa réponse, un lien de liaison, un refus). Slack est donc interrogé
  (`conversations.info`, permission `im:read`) avant que rien n'y soit dit ni lancé :
  une conversation directe, et celle de cet utilisateur. Celle d'un autre, ou une que
  l'application ne voit pas (`channel_not_found`) : rien n'y est dit ni lancé, même si
  l'audit échoue, et c'est audité (`gateway.conversation_refused`, sans contenu). Slack
  ne le disant pas : rien n'est lancé, et l'auteur est prévenu dans sa propre conversation,
  adressée par son identifiant, jamais dans celle que l'événement nomme. Le lien de
  liaison part toujours ainsi.
- Permissions de l'application Slack (ADR-132) : celles que Slack dit du jeton
  (`x-oauth-scopes`) sont lues à sa déclaration et à chaque nouveau jeton ; un jeton
  auquel manque une permission demandée est refusé, rien n'est gardé. Gardées avec la
  passerelle, elles se vérifient à la demande avec le jeton enregistré (audité, jamais le
  jeton) ; la carte montre celles qui manquent et celles en plus de celles demandées, à
  retirer pour que la plateforme ne garde pas un jeton plus fort qu'il ne lui faut.
- Bot Telegram (ADR-133) : déclaré par l'entreprise (`connector:manage`, un par
  entreprise), son jeton essayé auprès de Telegram ; Telegram reçoit l'ordre d'envoyer
  ses mises à jour à l'adresse de la passerelle avec un secret de 64 caractères que fait
  la plateforme, sous le verrou de l'administration, un refus ne gardant rien. Jeton et
  secret scellés, jamais rendus ni audités ; le jeton fait partie de l'adresse des
  méthodes de Telegram, que rien n'écrit jamais. Une mise à jour n'est lue qu'avec ce
  secret (comparé par empreinte), sinon répondue comme une passerelle inconnue ; bornée,
  prise une fois, seul le message privé d'une personne est lu et la réponse va dans sa
  conversation privée. Telegram ne prouvant aucune adresse, un compte se relie depuis la
  session de la personne : un code de 256 bits qu'elle demande (gardé par son empreinte,
  quinze minutes), renvoyé par Telegram depuis la conversation privée de qui ouvre le
  lien, à qui le bot nomme le compte qu'il relierait et qui l'accepte (`/oui`, la seule
  offre qui l'attend : la dernière nommée), puis
  confirmé dans cette même session, qui voit le nom et l'identifiant Telegram (un nom
  libre ne passe jamais pour un nom d'utilisateur ni ne cache de caractère). Chaque côté
  consent donc en sachant à qui : un code vu par un autre ne relie rien sans la session ;
  un lien qu'un collègue fait ouvrir ne relie le compte Telegram de personne au sien sans
  son accord donné en connaissance de cause ; une liaison n'est jamais reprise par le
  code d'un autre, seul son titulaire la retire. Codes oubliés à expiration ; identités
  et codes tenus par la base au service de leur passerelle.
- Conversations dans Telegram (ADR-135) : les mots d'une personne reliée sont lancés en
  son nom, avec ses droits tels qu'ils sont, sur le canal `telegram` que les politiques
  lisent, une fois vérifiés sa liaison, son mot de passe remplacé, son entreprise
  active et `agent:use` (règles partagées avec Slack). Telegram atteste qui écrit (le
  secret du webhook, la conversation privée de son identifiant) ; un compte Telegram
  relié depuis à quelqu'un d'autre ne poursuit jamais la conversation de la personne
  d'avant, et la réponse d'un run ne va dans la conversation privée que tant que ce
  compte Telegram est encore celui de la personne et le bot actif. Ce qui y est écrit
  est en HTML que la plateforme produit, le texte de l'agent échappé, seules des balises
  de mise en forme et des liens web refermés ; jamais ses mots dans les journaux ni
  l'audit. Les validations se donnent dans l'application, qui en montre les paramètres
  exacts, ou avec les boutons de la pause pour celles qu'autorise ADR-136.
- Bot Teams (ADR-138) : déclaré par l'entreprise (`connector:manage`, un par
  entreprise) avec son application, l'annuaire Microsoft Entra commun au bot et aux
  personnes, et son secret client, essayé auprès d'Entra avant que rien ne soit gardé ;
  secret scellé, jamais rendu ni audité ; ni l'application ni l'annuaire ne changent.
  Une activité n'est prise qu'avec le jeton que signe le Bot Framework (RS256, clé
  publiée par Microsoft et endossée pour Teams, émetteur, audience du bot, validité à
  cinq minutes près, service de l'activité), sinon répondue comme une passerelle
  inconnue ; chemin exact dispensé de l'en-tête du navigateur. Seul le message d'une
  personne dans sa conversation personnelle avec le bot, de l'annuaire de
  l'entreprise, est lu, une fois ; le bot n'écrit qu'aux services Teams de Microsoft
  du cloud commercial (HTTPS, port par défaut, chemin simple), vérifiés à la réception
  et dans le client. Les clés du Bot Framework sont relues au plus toutes les cinq
  minutes pour une clé inconnue, une lecture à la fois que les autres attendent, sans
  connexion à la base tenue pendant ce temps ; une lecture échouée ou interrompue n'est
  retentée qu'après trente secondes : des activités forgées ne font pas demander
  Microsoft à chaque fois.
  Une identité ne se relie que depuis la session de la personne, par le lien scellé que
  le bot lui envoie (quinze minutes), quand Teams, interrogé alors dans la conversation
  où le bot l'a rencontrée, nomme le même objet Entra, de l'annuaire de l'entreprise,
  avec l'adresse du compte ; `agent:use` exigé ; refus audités sans adresse.
- Validations dans Telegram (ADR-136) : seules les actions qui attendent la personne
  elle-même, sans effet hors de l'entreprise, réversibles, aux paramètres exacts
  montrés entiers, se valident avec les boutons de la pause ; un bouton pressé n'est
  pris que dans la conversation privée de qui le presse, sous un message du bot et
  s'il est l'un des boutons de ce message tel que Telegram le garde (ce qu'un client
  prétend presser ne décide rien), puis décidé au nom de la personne reliée, comme
  depuis l'écran, audité avec le service. Toute autre action, dans l'application.
- Responsable prévenu dans Slack (ADR-130) : une demande qui attend un responsable est
  annoncée par un message direct au responsable, ou au remplaçant (ADR-071), seulement
  s'il peut encore la décider (`approval:decide` sur la personne servie, relu au
  moment d'écrire) et si son identité Slack est reliée à la passerelle active de
  l'entreprise, Slack interrogé d'abord (s'il ne dit plus que l'utilisateur est la
  personne, l'identité est déliée, auditée, et rien n'est écrit). Seules les actions qui
  se valident dans Slack (mêmes règles qu'ADR-129) y montrent leurs paramètres exacts et
  leurs boutons ; les autres, leur nombre et le lien des validations. Pressé, le bouton
  est décidé par le même contrôle que l'écran : seul celui que la demande attend, ou
  son remplaçant, la décide ; l'auteur de la demande ou un autre ne la décident pas.
  Qui détient le secret de signature peut donc aussi valider, au nom d'un responsable
  relié, ces actions-là d'un de ses collaborateurs : jamais un envoi hors de
  l'entreprise ni une action irréversible. Chaque avis est audité
  (`gateway.approval_notified`), jamais ce qui est écrit ; ce qu'il en est devenu est
  gardé avec la demande (`gateway_approval_notices`), supprimé avec elle.
- Responsable prévenu dans Telegram (ADR-137) : de même, une demande qui attend un
  responsable est annoncée dans sa conversation privée avec le bot actif de
  l'entreprise, ou dans celle du remplaçant, seulement s'il peut encore la décider
  (relu au moment d'écrire) et s'il a relié son compte Telegram au bot (le compte qu'il
  a accepté de relier en sachant à quel compte, ADR-133 ; Telegram ne prouvant aucune
  adresse, rien ne lui est demandé avant d'écrire). Mêmes actions montrées avec leurs
  paramètres exacts et leurs boutons qu'à l'auteur (ADR-136) ; pressé, le bouton passe
  les mêmes contrôles (`concerned::decide_as`) : seul celui que la demande attend, ou
  son remplaçant, la décide, quoi que prétende la pression. Qui détient le secret du
  webhook peut donc aussi valider, au nom d'un responsable relié, ces actions-là d'un
  de ses collaborateurs : jamais un envoi hors de l'entreprise ni une action
  irréversible. Chaque passerelle annonce une demande une fois, ce qu'il en advient
  gardé pour chacune ; chaque avis audité avec son service, jamais ce qui est écrit.
- Déclencheurs (ADR-109) : un autre système envoie avec le jeton secret du déclencheur
  (`Authorization: Bearer`, 256 bits, gardé en empreinte SHA-256, montré une fois). Un
  déclencheur inconnu, un jeton faux et un déclencheur en pause répondent de même (404).
  Corps borné (16 Kio, UTF-8), trente envois par heure au plus. Les données reçues
  suivent la demande de l'auteur, encadrées comme données non fiables, jamais dans
  l'audit ni les journaux ; le run agit avec les droits de l'auteur et n'agit jamais
  seul (autonomie plafonnée à « exécution avec validation »). Il porte son canal
  (`hook`) jusqu'au bout, reprises et sous-agents compris : ce qu'il retiendrait est une
  proposition à l'auteur, jamais une mémoire active, même personnelle, et les politiques
  d'accès peuvent le lire (`context.channel`). Un envoi refusé ne dit à l'expéditeur que
  son statut (409, 503) : la raison est gardée pour l'auteur seul ; au-delà de la
  limite horaire, seul le premier refus de l'heure est gardé et tracé.
- Limitation de débit sur la connexion :
  - **par compte** : 10 échecs par quart d'heure et par adresse e-mail, comptes
    inconnus compris (rien ne distingue un compte existant). Chaque tentative réserve
    sa place avant la vérification du mot de passe, rendue en cas de succès : ni des
    tentatives simultanées ni un changement d'adresse ne dépassent la limite.
    Contrepartie assumée : une fois la limite atteinte, même le bon mot de passe est
    refusé jusqu'à la fin de la fenêtre, ce qui permet de bloquer temporairement un
    compte ciblé (la MFA et l'OIDC de la phase 2 réduiront ce risque) ;
  - **par adresse cliente** (`LOGIN_RATE_LIMIT_PER_MINUTE`, 10 par défaut), en plus
    d'une limite globale de 600 requêtes par minute et par adresse. Sans reverse
    proxy (voir ci-dessous), ces limites s'appliquent à toute la plateforme, puisque
    tous les utilisateurs partagent alors une même adresse ;
  - **connexion par le fournisseur d'identité** (ADR-063) : les adresses essayées par
    la découverte ou le départ, qui disent tous deux si une adresse a un compte relié à
    un fournisseur, ont leur propre limite par adresse cliente, égale à la précédente
    (une adresse déjà comptée ne coûte rien ; seule son empreinte est gardée, jusqu'à
    la fenêtre suivante, deux heures au plus) ; le départ et le retour ensemble, deux fois la précédente. Aucune ne retire
    rien à la limite des mots de passe ; un retour rejoué au-delà est refusé sans trace
    ni appel au fournisseur.
  - Ces limites d'authentification (connexions par adresse cliente, échecs par compte,
    liens par adresse e-mail, adresses essayées et connexions par le fournisseur
    d'identité) sont partagées par tous les processus `api` (ADR-088) : comptées dans
    PostgreSQL, une tentative répartie sur plusieurs instances ne gagne rien. La base
    n'en garde qu'une empreinte (SHA-256) de chaque clé et de chaque valeur, dans des
    tables que le rôle applicatif ne lit pas (seules des fonctions `SECURITY DEFINER`
    les comptent), purgées deux heures après. Un décompte que la base ne rend pas fait
    échouer la tentative : ces limites gardent la connexion, qui lit la même base.
  - La limite globale par requête (600 par minute et par adresse) reste comptée en
    mémoire, par processus, pour ne pas écrire en base à chaque requête : avec N
    processus `api`, un client réparti sur toutes les instances en dispose N fois.
- Adresse cliente : par défaut, l'adresse TCP de l'appelant (`TRUST_PROXY=false`).
  Derrière le seul relais web, tous les utilisateurs partagent l'adresse du serveur
  Next.js, qui transmet tel quel un `X-Forwarded-For` fourni par le client. En
  production, un reverse proxy de confiance (TLS) **ajoute** l'adresse du client à
  `X-Forwarded-For` ; avec `TRUST_PROXY=true`, l'API retient alors la dernière entrée
  de cet en-tête.
- **Fournisseur d'identité de l'entreprise** (SSO OpenID Connect, ADR-063) :
  Microsoft 365 (Entra ID, un annuaire), Google Workspace (domaines déclarés, `hd`) ou
  tout fournisseur OpenID Connect, déclarés par l'administration (`settings:manage`) avec
  leurs domaines d'e-mail ; secret client dans le coffre (ADR-024), jamais relu.
  - Un fournisseur ne sert que les comptes de son entreprise : celui qui est proposé
    pour une adresse est celui de l'entreprise du compte, actif ou invité, qui couvre son
    domaine
    (même réponse pour une adresse sans compte ou d'un compte désactivé) ; déclarer le
    domaine d'une autre entreprise ne détourne aucun de ses comptes.
  - Flux par code avec PKCE, `state` et `nonce` ; la tentative voyage dans un cookie
    `HttpOnly`, `SameSite=Lax`, de 10 minutes, scellé par la clé maîtresse avec
    l'entreprise en données associées ; le retour d'une autre tentative est refusé (pas
    de connexion forcée), avant même un refus du fournisseur ; le code s'échange une
    fois. Le jeton d'identité est vérifié (signature par les clés publiées, émetteur,
    audience, expiration, nonce) ; la déclaration et les clés sont gardées une heure par
    fournisseur déclaré (jamais partagées entre entreprises), et relues une fois si la
    signature échoue avec elles ; une clé retirée par le fournisseur reste acceptée au
    plus une heure (ADR-065). Le fournisseur
    est joint par le client HTTP durci des
    connecteurs (ADR-025), borné en durée et en taille ; toute adresse qu'il nomme
    (clés, point d'échange, adresse d'autorisation) est contrôlée comme son émetteur
    avant la connexion : `https`, adresse IP en clair publique (pas de SSRF vers le
    réseau de l'exploitant).
  - La première connexion relie un compte existant au
    sujet immuable du fournisseur (`oid` pour Microsoft), seulement par une adresse que
    le fournisseur garantit (Microsoft : le nom de connexion, ou `email` avec
    `xms_edov`), dans un domaine déclaré ; ensuite, seul le sujet compte. Un compte
    invité accepte ainsi son invitation : actif sans mot de passe, son lien supprimé ;
    désactivé puis réactivé, il reste actif (ADR-064).
    Création de compte (ADR-067) : seulement si le fournisseur y est autorisé et
    seulement pour une connexion partie du lien de connexion de l'entreprise, qui nomme le
    fournisseur (origine scellée dans la tentative), jamais d'après une adresse (les
    domaines déclarés ne sont pas vérifiés et peuvent l'être par deux entreprises) ; mêmes conditions d'adresse qu'une liaison ; le compte est actif,
    sans mot de passe, sans rôle ni unité, son nom nettoyé des caractères cachés ; une
    adresse déjà prise, dans une autre entreprise comprise, n'est jamais reprise. Le type
    générique OpenID Connect refuse les émetteurs de Google et de Microsoft, dont les
    types propres vérifient le domaine Workspace ou le nom de connexion de l'annuaire.
    La session s'ouvre comme pour un mot de passe ; chaque refus est tracé avec sa
    raison dans l'entreprise de la tentative, jamais un jeton ni un code.
  - Fournisseur **obligatoire** pour ses domaines (ADR-066) : leurs comptes ne se
    connectent plus par mot de passe (refus dit seulement à qui donne le bon mot de
    passe, sans compter comme un échec), n'en choisissent pas en acceptant leur
    invitation, ne reçoivent aucun lien de réinitialisation (demandé ou envoyé par
    l'administration), ne peuvent plus utiliser un lien reçu avant et ne reçoivent pas
    de mot de passe provisoire : aucun chemin ne leur rend un mot de passe. Les
    administrateurs de la connexion de l'entreprise (`settings:manage` sur toute
    l'entreprise) gardent le leur : un fournisseur en panne ne bloque jamais
    l'entreprise. Le fournisseur d'une autre entreprise, même obligatoire pour le même
    domaine, n'atteint aucun de ses comptes.
  - **SAML 2.0** (ADR-068, `saml-rs` en Rust pur) : fournisseur déclaré par ses
    métadonnées ; assertion obligatoirement signée (RSA avec SHA-2, canonicalisation
    exclusive) par un certificat de ces métadonnées, audience et destinataire de
    l'application, réponse à la demande attendue (tentative scellée dans un cookie
    `SameSite=None; Secure` propre au retour SAML), fenêtre de validité, assertions à
    usage unique en base (rejeu refusé sur tout serveur). L'identifiant d'entité de
    l'application est propre à chaque entreprise : la confiance d'un fournisseur ne vaut
    que pour elle. Exemptée
    de l'en-tête anti-CSRF et du contrôle d'origine (avec l'envoi à un déclencheur,
    ADR-109, les événements de Slack, ADR-125, et les mises à jour de Telegram, ADR-133) : la réception de la réponse, liée
    par la tentative, la demande attendue et la signature ; un test tente de la
    contourner (`crates/api/tests/auth.rs`). Ni connexion à l'initiative du fournisseur, ni assertion chiffrée.
  - **Propriété des domaines** (ADR-069) : une entreprise prouve un domaine par un
    enregistrement DNS TXT de son propre (`_mimorya.<domaine>`). Un domaine prouvé réserve
    ses nouvelles adresses, uniques sur la plateforme, à l'entreprise qui l'a prouvé :
    aucune autre n'y crée de compte, par invitation ou à la connexion, sauf si elle l'a
    prouvé aussi. Un fournisseur OpenID Connect générique ou SAML, qui garantit les
    adresses que veut son propriétaire, ne crée de compte que dans un domaine prouvé par
    son entreprise ; Microsoft et Google vérifient eux-mêmes les domaines de leurs
    clients. Une adresse prise avant la preuve le reste (ADR-011).
  - Suite : provisionnement SCIM (phase 3), MFA déléguée à l'IdP ; MFA TOTP locale si
    besoin.

## 5. Autorisations

- **RBAC à périmètre** : rôle × personne × périmètre (`self`, unité ± sous-unités,
  entreprise). La position hiérarchique (responsable d'unité) **n'accorde rien** seule.
- **ABAC** : politiques `allow` / `deny` à conditions JSON (attributs du sujet, de la
  ressource, du contexte), `deny` prioritaire ; exemple livré : le service
  administratif ne consulte que les interventions terminées. L'administration n'écrit
  que des politiques de refus, sur des permissions métier que la plateforme vérifie
  (jamais d'administration, ni `org:read`/`user:read`), avec une condition vérifiée
  avant stockage. Un refus portant sur le collaborateur ou le contexte s'applique
  partout où la permission est vérifiée, outils de l'agent compris ; un refus portant
  sur la ressource seulement aux permissions vérifiées ressource par ressource (un
  créneau d'intervention illisible n'apparaît que comme « occupé » dans le planning).
  Une condition remplie par tous retire la permission à toute l'entreprise,
  administrateurs compris : le formulaire le signale. Les attributs des
  collaborateurs (`subject.attributes.*`) sont posés par un administrateur des comptes,
  jamais sur son propre compte. Ils sont typés et courts. L'audit nomme les attributs
  modifiés, jamais leurs valeurs (ADR-023). Les
  politiques d'autorisation, qui pourraient ouvrir les données métier à leur auteur,
  restent en lecture seule jusqu'à la double validation (ADR-022).
- **Sensibilité** : `public < internal < confidential < restricted`, comparée à
  l'habilitation (`clearance`) de la personne ; le propriétaire voit toujours ses
  propres données.
- **Requêtes de liste** : pré-filtrage par le périmètre RBAC (`dataScope`) puis contrôle
  unitaire ABAC (`check`) — jamais l'inverse.
- **Permissions non délégables** (l'agent ne peut jamais les exercer) : gestion des
  comptes, rôles, politiques, connecteurs, compétences, paramètres, lecture de l'audit,
  décision de validation, dépôt et suppression de documents (`document:write`,
  ADR-095 : un document écrit par l'agent serait une source d'injection durable pour
  les runs de tous ses collègues). C'est ce qui empêche une injection ou une auto-amélioration
  de s'accorder des privilèges.
- **Validation par un responsable** (ADR-059) : désigné parmi les responsables d'unité,
  il doit détenir `approval:decide` sur la personne, à la demande puis à la décision ;
  la position seule n'accorde rien. Le run reprend avec les droits de la personne pour
  qui l'agent agit, jamais avec ceux du décideur ; sans responsable éligible, l'action
  est refusée. La personne ne peut pas approuver la demande, seulement la retirer
  (l'action n'est alors pas exécutée). L'e-mail qui prévient le responsable (ADR-060),
  comme celui qui prévient l'auteur d'une tâche planifiée, d'une mission ou d'un
  déclencheur dont le run attend sa validation (ADR-082, ADR-091, ADR-111), ne contient
  ni l'action ni ses paramètres, lisibles seulement après connexion ; celui d'un
  déclencheur ne contient rien non plus des données reçues, pas même dans son titre (la
  conversation qu'ouvre un envoi garde l'intitulé du déclencheur) ; celui qui annonce la fin d'une mission ne
  contient ni sa réponse ni la raison de son arrêt (ADR-091) ; celui qui annonce aux
  administrateurs des compétences les propositions de l'agent ne nomme que les
  compétences, jamais leur procédure ni la personne pour qui elles ont été proposées,
  et ne va qu'à qui détient `skill:manage` sur toute l'entreprise (ADR-102). Chacun peut ne plus
  recevoir ces avis (ADR-094) : un avis non voulu n'est pas envoyé, mais tracé, refusé ;
  les e-mails d'invitation et de mot de passe sont toujours envoyés.
- **Remplaçant d'un responsable absent** (ADR-071) : désigné par le responsable (ou pour
  lui par l'administrateur des comptes, ADR-078) pour une période d'au plus 90 jours, il
  ne reçoit aucun droit. Il ne voit et ne décide les
  demandes confiées à l'absent que s'il détient lui-même `approval:decide` sur la
  personne (404 sinon), tant que la période court et que le compte de l'absent est
  actif ; un seul relais ; il ne voit jamais ce que l'absent a décidé ; l'audit nomme
  l'absent qu'il remplace. L'avis et la relance ne lui sont envoyés que s'il peut décider
  les demandes (ADR-072). La liste des remplaçants possibles (noms des collaborateurs
  habilités à valider) n'est donnée qu'à qui valide lui-même (ADR-073), et à
  l'administrateur des comptes qui déclare l'absence d'un collaborateur (ADR-078).
- **Administration** (ADR-019) :
  - les permissions d'administration (`TENANT_WIDE_PERMISSIONS` : organisation,
    comptes, rôles, politiques, connecteurs, compétences, paramètres, mémoire, audit,
    consommation) ne valent que détenues **sur toute l'entreprise** ; un rôle qui en
    contient ne s'attribue qu'à ce périmètre ;
  - personne ne modifie depuis l'administration son propre compte, son statut ou ses
    habilitations : il faut un autre administrateur (pas de modification directe de
    ses propres droits, pas de blocage de soi-même) ;
  - les changements d'administration d'une entreprise sont sérialisés (verrou
    PostgreSQL par entreprise, ADR-020) et revérifient sur les données en base les
    droits de leur auteur et les règles de l'organigramme : deux administrateurs
    simultanés ne peuvent ni créer de cycle, ni se retirer mutuellement leurs droits ;
  - un compte créé reçoit un lien d'invitation à usage unique, valable 7 jours
    (ADR-027) : jeton de 256 bits dont seule l'empreinte est conservée, envoyé par
    e-mail si un relais SMTP est configuré, sinon montré une seule fois à
    l'administrateur (`Cache-Control: no-store`), jamais journalisé ni audité ; la
    personne y choisit son mot de passe. Le premier administrateur d'une entreprise
    est invité de même par l'exploitant de la plateforme (`mimorya-admin`, ADR-033) :
    le lien n'apparaît que sur la sortie de sa commande, à transmettre par un canal
    sûr. Pour réinitialiser un compte, l'administrateur
    lui envoie, si un relais est configuré, un lien de réinitialisation valable
    24 heures (ADR-029) qu'il ne voit jamais (le mot de passe actuel et les sessions
    restent valables jusqu'à son usage), ou lui donne un mot de passe provisoire
    aléatoire (96 bits), montré de même que le lien d'invitation ; ce mot de passe
    provisoire et la désactivation ferment immédiatement les sessions (et la désactivation supprime
    l'invitation en cours) ; le changement de son mot de passe exige l'actuel (tentatives
    limitées comme la connexion), ferme les autres sessions et n'écrase jamais une
    réinitialisation ou une révocation faite au même moment par un administrateur
    (écriture conditionnée à l'ancienne empreinte, session vérifiée sous verrou) ;
  - une connexion n'ouvre sa session qu'après avoir relu, ligne du compte
    verrouillée, que le mot de passe vérifié est toujours celui en base et que le
    compte est actif : une connexion en cours ne survit pas à une réinitialisation,
    une désactivation ou un changement de mot de passe validé pendant sa vérification ;
  - un compte qui détient un mot de passe provisoire (réinitialisation) n'atteint que son identité, la déconnexion et le changement de mot de passe tant
    qu'il ne l'a pas remplacé (contrôle par l'extracteur d'identité `Auth`, ADR-021 ;
    la connexion reste possible) ;
  - rôles personnalisés (ADR-021) : construits à partir du catalogue de permissions,
    sans joker ; les rôles de la plateforme ne se modifient pas ; personne ne modifie
    ni ne supprime un rôle qu'il détient ; un rôle qui contient une permission
    d'administration reste attribué sur toute l'entreprise ; un rôle encore attribué
    ne se supprime pas ; un changement s'applique aussitôt à ses détenteurs, et la
    revérification des droits sous verrou relit les rôles et les politiques en base ;
  - absence d'un collaborateur empêché (ADR-078) : seul un administrateur des comptes
    (`user:manage` sur toute l'entreprise) la déclare pour lui, jamais pour lui-même ni
    en se désignant remplaçant. Elle n'accorde aucun droit : le remplaçant ne décide que
    là où il détient lui-même `approval:decide`. Elle détourne pourtant les avis du
    responsable vers le remplaçant (ADR-072) : le collaborateur la lit dans
    « Validations » avec le nom de l'administrateur qui l'a déclarée (ADR-079), en est
    prévenu par e-mail quand un relais est
    configuré, et peut y mettre fin. Chaque déclaration et chaque levée sont tracées au
    nom de l'administrateur ;
  - limites assumées :
    - la règle « pas d'administration de soi-même » évite les erreurs, ce n'est **pas
      une frontière contre un administrateur malveillant** : il peut créer un compte,
      lui attribuer un rôle d'administration et agir par ce compte ; la
      réinitialisation par mot de passe provisoire lui permet de se connecter à la place
      de la personne (y compris un autre administrateur) et donc de lire ses conversations et
      sa mémoire personnelle. Tout est tracé (`user.created`, `role.assigned`,
      `user.password_reset`…) ; la double validation des actions d'administration est
      prévue en phase 3 ;
    - les adresses e-mail sont uniques entre entreprises (ADR-011) : créer un compte
      avec une adresse déjà utilisée ailleurs est refusé (409), ce qui révèle à
      l'administrateur qu'elle existe dans une autre entreprise. L'invitation par
      e-mail ne lève pas cette limite (un compte appartient à une seule entreprise) ;
    - les comptes créés ou réinitialisés avant la migration `0007` ne sont pas tenus
      de remplacer leur mot de passe provisoire (ADR-021) : sur une instance déjà en
      service, les réinitialiser au besoin ;
    - un run d'agent en cours relit les droits de son utilisateur avant chaque appel au
      modèle, avant les appels d'outils qu'il renvoie, avant de conclure une réponse et
      à la reprise après validation (ADR-035) : un compte désactivé, ou privé de
      `agent:use`, l'arrête à ce moment, une entreprise suspendue dans les 30 s au plus
      (la suspension se fait en base, l'API le voit à l'expiration du cache de
      l'entreprise) ; une habilitation retirée ne compte plus. Restent
      acquis au run : ses consignes et mémoires rappelées au démarrage, les résultats
      d'outils déjà obtenus, l'outil en cours d'exécution (délai borné), le registre
      d'outils jusqu'à la pause ou la fin et le niveau d'autonomie. Les règles de
      l'entreprise (rôles, politiques) viennent de son cache, invalidé par chaque
      changement d'administration dans tous les processus (`api`, `worker`) : le
      processus qui l'a fait l'annonce aux autres (ADR-085), au mieux ; une annonce
      perdue, l'écart reste borné par la durée de vie du cache (30 s), et un processus
      reconnecté au canal vide ses caches ;
    - une mission (ADR-089) n'est lue et arrêtée que par son auteur (404 pour tout
      autre, administrateur compris) : la confier demande `mission:create` en plus de
      `agent:use`, la suivre `mission:read`. Chacune de ses étapes est un run comme un
      autre, qui relit les droits de son auteur à chaque étape : retirer `agent:use`
      l'arrête ; retirer `mission:create` n'empêche que d'en confier de nouvelles. Ses
      limites bornent ce qu'elle coûte : celles de l'entreprise (ADR-093), six étapes et
      12 USD par défaut, 20 étapes et 60 USD au plus, réglées par les administrateurs
      des réglages de toute l'entreprise et gardées par la mission dès qu'elle est
      confiée ; chaque étape bornée par ce qui reste (3 USD au plus) ; trois missions en
      cours par personne. S'y ajoutent, par étape, l'appel qui franchit son budget et
      son point d'étape, et la revue de la réponse finale.
      L'arrêt retire son bail à l'étape en cours, dont l'exécuteur n'écrit ni n'appelle
      plus rien.

## 6. Sécurité de l'agent

| Menace                                   | Mesure                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Injection de prompt directe ou indirecte | Autorisations hors modèle ; résultats d'outils balisés « non fiables » (seule la compétence active chargée par `skill_read` est balisée comme procédure de l'entreprise, ADR-052) ; consignes système fixes, non modifiables par les compétences ou la mémoire ; actions sensibles soumises à validation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Exfiltration via outils externes         | Outils `external` soumis à validation par défaut ; destinataires et domaines autorisés configurables (phase 2) ; pas d'outil de requête HTTP arbitraire                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Abus d'outils / boucles                  | Budgets par run (étapes, tokens, coût, durée), délai par outil, limites de parallélisme                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Paramètres modifiés après validation     | Les paramètres validés sont figés et exécutés tels quels ; une action que son outil ne décrit plus comme elle a été validée (déclaration changée entre-temps) n'est pas exécutée (ADR-112)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| Hallucination                            | Contrôle d'ancrage des identifiants cités, vérification des critères, sources affichées                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Données hors périmètre dans le contexte  | Mémoire, documents (ADR-096) et outils filtrés par le périmètre et l'habilitation du sujet avant d'atteindre le modèle                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| Écriture malveillante en mémoire         | Mémoire partagée en statut `proposed` jusqu'à validation ; détection de motifs d'injection (phase 2)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Compétence malveillante ou régressive    | Versions immuables, activation humaine (`skill:manage`, non délégable), aucune permission portée par une compétence, retour arrière, contenu sans caractère invisible et description d'une ligne (ADR-052) ; propositions de l'agent validées par la personne avant envoi puis relues par les administrateurs, jamais lues par un run avant leur activation (ADR-100), sinon comme procédure dans l'évaluation qu'en demande un administrateur, qui la lit d'abord (ADR-104) ; évaluation d'une version sur les cas écrits par les administrateurs, jouée en lecture seule pour l'administrateur qui la demande, avec ses droits, sans run, sans action ni validation, ses réponses lues par lui seul, et activation contre ses régressions seulement confirmée (ADR-103, ADR-104) ; critères de qualité d'une version écrits par les administrateurs, une ligne visible chacun, lus par l'agent à la fin de la procédure et par le modèle qui juge la réponse, qu'aucun outil autre que `skill_read` ne peut ajouter et qu'aucune proposition de l'agent ne change : elle reprend ceux de la version active, et une évaluation juge les deux versions sur les critères de l'une et de l'autre (ADR-106)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| Serveur MCP non fiable                   | Association explicite outil → permissions/risque par l'administrateur, risque `write` par défaut, `connector:use` toujours exigé, résultats non fiables, descriptions, nombre d'outils et schémas bornés, sans caractère invisible (retirés des titres et de la documentation avant le masquage du jeton, refusés dans un nom d'outil et ailleurs dans un schéma, ADR-058), description d'un outil remplaçable par l'administrateur, sans caractère invisible et toujours précédée du nom du serveur (ADR-057) ; HTTPS vers une adresse publique seulement (résolution DNS filtrée à la connexion, pas de redirection), stdio réservé à l'exploitant ; jeton (alphabet RFC 6750) masqué dans tout ce que renvoie le serveur, tel quel, échappé en JSON ou encodé dans une URL (ADR-026), journaux de `rmcp` coupés par défaut ; un essai avant l'enregistrement (ADR-050) subit les mêmes contrôles et n'enregistre rien, le jeton absent de l'audit ; réponses JSON et réponses d'erreur de tout type lues en entier, seuls les événements SSE étant bornés (16 Mio, `rmcp`) : taille des réponses non bornée (dette, ADR-025)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| API REST décrite par l'entreprise        | Mêmes règles qu'un outil MCP (association par l'administrateur, `write` par défaut, `connector:use`, résultats non fiables, HTTPS public sans redirection, jeton RFC 6750 scellé) ; déclaration bornée et visible (ADR-057) ; requête limitée aux paramètres déclarés et typés, paramètre de chemin encodé entier et refusé s'il ferait remonter le chemin, chemin sans `%`, `?`, `#` ni segment `.`/`..` tel qu'envoyé ni tel qu'un serveur décodant `%2F` ou `%5C` (une ou deux fois) le lit, adresse obtenue contrôlée : jamais hors du chemin écrit ; en-têtes du protocole et du client (`Content-*`, `User-Agent`…) réservés ; requête montrée entière à la validation ; politique d'une opération retirée ou changée de méthode, de chemin ou de paramètres retirée avec elle, politique refusée pour une opération non déclarée (ADR-113) ; réponse lue jusqu'à 256 Kio, en-têtes non transmis, contenu non textuel non lu ; jeton masqué (même écrit comme un nombre JSON) avant la troncature, la fin de la lecture retirée pour qu'aucun morceau ne reste ; appel borné à 20 s (ADR-112)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Description OpenAPI d'une API REST       | Lue seulement, jamais téléchargée : document JSON donné par un administrateur des connecteurs (`connector:manage`), rien enregistré, appelé ni audité ; références internes seulement (512 octets, dix sauts au plus, une boucle s'arrête) ; lecture bornée quelle que soit la forme du document : 768 Kio, chaque partie payée par sa taille (un texte quatre fois plus cher) chaque fois qu'elle est relue (budget de 200 000 unités), textes lus jusqu'à leur borne seulement, 1 000 opérations converties, 200 proposées pesant 2 Mio au plus, 100 remarques ; hors de l'exécuteur des requêtes ; textes et remarques sans caractère caché ; aucune opération que son outil ne saurait appeler ; chaque opération proposée vérifiée comme une déclaration, ce qui ne l'est pas laissé de côté et dit ; une description YAML convertie dans le navigateur de l'administrateur (alias répétés au plus 100 fois), jamais lue en YAML sur le serveur (ADR-115) ; un administrateur coche et relit avant d'enregistrer, et les outils restent `write` jusqu'à leur association (ADR-114)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Microsoft 365 relié par chacun           | Permissions déléguées seulement, jamais d'application : la plateforme n'obtient que ce que la personne peut lire, avec son accord (ADR-116) ; application déclarée par un administrateur des connecteurs (annuaire et application en GUID, secret scellé jamais renvoyé ni audité, une par entreprise, émetteur et Graph fixés par la plateforme, secret remplacé jamais retiré) ; liaison par la personne elle-même (`connector:use`, connecteur actif), tentative scellée dans un cookie `HttpOnly` à son chemin (dix minutes, `state`, `nonce`, PKCE) et liée à la personne qui l'a commencée ; jeton d'identité vérifié et devant nommer son propre compte (identifiant d'objet, nom de connexion ou adresse au domaine vérifié égal à l'adresse du compte) ; jeton de renouvellement scellé pour l'entreprise, le connecteur et la personne, jamais montré, journalisé ni audité, le jeton d'accès non gardé ; navigateur informé seulement de l'issue, refus audités avec leur raison ; liaison retirée par la personne, avec le connecteur ou le compte désactivé ; émetteur de l'annuaire chez Microsoft, que seul le serveur des tests de bout en bout remplace quand il joint le fournisseur, jamais dans la déclaration (`BuildOptions`, ADR-118)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Outils de lecture Microsoft 365          | Lecture seule (`read`), `connector:use` toujours exigé, même par une politique (ADR-117) ; jeton d'accès de la seule personne servie, donné par la plateforme (`LinkedAccounts`), jamais choisi par l'outil, aucun pendant une évaluation ; renouvelé depuis la liaison avec les seules permissions accordées, gardé en mémoire du processus jusqu'à deux minutes avant son expiration et seulement pour la version de la liaison relue à chaque appel, oublié si Graph le refuse ; jeton de renouvellement tourné scellé si la liaison n'a pas changé, liaison retirée et auditée quand Microsoft ne l'honore plus ; identifiant d'e-mail strict et encodé entier ; client durci (adresses publiques, sans redirection), appel borné en temps et à 1 Mo ; texte des erreurs de Graph jamais montré au modèle ; jetons absents du journal, des événements et de l'audit                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Écriture Microsoft 365                   | Outils proposés seulement si l'entreprise le permet (`writeTools`, audité) ; risque `external` et validation toujours exigée (ADR-119) ; permissions `Mail.Send` et `Calendars.ReadWrite` demandées à la liaison seulement alors, vérifiées dans la liaison avant d'atteindre Microsoft (`NotGranted`) ; aucune politique ne retire la validation ; ce qui part ne cache rien : adresses simples (caractères permis avant l'unique `@`, nom de domaine sans adresse IP, règle partagée avec Google Workspace, ADR-123), en ASCII et uniques, objet et lieu sur une ligne visible, texte sans caractère caché, début de rendez-vous borné, refusés avant la demande de validation ; description donnant les destinataires avant ce qu'écrit le modèle ; paramètres montrés entiers à qui valide et exécutés tels quels, envoyés une fois, jamais répétés ; une écriture sans réponse, en erreur du serveur ou au délai dépassé dite peut-être faite, à vérifier ; jeton de la seule personne servie ; texte des erreurs de Graph jamais montré au modèle                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| Google Workspace relié par chacun        | Permissions déléguées seulement, jamais de délégation à l'échelle du domaine (compte de service) : la plateforme n'obtient que ce que la personne peut lire, avec son accord (ADR-121) ; client OAuth déclaré par un administrateur des connecteurs (identifiant de Google Cloud vérifié, secret scellé jamais renvoyé ni audité, un par entreprise, émetteur et adresses de Gmail et de l'agenda fixés par la plateforme, secret remplacé jamais retiré) ; liaison, tentative, retour, jeton de renouvellement scellé et retrait communs avec Microsoft 365 (ADR-116) ; accès hors connexion et consentement demandés à la façon de Google (`access_type=offline`, `prompt=consent`), `hd` donné comme simple indication ; jeton d'identité devant porter le sujet Google, une adresse vérifiée par Google et le domaine Workspace du compte (`hd`) égal au domaine de cette adresse, l'adresse égale à celle du compte : compte d'un collègue, compte personnel (sans `hd`), d'un autre domaine ou non vérifié refusé ; renouvellement sans permission nommée (celles accordées) ; jetons d'accès gardés par personne et connecteur, oubliés ensemble quand un fournisseur en refuse un                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Outils de lecture Google Workspace       | Lecture seule (`read`), `connector:use` toujours exigé, même par une politique (ADR-122) ; jeton d'accès de la seule personne servie, donné par la plateforme, permission accordée vérifiée avant Google, aucun pendant une évaluation ; client partagé avec Microsoft 365 (`delegated`) : adresses publiques, sans redirection, appel borné en temps et à 1 Mo, jeton refusé oublié ; identifiant d'e-mail strict et encodé entier ; recherche sans caractère de contrôle ; texte d'un e-mail borné, parties MIME parcourues jusqu'à une profondeur et un nombre bornés, pièces jointes nommées jamais lues ; texte des erreurs de Google jamais montré au modèle ; jetons absents du journal, des événements et de l'audit                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| Écriture Google Workspace                | Outils proposés seulement si l'entreprise le permet (`writeTools`, audité, ADR-121) ; risque `external` et validation toujours exigée (ADR-123) ; permissions `gmail.send` et `calendar.events` demandées à la liaison seulement alors, vérifiées dans la liaison avant d'atteindre Google ; règles de ce qui part partagées avec Microsoft 365 (`delegated`) : adresses simples (caractères permis avant l'unique `@`, nom de domaine sans adresse IP), en ASCII et uniques, objet et lieu sur une ligne visible, texte sans caractère caché, début de rendez-vous borné, refusés avant la demande de validation ; message Gmail écrit par la plateforme, refusé si une adresse n'est pas simple, objet en mots encodés et texte en base 64 : aucun en-tête ajouté par ce qu'écrit le modèle, même hors validation, expéditeur posé par Gmail ; paramètres montrés entiers à qui valide et exécutés tels quels, envoyés une fois, jamais répétés ; une écriture sans réponse ou en erreur du serveur dite peut-être faite, à vérifier ; jeton de la seule personne servie ; texte des erreurs de Google jamais montré au modèle                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Passerelle Slack                         | Application déclarée par l'entreprise (`connector:manage`, une par entreprise), jeton du bot essayé auprès de Slack avant d'être gardé, jeton et secret de signature scellés, jamais rendus ni audités ; nouveau jeton du même espace de travail seulement ; événements prouvés par leur signature seule (HMAC, cinq minutes), chemin exact dispensé de l'en-tête du navigateur ; identité Slack reliée depuis la session de la personne, Slack prouvant l'espace de travail et l'adresse du compte ; refus audités sans adresse (ADR-125) ; validations dans Slack limitées aux actions sans effet hors de l'entreprise et réversibles, aux paramètres exacts montrés entiers, celles à impact externe ou irréversibles exigeant la session de la personne, même pour qui détiendrait le secret de signature (ADR-129) ; responsable ou remplaçant prévenu par message direct s'il est relié, Slack interrogé d'abord, mêmes boutons et mêmes limites (ADR-130) ; conversation directe d'un message ou d'un bouton vérifiée auprès de Slack avant que rien n'y soit dit ni lancé (ADR-131) ; jeton sans une permission demandée refusé, permissions en plus montrées pour être retirées (ADR-132) ; bot Telegram : mises à jour prouvées par le secret du webhook seul, chemin exact dispensé de l'en-tête du navigateur, compte relié par un code demandé dans la session de la personne, accepté dans Telegram par qui le bot a dit à quel compte il le relierait, confirmé dans la session, une liaison jamais reprise par le code d'un autre (ADR-133) ; conversations dans Telegram lancées au nom de la personne reliée avec ses droits, réponse seulement tant que le compte Telegram est le sien et le bot actif, texte de l'agent échappé dans un HTML de mise en forme, validations dans l'application (ADR-135) ; validations dans Telegram limitées aux mêmes actions que dans Slack, un bouton pressé n'étant pris que s'il est l'un de ceux du message du bot dans la conversation privée de qui le presse (ADR-136) |

## 7. Secrets

- Secrets d'entreprise (clé d'API du modèle de l'entreprise, jetons des connecteurs,
  secrets clients des fournisseurs d'identité, ADR-063) chiffrés en **AES-256-GCM** (`ring`) avec une clé maître fournie
  par l'environnement (`MIMORYA_MASTER_KEY`, 32 octets base64), un nonce aléatoire par
  écriture, et l'entreprise et le nom du secret comme données associées : une valeur
  déplacée vers une autre entreprise ou un autre nom ne s'ouvre pas (ADR-024). Sans
  clé maître, aucun secret n'est enregistré. En production, les clés maîtres publiées
  par les tests du dépôt (`PUBLIC_TEST_MASTER_KEYS`) sont refusées au démarrage : une
  installation qui en utilisait une en génère une autre, et les secrets scellés avec
  l'ancienne (clé du modèle, jetons des connecteurs) sont à ressaisir. `key_version` prépare la rotation (avec
  un KMS, phase 3 : pas encore faite).
- Clé du modèle d'une entreprise : écriture seule (`keyStored` seulement), jamais dans
  l'audit, et liée au fournisseur et à l'adresse pour lesquels elle a été saisie. Les
  adresses d'un fournisseur local ou privé sont choisies dans la liste de l'exploitant
  (`MIMORYA_TENANT_LLM_ENDPOINTS`, commune à toutes les entreprises) : une entreprise
  ne fait jamais appeler par le serveur une adresse de son choix (SSRF), et une
  adresse retirée de la liste n'est plus appelée (configuration revérifiée à l'usage). Si le modèle choisi par l'entreprise devient
  inutilisable, ses runs s'arrêtent, sans repli vers le fournisseur de la plateforme.
- Modèle de contrôle des réponses d'une entreprise (ADR-108) : mêmes règles, avec sa clé à
  part (`verification.api_key`), qui ne sert jamais aux conversations, ni la leur à lui.
  Choisi par l'entreprise, il reçoit la demande, les critères, les identifiants et
  libellés des sources consultées et la réponse à juger. S'il devient inutilisable, ou si
  un appel échoue dans un run, le modèle des conversations juge à sa place : il lit déjà tout cela,
  rien ne part ailleurs.
- Modèle d'embeddings d'une entreprise (ADR-074) : mêmes règles (clé `embedding.api_key`
  en écriture seule, liée à son fournisseur et à son adresse ; adresses locales prises
  dans la même liste de l'exploitant ; configuration revérifiée à l'usage). Désactivé par
  défaut : sans choix de l'entreprise, rien de sa mémoire ne part vers un tel modèle. Une
  fois choisi, le texte des mémoires actives (jamais d'une proposition non approuvée) et
  celui des demandes y partent ; un modèle inutilisable laisse la recherche aux mots,
  sans repli vers un autre. Les vecteurs ne comparent que les mémoires que le sujet peut
  lire (pré-filtrage SQL), et sont tous effacés au retour à la recherche par les mots :
  l'indexation relit le choix avant chaque lot et le verrouille pendant l'écriture, si
  bien qu'après ce retour seul le lot déjà en vol a pu partir, et ses vecteurs sont
  jetés. Le texte des passages de documents (ADR-096) part de même vers ce modèle,
  quelle que soit leur sensibilité, et leurs vecteurs sont effacés avec ceux de la
  mémoire.
- Jamais renvoyés par l'API, jamais injectés dans le contexte du modèle, jamais
  journalisés : les journaux de requêtes ne contiennent ni en-têtes ni corps (seuls
  méthode, chemin, statut, durée et identifiant de requête), et les structures qui
  portent une clé (`ProviderConfig`, options des fournisseurs) n'affichent jamais sa
  valeur en débogage. Le journal d'audit ne recopie jamais une entrée refusée.
- Clés LLM et clé maître : variables d'environnement / gestionnaire de secrets ; jamais
  committées (`.env` ignoré, `.env.example` sans valeur réelle).

## 8. Journal d'audit

- Table `audit_log` en **ajout seul** : le rôle applicatif n'a que `INSERT` et `SELECT`.
- Ni effacement ni réécriture indirecte : le rôle applicatif ne peut pas supprimer les
  entreprises, utilisateurs, conversations et runs que l'audit référence (les clés
  étrangères effaceraient ou videraient des entrées). Leur suppression relève du rôle
  propriétaire (voir [DATA_MODEL.md](DATA_MODEL.md) §4).
- Contenu : qui (utilisateur, agent pour le compte de…), quoi (action, ressource),
  résultat (succès, refus, échec), pourquoi (raison de la décision), contexte
  (requête, run, IP). Pas de contenu de message ni de secret.
- Actions d'administration refusées (ADR-038) : toute modification refusée (403) par
  une route d'administration, pour un utilisateur connecté, est tracée
  (`admin.refused`, résultat `denied`) avec sa raison (le message du refus, écrit par
  la plateforme : sur soi-même, permission absente ou limitée à une équipe, rôle de la
  plateforme, mot de passe temporaire à remplacer…), la méthode et la route,
  l'identifiant (UUID) que porte le chemin, l'IP et l'identifiant de requête. Hormis
  cet identifiant, rien de ce que la requête a envoyé n'y figure ; il n'est pas
  vérifié : quand le refus précède toute lecture (permission absente, mot de passe
  temporaire à remplacer, compte inactif), il peut
  désigner une ressource absente ou d'une autre entreprise, sans que la réponse en
  dise rien. Ne sont pas tracés : les lectures refusées, les refus anti-CSRF (en-tête
  ou origine, prononcés avant la résolution de la session), les requêtes invalides
  (hormis l'identifiant inconnu envoyé dans le corps, ADR-054, ci-dessous) et les
  requêtes anonymes. Un échec d'écriture de cette trace est journalisé et ne
  change pas la réponse.
- Modifications d'administration sur une ressource introuvable (ADR-047) : la
  modification, par un administrateur habilité, d'une ressource que désigne
  l'identifiant du chemin et que la plateforme répond introuvable (404) est tracée de
  même (`admin.not_found`, résultat `denied`, raison « Compte introuvable »…), dans
  l'entreprise de l'administrateur. L'entrée, comme la réponse, ne distingue pas une
  ressource absente d'une ressource d'une autre entreprise : la distinguer ferait de
  l'audit un oracle d'existence pour ses lecteurs.
- Identifiants inconnus envoyés dans le corps d'une requête d'administration (ADR-054) :
  le rôle, l'unité ou le responsable que désigne le corps et que l'entreprise n'a pas
  répond comme une requête invalide (400), identique pour une ressource absente et pour
  celle d'une autre entreprise, et est tracé (`admin.unknown_reference`, résultat
  `denied`) avec la raison, la route, la ressource du chemin, le type et l'identifiant
  envoyés (`metadata.reference`), dans l'entreprise de l'administrateur seulement. Les
  autres requêtes invalides ne sont pas tracées.
- Documents (ADR-095) : dépôt (`document.uploaded` : espace, unité, type, taille,
  pages, passages, sensibilité), téléchargement (`document.downloaded`), suppression
  (`document.deleted`), et fichier que le processus de lecture n'a pu lire
  (`document.rejected`, résultat `failure`, raison `timed_out` ou `reader_failed`,
  type et taille) ; jamais le nom du fichier, son titre ni son texte. Les fichiers
  refusés pour leur forme (taille, type, chiffrement, texte absent) ne sont pas tracés,
  comme les autres requêtes invalides.
- Phase 3 : chaînage cryptographique des entrées et export vers un SIEM.

## 9. Protection des données (RGPD)

- **Minimisation** : seules les données nécessaires sont envoyées au modèle ; la
  mémoire personnelle n'est constituée qu'avec l'accord de l'utilisateur. Une leçon tirée
  d'une conversation (ADR-076, désactivé par défaut) n'est qu'une proposition dans la
  mémoire personnelle de la personne servie, qu'elle seule voit et valide : aucun
  collègue ne lit ce qu'une conversation d'un autre a appris ; le run relu est remis au
  modèle comme donnée.
- **Transparence** : chaque utilisateur consulte sa mémoire et l'historique des actions
  faites pour lui. Une tâche planifiée (ADR-080) n'agit qu'en son nom, avec ses droits du
  moment, dans une conversation à lui ; elle ne part pas pour un compte désactivé (elle se
  met en pause), à mot de passe provisoire ou privé de l'agent, ni pour une entreprise
  suspendue. Ce qu'est devenue la dernière occurrence lui est dit, et chacune est tracée
  dans le journal, comme le démarrage de son run : par la plateforme, à son nom.
- **Droits** : export (JSON) et suppression des données d'un utilisateur ; suppression
  complète d'une entreprise ; journal conservant la trace sans le contenu. Un document
  déposé (ADR-095) appartient à l'entreprise : il reste au départ de celui qui l'a
  déposé, se télécharge tel quel par qui le lit, et sa suppression par qui peut écrire
  son espace emporte son fichier, ses passages et leurs vecteurs. L'agent n'en lit,
  par `document_search` et `document_read` (ADR-096), que ce que son utilisateur peut
  lire, avec ses droits du moment ; un document hors de portée lui répond comme
  inexistant, et ses passages lui sont remis comme données.
- **Rétention** paramétrable (voir [DATA_MODEL.md](DATA_MODEL.md) §5).
- **Sous-traitants** : choix du fournisseur LLM et de sa région par entreprise ;
  possibilité de modèles locaux ; registre des sous-traitants et DPA.
- **Hébergement** : SaaS en UE ; installation privée possible.

## 10. Sécurité applicative

- Validation de toutes les entrées (serde + validator, messages en français) ; requêtes SQL
  paramétrées (sqlx) ; aucun `unsafe` (`unsafe_code = "forbid"` sur tout l'espace de travail).
- En-têtes de sécurité (ADR-030) :
  - **pages web** : CSP stricte avec un nonce de 128 bits nouveau à chaque page
    (`proxy.ts`) ; scripts et styles de l'origine portant le nonce, images et connexions
    vers l'origine seulement (une image distante dans une réponse du modèle n'est pas
    chargée : pas de fuite par son adresse), ni `object`, ni `base`, ni cadre. S'y
    ajoutent `X-Content-Type-Options: nosniff`, `Referrer-Policy`, `X-Frame-Options:
DENY`, `Permissions-Policy` et `Cross-Origin-Opener-Policy: same-origin` ;
  - **API** : toute réponse, refus compris, porte `Content-Security-Policy:
default-src 'none'; frame-ancestors 'none'`, `X-Content-Type-Options: nosniff`,
    `X-Frame-Options: DENY`, `Referrer-Policy: no-referrer` et `Cache-Control:
no-store` (le flux d'événements garde `no-cache, no-transform`) ;
  - **HSTS** : à poser par le reverse proxy qui termine TLS (fourni :
    `infra/caddy/Caddyfile`, ADR-032 et ADR-034), par exemple
    `Strict-Transport-Security: max-age=63072000; includeSubDomains` (ajouter
    `preload` seulement après inscription du domaine). L'application ne le pose pas :
    elle ne sait pas si elle est servie en HTTPS.
- Limitation de débit globale et par route sensible (connexion, par adresse et par
  compte), voir §4 pour la détermination de l'adresse cliente. Les démarrages de
  connecteurs demandés par l'administration (test, essai) sont de plus bornés à deux en
  cours par entreprise (ADR-055).
- Documents déposés (ADR-095) : lus dans un processus à part (`mimorya-extract`), jamais
  dans celui de l'API, lancé avec un environnement vide et borné par lui-même avant de
  rien lire (1 Gio de mémoire, 20 s de processeur, aucun fichier écrit), tué au bout de
  30 s, deux à la fois au plus par processus ; type confirmé par les premiers octets,
  limites de taille, de pages, de texte et de décompression, aucune entité XML développée.
- Dépendances verrouillées (`Cargo.lock`, `pnpm-lock.yaml`), audit des vulnérabilités en CI
  (phase 2), revue des licences (MIT/Apache/BSD privilégiées, AGPL exclue).
- Sauvegardes chiffrées, restauration testée périodiquement (phase 3).

## 11. Liste de contrôle pour chaque contribution

- [ ] Toute nouvelle table métier a `tenant_id`, la politique RLS et des tests.
- [ ] Toute nouvelle route vérifie l'authentification et les permissions côté serveur.
- [ ] Tout nouvel outil déclare permissions requises et niveau de risque, et filtre ses
      données avec `ctx.authz`.
- [ ] Aucun secret, jeton ou donnée personnelle superflue dans les logs.
- [ ] Les nouvelles permissions sensibles sont marquées non délégables si nécessaire.
- [ ] Les entrées externes (documents, résultats d'outils) sont traitées comme non fiables.
