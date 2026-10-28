# Vérification des exports Office

Les exports contiennent du texte, des tableaux et des illustrations modifiables.
Les tests de structure ne remplacent pas une comparaison du rendu dans un
traitement de texte. Les documents privés de l’utilisateur restent hors du dépôt.

## Corpus synthétique redistribuable

Le test Rust `native_scripts_keep_their_source_size_and_baseline_offsets` crée son
PDF en mémoire et s’exécute sans document extérieur. Il vérifie un indice, un
exposant et un petit caractère resté sur la ligne de base. Le test échouait avant
le correctif : l’indice perdait son décalage et l’exposant subissait une seconde
réduction de taille.

Le DOCX utilise le décalage vertical `w:position` en demi-points, qui conserve
la taille du caractère, conformément à la [documentation Open XML](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.wordprocessing.position?view=openxml-3.0.1).
L’ODT conserve explicitement 100 % de la taille du caractère déplacé.

Pour les contrôles indépendants, installer les dépendances de développement
Python `reportlab` et `pypdfium2`, puis lancer depuis la racine du projet :

```powershell
python scripts/office_corpus.py
cargo run --locked --example office_probe -- test-output/office-corpus/scripts.pdf test-output/office-corpus/scripts-after
cargo run --locked --example office_probe -- test-output/office-corpus/mixed-pages.pdf test-output/office-corpus/mixed-pages
```

Avec LibreOffice installé, rendre les deux formats dans des dossiers distincts :

```powershell
foreach ($format in @('docx', 'odt')) {
    $destination = "test-output/office-after-$format"
    New-Item -ItemType Directory -Force $destination | Out-Null
    & 'C:/Program Files/LibreOffice/program/soffice.com' --headless --convert-to pdf --outdir $destination "test-output/office-corpus/scripts-after.$format" "test-output/office-corpus/mixed-pages.$format"
}
python scripts/verify_office_corpus.py test-output/office-corpus
```

Utiliser un profil LibreOffice de test distinct si une session est déjà ouverte.
Le vérificateur contrôle le texte natif, les réglages DOCX/ODT, la taille réelle
des petits caractères rendus, leur décalage relatif et les trois formats de page,
y compris une page blanche paysage au milieu du document. Il écrit un rapport
dans `test-output/office-corpus/verification.json`.

Les décalages relatifs sont contrôlés à moins d’un point dans ce corpus ; cette
tolérance ne constitue pas une garantie de fidélité sur tout PDF. Le placement
absolu de la ligne et le rendu dans Microsoft Word restent des contrôles séparés.

## Régressions locales sur documents fournis

Les tests de `src/docx.rs` acceptent les variables `PDFINATOR_LAYOUT_PDF`,
`PDFINATOR_PRISMS_PDF`, `PDFINATOR_PERCENTAGES_PDF` et `PDFINATOR_REGRESSION_PDF`.
Sans ces variables, ces tests optionnels ne chargent aucun document utilisateur.
Ne pas compter leur retour sans fichier comme une validation sur PDF réel.

Les scripts `verify_worksheet.py`, `verify_prisms.py` et `verify_percentages.py`
contrôlent ensuite les fractions éditables, les mots et positions, les bordures,
les illustrations et la pagination des exports rendus. L’ODT d’origine sert
uniquement d’oracle de vérification : il n’entre jamais dans le convertisseur.

## Recette locale du correctif 1.0.2

Les trois documents fournis ont été reconvertis puis rendus en PDF par
LibreOffice, dans les deux formats. Les originaux n’ont pas été modifiés.

| Document | Pagination | Contrôles | Écart vertical maximal mesuré |
| --- | --- | --- | --- |
| Multiples et diviseurs | 1 page dans les deux formats | 367 éléments textuels, fractions et tableau | DOCX 1,23 pt ; ODT 0,92 pt |
| Prismes et cylindres | 1 page dans les deux formats | 54 éléments textuels, patron et cylindre | DOCX 0,33 pt ; ODT 0,33 pt |
| Pourcentages | 1 page dans les deux formats | 5 fractions éditables, 54 éléments numériques, tableaux et flèches | Numérique : DOCX 1,50 pt ; ODT 1,37 pt |

Le corpus d’indices/exposants conserve les dimensions des petits caractères à
moins de 0,15 pt et leurs décalages relatifs à moins de 0,91 pt dans LibreOffice.
Le document synthétique de formats mixtes conserve ses trois pages, dont la page
blanche paysage. Ces mesures documentent le périmètre testé, pas une identité
de rendu générale ni une recette Microsoft Word.

## Recette locale du correctif 1.0.5

Quatorze exports ont été reconvertis et rendus dans LibreOffice : les six sorties
sur les trois documents fournis, les quatre sorties du corpus indices/exposants
et formats mixtes, puis quatre sorties minimales de fractions isolées.

- Les fractions isolées positive et négative sont des objets mathématiques
  éditables dans DOCX/ODT. Les quatre rendus ont une page, du texte visible et
  une barre présente ; le cas DOCX initialement blanc ne l’est plus.
- Les 14 fractions du document « Multiples et diviseurs », son tableau et les
  367 éléments textuels vérifiés sont conservés. Écart vertical maximal :
  DOCX 1,23 pt ; ODT 0,92 pt.
- Le document sur les prismes conserve ses 54 éléments textuels, son patron,
  son cylindre et sa page. Écart vertical maximal : 0,33 pt dans les deux formats.
- Les pourcentages conservent leurs cinq fractions éditables, tableaux et flèches.
  Écart vertical numérique maximal : DOCX 1,50 pt ; ODT 1,37 pt.
- Les petits indices/exposants conservent leurs dimensions à moins de 0,15 pt
  et leurs décalages relatifs à moins de 0,91 pt. Les trois formats de pages,
  dont la page blanche paysage, restent conservés.

Le contrôle de fractions isolées est reproductible avec
`scripts/fraction_corpus.py <dossier>`, conversion par `examples/office_probe.rs`,
puis rendu des deux DOCX dans `<dossier>/rendered-docx` et des deux ODT dans
`<dossier>/rendered-odt`. Exécuter ensuite
`scripts/verify_fraction_corpus.py <dossier>` avec pypdfium2 et lxml disponibles.
Il contrôle les objets éditables, le texte, la pagination et la présence de la
barre dans le rendu ; une simple recherche XML ne suffit pas.

Les exemples minimaux utilisent la police Inter libre déjà présente dans les
sources. Aucun document utilisateur n’est publié dans ce corpus. Ces contrôles
ne garantissent pas une identité générale : les traitements de texte recalculent
les polices de repli et la géométrie des équations. Le contrôle Microsoft Word
reste distinct des rendus LibreOffice effectués ici.
