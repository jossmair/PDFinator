# Dépendances : origine et suivi

## Contrôle RustSec du 26 octobre 2026

Outil : `cargo-audit 0.22.2`, installé avec son verrouillage et sans analyse des
binaires. Base consultée : `rustsec/advisory-db`, révision
`115e418ee9a2ff542fee41db641128fa84483383`. Aucune exception d’avis ajoutée.

Le verrouillage initial contenait **Tokio 1.54.0**, concerné par
[RUSTSEC-2026-0384](https://rustsec.org/advisories/RUSTSEC-2026-0384.html).
La mise à jour ciblée vers **1.54.1** élimine l’avis lors du contrôle suivant :
**zéro vulnérabilité connue signalée**, sur les 456 entrées du verrouillage.
Le graphe normal Windows x64 interrogé ne contient pas Tokio : ce constat ne
démontre pas une exploitation de l’EXE distribué et ne remplace pas une analyse
du code ou des fonctions réellement utilisées.

Cinq avis informatifs de non-maintenance restent visibles, sans masquage :

| Dépendance | Avis | Périmètre et suite |
| --- | --- | --- |
| `ttf-parser 0.25.1` | [RUSTSEC-2026-0192](https://rustsec.org/advisories/RUSTSEC-2026-0192.html) | Utilisée pour les polices Windows. Étudier une migration avec le corpus de polices et les régressions d’édition ; ne pas remplacer aveuglément le parseur. |
| `proc-macro-error 1.0.4` | [RUSTSEC-2024-0370](https://rustsec.org/advisories/RUSTSEC-2024-0370.html) | Dépendance de compilation via `fmt-derive` et `winprint`. Suivre une migration du fournisseur d’impression. |
| `zbus_names 4.3.5` | RUSTSEC-2026-0371 | Absente du chemin Windows normal vérifié ; suivre le chemin des portails Linux. |
| `zvariant 5.15.1` | RUSTSEC-2026-0370 | Même périmètre Linux ; migration à coordonner avec les dépendances parentes. |
| `zvariant_derive 5.15.1` | RUSTSEC-2026-0372 | Même périmètre Linux. |

« Non maintenu » n’est pas synonyme de vulnérabilité exploitable démontrée.
Ces avertissements demandent néanmoins un suivi. Les contrôles suivants utilisent
une base actualisée et peuvent donc produire un résultat différent de ce relevé.

## Version retirée du catalogue

Le contrôle de préparation 1.0.2 a également signalé `clipboard-win 5.4.1`
comme version retirée du catalogue crates.io (« yanked »). Elle est remplacée
par `5.4.3`, sans exception d’audit. Le fournisseur a corrigé la gestion de
[fragments HTML invalides](https://github.com/DoumanAsh/clipboard-win/commit/38c3c3fef175c20ed8f8e33dacb9b1e3bb8eca87)
et [l’attente entre tentatives d’ouverture](https://github.com/DoumanAsh/clipboard-win/commit/ed27008ae5d1f10cfbb266de039f0fb35fd05a56).
Un retrait de version n’est pas à lui seul la preuve d’une vulnérabilité. Le
contrôle CI refuse désormais les dépendances retirées (`--deny yanked`), tout
en laissant visibles les cinq avis de non-maintenance suivis ci-dessus.

## Composants locaux

- Le moteur PdfCraft copié est issu de la révision
  `ec0a965074f376a5bf610435948e4e5184bf3299`, version de workspace 0.9.0, avec
  correctifs locaux. Voir `licenses/PDFINATOR-NOTICE.txt` et les notices du fournisseur.
- Les crates locales `craft-libs` et `winprint` ainsi que les patches de
  `vendor/pdfcraft/vendor/` sont sélectionnés explicitement par Cargo.toml.
- Les licences des composants et polices restent dans `vendor/` et `licenses/`.
  Les conteneurs de certificats/clés de test amont ne sont pas distribués.

## Procédure de mise à jour

1. Identifier la version/révision cible et ses notes de sécurité auprès du fournisseur.
2. Comparer les sources et préserver ou reporter chaque correctif local pertinent ;
   ne pas remplacer tout `vendor/` par un téléchargement non revu.
3. Vérifier les dépendances, licences et ressources ; mettre à jour les notices et
   le verrouillage sans effacer les attributions antérieures.
4. Exécuter Clippy, les tests synthétiques, sauvegarde/réouverture, reprise après
   interruption et les régressions du corpus concerné.
5. Générer et vérifier les distributions avant de remplacer un téléchargement.

Un contrôle Cargo.lock ne certifie ni les bibliothèques vendored modifiées, ni les
décodages d’images, ni les fichiers PDF hostiles. Ces risques restent dans ROADMAP.md.
