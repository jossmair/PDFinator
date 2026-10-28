# Contribuer à PDFinator

Partir de `main`, conserver Cargo.lock et utiliser le guide [BUILDING.md](BUILDING.md).
Les dépendances locales et leurs licences font partie de la source ; ne les
remplacer ni ne supprimer leurs notices lors d’un correctif.

Pour une régression, fournir le déclencheur et un document synthétique ou librement
redistribuable. Ajouter un test qui vérifie le comportement attendu, puis contrôler
enregistrement/réouverture et annuler/rétablir lorsque le changement touche le PDF.
Éviter les documents personnels et les captures exposant des données d’élèves.

Avant une proposition :

```powershell
New-Item -ItemType Directory -Force test-output | Out-Null
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked -j 2 -- --test-threads=2
```

Les tests OCR réels nécessitent les langues Windows correspondantes. Les contrôles
GitHub doivent signaler un environnement manquant au lieu de prétendre avoir testé
un moteur OCR absent. Une validation avec Word, NVDA, un pilote ou une imprimante
doit nommer l’environnement réellement utilisé.

Pour les dépendances, exécuter `cargo audit` avec une base RustSec actualisée.
Préférer une mise à jour ciblée lorsqu’un correctif existe. Toute exception
éventuelle doit être justifiée par le chemin de dépendances et son périmètre ;
ne pas masquer un avis uniquement pour obtenir un contrôle vert.

## Préparer une publication

1. Mettre à jour version et notes ; reconstruire depuis une copie propre.
2. Exécuter les contrôles, la reprise entre processus et les régressions de documents.
3. Construire le portable et l’installeur avec les scripts, puis vérifier le contenu,
   l’icône, les versions, les empreintes et l’installation/désinstallation de test.
4. Documenter les limites de la recette matérielle et du système de signature.
5. Préparer la version GitHub en brouillon ; comparer les fichiers téléversés aux
   fichiers vérifiés avant publication. Ne pas déplacer un ancien tag pour changer
   silencieusement des téléchargements déjà distribués.

Les problèmes de sécurité suivent [SECURITY.md](SECURITY.md). Les priorités sont
suivies dans [ROADMAP.md](ROADMAP.md).

Les exports DOCX/ODT ont une [procédure de validation et un corpus synthétique](OFFICE-VALIDATION.md),
en complément des tests de structure Rust.

La [dissociation des schémas](GROUPS.md) est couverte par `cargo test --test forms`
et un test d’interface du menu contextuel. Toute extension à une nouvelle structure
doit conserver son rendu et les occurrences partagées dans ces tests.
