# Compiler PDFinator

## Prérequis Windows

- Windows 10/11 **x64**.
- Rust stable pour **x86_64-pc-windows-msvc**, au minimum la version déclarée dans
  `Cargo.toml` (1.95). La V1 a été construite avec Rust/Cargo 1.99.1.
- Visual Studio Build Tools : outils C++ pour applications de bureau et Windows
  SDK. Le compilateur de ressources `rc.exe` est utilisé pour l’icône de l’EXE.
- Python **3.11 ou supérieur**, accessible par la commande `python`, pour assembler
  les mentions légales dans les scripts de distribution.
- Facultatif : **Inno Setup 6 ou 7** pour construire l’installeur.

Le premier téléchargement des dépendances Rust nécessite Internet. L’application
construite fonctionne ensuite hors ligne. `vendor/` contient les dépendances
locales et leurs correctifs ; ce dossier doit être conservé avec les sources.

## Récupérer le projet

```powershell
git clone https://github.com/jossmair/PDFinator.git
cd PDFinator
cargo fetch --locked
```

Pour les sources figées de la V1, choisir le tag `v1.0.0-sources`, ou télécharger
l’archive **PDFinator-1.0.0-sources.zip** depuis la page de version. Le tag initial
`v1.0.0` a été créé lors de la première publication des téléchargements, avant
l’ajout du code au dépôt ; le tag `v1.0.0-sources` référence le code complet.

## Construire et lancer

```powershell
cargo build --locked
cargo run --locked -- "C:/Documents/exemple.pdf"
```

Pour préparer l’exécutable optimisé et son fichier légal :

```powershell
./build.ps1 -SkipTests
```

Les résultats sont `runtime/pdfinator.exe` et `runtime/LICENCES.txt`. Le script
utilise la liaison statique du runtime C Microsoft et incorpore les ressources
de version et l’icône. Les chemins de sortie restent relatifs au projet.

## Vérifier les sources

```powershell
New-Item -ItemType Directory -Force test-output | Out-Null
cargo test --locked -j 2 -- --test-threads=2
cargo clippy --locked --all-targets -- -D warnings
```

Les tests OCR Windows utilisent de vraies reconnaissances locales, notamment du
texte français : ils nécessitent les langues OCR correspondantes dans Windows.
Les tests de régression sur des documents externes sont optionnels et utilisent
les variables `PDFINATOR_REGRESSION_PDF`, `PDFINATOR_LAYOUT_PDF`,
`PDFINATOR_PRISMS_PDF`, `PDFINATOR_PERCENTAGES_PDF` et `PDFINATOR_MIXED_OCR_PDF`.
Ces documents personnels ne sont pas distribués. La suite ordinaire crée ses
documents synthétiques et ses sorties dans `test-output/`.

Les tests Windows ne soumettent aucun travail papier. Les outils de vérification
indépendants dans `scripts/` peuvent nécessiter des bibliothèques Python et
LibreOffice ou d’autres outils indiqués dans leurs sources.

## Créer les distributions

Après `./build.ps1 -SkipTests` :

```powershell
./scripts/package-portable.ps1
./scripts/package-setup.ps1
```

Le compilateur Inno Setup peut être indiqué explicitement :

```powershell
./scripts/package-setup.ps1 -Compiler "C:/Outils/Inno Setup/ISCC.exe"
```

Les distributions sont produites dans `dist/`. Le portable contient l’EXE natif
et `LICENCES.txt`. La génération contrôle la version et les mentions légales.
Les exécutables officiels de la V1 ne sont pas signés Authenticode.

Une reconstruction peut produire une empreinte différente selon la version du
compilateur et l’environnement. Aucune reproductibilité bit à bit n’est revendiquée.

## Organisation

| Chemin | Contenu |
| --- | --- |
| `src/ui/` | Interface egui : documents, outils, pages, OCR, impression et export. |
| `src/document.rs` | Opérations PDF, sauvegarde et historique. |
| `src/docx/` | Reconstruction des documents DOCX et ODT. |
| `src/ocr/`, `src/ocr.rs` | Reconnaissance locale et préparation des images. |
| `tests/`, `examples/` | Régressions et documents de démonstration synthétiques. |
| `assets/` | Icône et pictogrammes PDFinator, captures de présentation. |
| `vendor/` | Dépendances locales et correctifs, avec leurs licences d’origine. |
| `licenses/` | Notices, attributions et mentions légales consolidées. |

## Licences

Le code original PDFinator est proposé sous **MIT OU Apache-2.0**, conformément
à `Cargo.toml`. Les composants tiers, polices et ressources conservent leurs
licences propres. Voir `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`,
`licenses/PDFINATOR-NOTICE.txt` et les notices présentes dans `vendor/`.

## Données de test exclues

Les conteneurs de certificats et de clés du jeu de tests amont (`.p12`, etc.)
ne sont pas distribués dans ce dépôt. Ils ne sont pas nécessaires pour compiler
PDFinator ni pour exécuter sa suite de tests ; certains tests de signature du
projet fournisseur, exécutés séparément, demanderaient leurs propres fixtures.
