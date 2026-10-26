# Identité PDFinator

`pdfinator.svg` : identité vectorielle originale créée pour PDFinator le
21 octobre 2026, sous MIT OU Apache-2.0. Document blanc, monogramme P et crayon
cyan sur un carré bleu/violet arrondi. Le SVG est utilisé dans la barre de titre.

`examples/app_icon.rs` produit déterministement `pdfinator.png` (1024 × 1024,
alpha conservé) et `pdfinator.ico` (16, 20, 24, 32, 40, 48, 64, 128, 256 pixels).
Le PNG est l’icône de fenêtre ; le fichier ICO est incorporé à l’exécutable par
`build.rs` avec le compilateur de ressources du Windows SDK. Le vérificateur
`scripts/verify_executable_icon.py` compare les images contenues dans le PE aux
images de l’ICO. L’ancien logo raster pastel généré avec ImageGen a été remplacé.

Les 51 pictogrammes d’outils utilisés sont des SVG originaux dans `assets/icons/`, créés
pour PDFinator les 20 et 21 octobre 2026 sous la licence du projet. Ils utilisent une
grille de 32 unités, des formes pleines, des angles arrondis et des dégradés en
deux tons. Bleu/cyan pour les commandes, rose pour l’édition, violet pour les
pages et le texte, turquoise pour les images, jaune pour le surlignage.
Aucune bibliothèque d’icônes ni icône de logiciel tiers n’est utilisée.
`src/icons.rs` les charge depuis l’exécutable et demande un rendu à la résolution
réelle de l’écran. Le test automatisé vérifie les 52 SVG utilisés, marque comprise,
à 96 × 96 pixels. Annuler et Rétablir ont été redessinés avec des courbes ouvertes
et des pointes équilibrées le 21 octobre 2026.
La Main reprend la même grille, les contours arrondis et le dégradé bleu/cyan.
Copier, Coller et Ajouter des PDF suivent également cette grille et ces dégradés,
avec des silhouettes distinctes de feuilles, presse-papiers et document avec plus.
Les libellés sont fournis aux technologies d’assistance et dans les infobulles.

L’ancien atlas raster `icons-atlas.png` est un essai de création original généré
avec ImageGen. Il n’est ni utilisé ni embarqué par l’application actuelle.

Les créations propres à PDFinator sont distribuées sous MIT OU Apache-2.0.

Dessin, Trait, Cercle, Carré, Rectangle et Tableau utilisent le même dégradé
violet et la même grille. Les chevrons Haut/Bas suivent les chevrons de
navigation bleu/cyan. Ces SVG originaux ont été créés pour la version 0.5.0.
Le pictogramme Pages de l’ancien bouton de miniatures n’est plus embarqué.

Page blanche (0.5.1) : document bleu clair, coin replié et plus turquoise.
Ce SVG original suit la grille 32 unités et les couleurs de la navigation.
