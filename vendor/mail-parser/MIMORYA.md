# mail-parser, gardé par la plateforme

Copie de `mail-parser` 0.11.9 (crates.io, licences Apache-2.0 ou MIT, jointes dans
`LICENSES`), la bibliothèque avec laquelle l'outil `mail_read` lit un e-mail (ADR-205).
Le `Cargo.toml` racine la substitue à celle de crates.io (`[patch.crates-io]`).

## Ce qui change

mail-parser lit un e-mail joint à un autre (`message/rfc822`, `message/global`, ou la
partie d'un `multipart/digest`) comme un e-mail dans l'e-mail qui le joint. Non encodé,
il le lit sans s'appeler lui-même. Encodé (quoted-printable, base64), il le décode, le lit
en s'appelant lui-même (trois fois au plus), puis le rend propriétaire de ses données
(`into_owned`), qui s'appelle elle-même une fois par e-mail joint dans ce qu'il a décodé.
Libérer l'arbre s'appelle aussi une fois par niveau. Quelques milliers d'e-mails joints
les uns dans les autres, en moins d'un mégaoctet que n'importe quel expéditeur peut
envoyer, débordent la pile du fil qui les lit, ce qui arrête tout le processus.

Le correctif borne ce nombre à 16 (`src/nesting.rs`). Une partie jointe à 16 e-mails
déjà n'est plus lue comme un e-mail mais gardée comme une pièce jointe, quel que soit son
encodage. Sont comptés les e-mails que l'analyseur tient pendant qu'il lit ce qu'ils
joignent, et ceux qu'il a décodés pour se rappeler lui-même : `parse_` reçoit le nombre
d'e-mails qui englobent ce qu'il lit (`enclosing`, 0 pour un e-mail lu par `parse` ou
`parse_headers`), si bien que chaque décodage ne repart pas de zéro. L'arbre reste peu
profond : ni sa lecture, ni sa conversion, ni sa libération ne s'appellent elles-mêmes
plus de seize fois. Le code d'origine ne change qu'en des lignes marquées « Patch of the
Mimorya platform » : la déclaration du module dans `src/lib.rs` ; dans
`src/parsers/message.rs`, le paramètre `enclosing` de `parse_` et les trois appels qui le
donnent, et le contrôle fait là où le type de la partie est connu. Le `Cargo.toml` n'a
plus les exemples ni le test d'intégration, non copiés avec les 5 Mio de messages qu'ils
lisent.

Les tests de `crates/connectors` vérifient la borne (`standard_mail::tools::message`) :
30 000 e-mails joints tels quels ou en quoted-printable, 20 000 en base64 dans un
multipart, et une chaîne qui alterne e-mails tels quels et e-mails décodés, chacun lu à
une profondeur d'exactement 16. `mail_read` lit 20 000 e-mails joints sous chacune des
trois premières formes à travers le faux serveur IMAP (`mail_tools_hostile.rs`).

## Mettre à jour

Quand une version publiée de `mail-parser` bornera elle-même cette profondeur, retirer
ce dossier et sa ligne de `[patch.crates-io]` du `Cargo.toml` racine. D'ici là, toute
nouvelle version se recopie ici avec les mêmes lignes marquées, et ses tests d'origine se
rejouent depuis le dépôt de mail-parser, le correctif appliqué.
