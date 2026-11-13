# imap-proto, gardé par la plateforme

Copie d'`imap-proto` 0.17.0 (crates.io, licences MIT ou Apache-2.0, jointes), l'analyseur
avec lequel `async-imap` lit les serveurs IMAP et avec lequel la garde des littéraux
(`crates/connectors/src/standard_mail/literals.rs`) les relit (ADR-205). Le
`Cargo.toml` racine la substitue à celle de crates.io (`[patch.crates-io]`) : la
bibliothèque et la garde lisent avec elle.

## Ce qui change

L'analyseur s'appelle lui-même pour chaque corps imbriqué dans un corps
(`BODYSTRUCTURE`, `BodyStructure::parse`) et pour chaque extension imbriquée dans une
extension (`BodyExtension::parse`), sans limite. Un serveur qui envoie quelques
centaines de parenthèses ouvrantes (`* 1 FETCH (BODYSTRUCTURE ((((…`) déborde la pile
du fil qui le lit, ce qui arrête tout le processus. Une seule réponse d'environ 1 Kio y
suffit, dès l'accueil, avant tout identifiant.

Le correctif borne cette imbrication à 32 niveaux (`src/nesting.rs`, compteur par fil).
Au-delà, la réponse est refusée par un échec qu'aucune alternative de l'analyseur ne
rattrape : la bibliothèque comme la garde la refusent, et le processus continue.
Le code d'origine ne reçoit que trois ajouts : la déclaration du module dans
`src/lib.rs`, et une ligne en tête de chacune des deux fonctions dans
`src/rfc3501/body_structure.rs`. Chacun est marqué « Patch of the Mimorya platform ».
Le `Cargo.toml` n'a plus l'exemple ni la dépendance de test, non copiés.

Les tests de `crates/connectors` vérifient la borne (`standard_mail::literals`) et
qu'un serveur qui imbrique sans fin est refusé, à la connexion comme pendant une
lecture.

## Mettre à jour

Quand une version publiée d'`imap-proto` bornera elle-même cette imbrication, retirer
ce dossier, `exclude` et `[patch.crates-io]` du `Cargo.toml` racine, et la ligne
`COPY vendor vendor` d'`infra/docker/api.Dockerfile`. D'ici là, toute nouvelle version
utilisée par `async-imap` se recopie ici, avec les trois mêmes ajouts.
