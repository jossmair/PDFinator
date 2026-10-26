# Changements PDFinator

## 1.0.2 — indices et exposants dans les exports Office

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
