# Changements PDFinator

## 1.0.6 — sélectionner et organiser plusieurs pages

- Sélection par Ctrl, Maj et Ctrl+Maj dans les miniatures des trois espaces ; état indépendant pour chaque onglet et indication accessible des pages sélectionnées.
- Glisser-déposer groupé avec repère d'insertion, aperçu du nombre de pages et maintien de l'ordre d'origine.
- Duplication, rotation et suppression groupées depuis le menu contextuel, avec une seule étape d'annulation et protection de la dernière page.
- Extraction des pages sélectionnées vers un nouvel onglet non enregistré, sans modification du PDF source.
- Ctrl+A, Maj+flèches, Suppr et Échap en mode Pages ; priorité conservée à l'annulation d'un glissement.
- Régressions synthétiques sur liens, signets, commentaires, champ AcroForm simple, formats mixtes, rendu exact, sauvegarde/réouverture et documents signés. Détails dans [PAGES.md](PAGES.md).

## 1.0.5 — curseurs de déplacement et fractions Office

- Croix de déplacement sur les textes, images, tracés et sélections mixtes ; la main reste dédiée au déplacement de la vue PDF. Conservation des curseurs de redimensionnement.
- Reconnaissance des signes moins Unicode et des opérateurs courants dans les fractions, avec conservation du texte.
- Réservation de la largeur de la barre source et exclusion des traits obliques dans la détection des fractions.
- Correction des cellules DOCX ne contenant qu’une équation et rendues blanches par LibreOffice ; formule native conservée.
- Prévention de l’étirement des caractères des fractions ODT lorsque la barre source est plus large.
- Corpus synthétique reproductible et vérification indépendante du rendu, en complément des tests automatisés.

## 1.0.4 — intégrité des sélections

- Refus des flux endommagés lors d’une sélection, d’un déplacement, d’une suppression ou d’un aperçu ; aucun contenu partiellement décodé n’est réécrit.
- Budget de décompression partagé entre les flux de la page et limites sur leur nombre et les opérateurs analysés.
- Conservation exacte des flux intacts pendant un déplacement et maintien de l’historique en cas de refus.
- Quatre régressions synthétiques supplémentaires ; périmètre détaillé dans `SELECTION-SAFETY.md`.

## 1.0.3 — dissocier les schémas PDF compatibles

- Commande « Dissocier le schéma » dans le menu contextuel du mode Modifier.
- Conservation du rendu, des polices, de l’ordre de dessin et des autres
  occurrences du schéma ; retrait des enveloppes simples de collage en une action.
- Tests de rendu exact, d’édition des éléments extraits, de sauvegarde/réouverture,
  d’annulation/rétablissement et de refus des flux endommagés ou trop complexes.
- Périmètre et limites documentés dans `GROUPS.md`.

## 1.0.2 — indices et exposants dans les exports Office

- Mise à jour ciblée de clipboard-win 5.4.1 (retirée du catalogue) vers 5.4.3 ;
  blocage des dépendances retirées dans le contrôle GitHub.
- Conservation de la taille des indices et exposants natifs du PDF, avec leur
  décalage de ligne de base, sans seconde réduction de taille à l’export DOCX/ODT.
- Régression synthétique exécutée sans fichier utilisateur ; corpus reproductible
  pour les formules et les documents alternant formats de page et pages blanches.
- Procédure de vérification des rendus dans `OFFICE-VALIDATION.md`.

## 1.0.1 — récupération complète et contrôles

- Récupération des blocs non validés avec styles de caractères, polices en mémoire,
  réglages de paragraphe, position, sélection et curseur ; commentaires et onglets.
- Écriture des PDF de récupération par générations avec publication atomique de
  l’état associé ; maintien du dernier checkpoint complet si le remplacement échoue.
- Compatibilité de lecture des anciens journaux texte et secours conservé en cas
  d’état inconnu. Un échec d’ouverture ne transfère pas le brouillon au document suivant.
- Actions GitHub pour formatage, Clippy strict, tests Windows, reprise entre
  processus et audit RustSec ; nouvelles régressions synthétiques.
- Mise à jour ciblée Tokio 1.54.0 vers 1.54.1 dans Cargo.lock pour RUSTSEC-2026-0384.
  Le graphe Windows normal vérifié ne dépend pas de Tokio ; aucun exploit de l’EXE
  n’est affirmé. Les avis de maintenance, dont ttf-parser dans la cible Windows,
  restent suivis dans DEPENDENCIES.md.

Les téléchargements 1.0.0 restent disponibles. La version 1.0.1 apporte ces correctifs.
Les anciens journaux ne contenant que le texte restent lisibles, mais leurs styles
non enregistrés ne peuvent pas être reconstruits rétroactivement.

## PDFinator 1.0.0

Application Windows x64 de lecture, annotation et modification de PDF, avec OCR
local et export en DOCX ou ODT. Utilisation hors ligne, sans compte.

## Installation

- **Portable** : extraire entièrement `PDFinator-1.0.0-portable-exe-windows-x64.zip`
  puis ouvrir `PDFinator.exe`. Garder `LICENCES.txt` avec le programme.
- **Installable** : ouvrir `PDFinator-1.0.0-Setup.exe`. Installation pour le compte
  Windows actuel, sans téléchargement supplémentaire.
- Vérifier les empreintes avec le fichier `SHA256SUMS.txt` joint à la publication.

## Correctifs de préparation V1

- Annuler une fermeture pendant une sauvegarde ne programme plus la fermeture
  de la fenêtre ou de l’onglet à la fin de cette sauvegarde.
- Une fermeture demandée attend la fin d’un export ou des autres opérations
  suivies. Un bouton permet d’annuler cette attente de fermeture.
- Une erreur d’export ou de sauvegarde conserve la fenêtre ouverte et son message.
- La protection contre les modifications externes reconnaît les alias de chemin
  canoniques, notamment un changement de casse sous Windows.
- L’identité du fichier est actualisée après un enregistrement sous un autre nom,
  pour préserver la détection des onglets déjà ouverts.
- Le compteur de pages affiche zéro quand aucun document n’est ouvert.

## Fonctions

Lecture, recherche, main et zoom ; surlignage et commentaires ; modification et
insertion de texte ; polices, taille, styles, puces et interlignes ; images avec
menu contextuel et rognage ; OCR d’images créant des blocs de texte éditables ;
dessins et tracés ; sélection et déplacement d’objets ; copier/coller entre PDF ;
ajout, rotation et réorganisation des pages ; onglets déplaçables ; annuler et
rétablir ; récupération après interruption ; export DOCX/ODT ; impression avec
choix des pages.

## Limites connues

- Windows 10/11 x64. L’OCR dépend des langues de reconnaissance installées dans
  Windows et de la qualité des images ; une relecture reste nécessaire.
- La conversion Office reconstruit du contenu éditable. Des différences de
  polices, métriques ou disposition restent possibles selon le PDF et le lecteur
  Office ; une identité parfaite pour tous les documents n’est pas garantie.
- Les illustrations exportées peuvent être conservées sous forme d’images.
- Les fichiers de plus de 256 Mio ne sont pas ouverts par cette version.
- Les éléments PDF complexes ou découpés ne sont pas tous modifiables comme un
  dessin créé dans PDFinator. Les permissions et signatures du PDF sont respectées.
- La récupération des brouillons non validés conserve le texte, mais ne rétablit
  pas automatiquement tous les réglages de mise en forme non enregistrés.
- Les exécutables ne possèdent pas de signature Authenticode.

Les tests automatisés et les vérifications de fichiers ne remplacent pas la
validation de chaque pilote d’imprimante, configuration Windows et document PDF.
