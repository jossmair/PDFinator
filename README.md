# PDFinator

PDFinator est une application Windows x64 de lecture et modification de PDF.
Elle fonctionne hors ligne, sans compte et sans téléchargement pendant son utilisation.

**[Télécharger la dernière version](https://github.com/jossmair/PDFinator/releases/latest)**

## Télécharger et lancer

- **Version portable** : télécharger le ZIP, l’extraire entièrement, puis lancer
  `PDFinator.exe`. Ce fichier est un véritable exécutable Windows natif.
- **Version installable** : télécharger le fichier `PDFinator-1.0.0-Setup.exe`, puis
  suivre l’assistant en français. L’installation concerne le compte Windows actuel.

Ne lancez pas l’exécutable directement depuis le ZIP. Gardez `LICENCES.txt` avec
la version portable. Le fichier `SHA256SUMS.txt` permet de vérifier les téléchargements.

## Fonctions principales

- Lecture, recherche, sélection de texte, main et zoom.
- Surlignage et commentaires.
- Modification de texte, polices et styles ; insertion et déplacement de blocs.
- Images : copier, couper, coller et rogner par menu contextuel.
- OCR local créant des blocs de texte modifiables à partir des images.
- Dessins, tracés, formes et tableaux.
- Réorganisation des pages et des onglets par glisser-déposer ; ajout de PDF.
- Export de copies éditables en DOCX ou ODT.
- Impression avec choix des pages ; sauvegarde et récupération après interruption.

## Compatibilité et limites

Windows 10/11 x64. L’OCR nécessite une langue de reconnaissance disponible dans
Windows. La fidélité et l’éditabilité dépendent de la structure du PDF, des polices,
de la qualité du scan et du logiciel Office utilisé. Les résultats OCR et les
documents convertis doivent être relus. Les fichiers dépassant 256 Mio ne sont
pas ouverts par cette version. Les exécutables ne sont pas signés Authenticode.

[Notes de version et limites détaillées](CHANGELOG.md) · [Validation](VALIDATION.md)

Ce dépôt distribue les versions Windows et leur documentation. Les mentions
légales des composants sont fournies dans chaque téléchargement. Aucun document
PDF utilisé pendant les essais n’est distribué dans ce dépôt.

Pour signaler un problème, utilisez les [Issues](https://github.com/jossmair/PDFinator/issues).
Précisez la version, les étapes et le résultat attendu ; ne publiez pas de document
contenant des informations personnelles.
