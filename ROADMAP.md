# Feuille de route PDFinator

Ces priorités décrivent le travail de développement, pas la gravité de bugs tous
reproduits. Un point n’est validé que dans le périmètre effectivement testé.
La branche `main` contient déjà les sources ; la V1 reste figée sous le tag
`v1.0.0-sources` et ses téléchargements existants sont conservés.

## Lot 1 — Brouillons et contrôles GitHub

- Restaurer le bloc en cours d’édition à son emplacement, avec styles de
  caractères, polices choisies, couleurs, alignement et interligne.
- Conserver le texte d’origine, la sélection, la saisie à venir et la position
  du curseur ; conserver les commentaires non validés et les états par onglet.
- Publier atomiquement le couple PDF/état du brouillon : une interruption ne doit
  pas associer un PDF récent aux styles d’un ancien brouillon.
- Conserver la compatibilité de lecture des anciens journaux texte ; récupérer
  le PDF et proposer le texte de secours si un état ne peut pas être restauré.
- Exécuter formatage, Clippy strict, tests Windows et reprise après arrêt brutal
  dans GitHub Actions, et contrôler Cargo.lock avec RustSec.
- Publier les règles de contribution, le périmètre des tests et la procédure de
  signalement confidentiel des problèmes de sécurité.

## Lot 2 — Fidélité du document (P0)

- Étendre le corpus synthétique : paragraphes fragmentés, groupes imbriqués,
  découpes, matrices, ordre de superposition et objets vectoriels complexes.
- Comparer les exports DOCX/ODT sur les dispositions, polices, fractions,
  tableaux et images ; ajouter une régression à chaque défaut reproduit.
- Compléter les vérifications LibreOffice par des vérifications Microsoft Word.
  La fidélité parfaite sur tous les PDF n’est pas une promesse de la V1.

## Lot 3 — OCR, pages et ergonomie (P1)

- Renforcer le prétraitement local, l’inclinaison, les petits labels, les accents,
  les tableaux et les expressions mathématiques. Conserver le moteur Windows
  tant qu’un corpus ne démontre pas un meilleur résultat d’un autre moteur local.
- Clarifier l’absence des langues OCR ; OCR de zones/pages multiples avec
  progression et annulation ; éviter les doubles couches de texte.
- Affiner les sélections, poignées, menus contextuels, repères et déplacements
  au clavier sans multiplier les réglages permanents.
- Préserver liens, signets, annotations et formulaires lors des mouvements de
  pages ; formats mixtes et gros documents ; extraction et transferts entre PDF.

## Lot 4 — Accessibilité et déploiement en collège (P1)

- Navigation au clavier et focus visible, noms accessibles, contraste, grandes
  tailles de texte et zoom de l’interface indépendant du PDF.
- Essais avec NVDA, Claviernator et plusieurs écrans/échelles Windows.
- Installation et mise à niveau en VM vierge ; portable sur clé USB et poste
  verrouillé ; comportement avec les restrictions et antivirus des établissements.
- Essais sur imprimantes réelles, plusieurs pilotes, recto verso et couleurs.
  Ces validations matérielles ne sont pas remplacées par les tests simulés.

## Lot 5 — Ressources et sécurité (P0/P1 selon les régressions)

- Encadrer mémoire, décodages, décompressions et récursion ; tester fichiers
  tronqués ou malformés et consolider les tests de résistance/fuzzing.
- Étudier le dépassement de 256 Mio à partir de mesures sur une machine modeste,
  sans simplement relever la limite au risque d’épuiser la mémoire.
- Auditer les dépendances et les changements vendored ; conserver le détail des
  versions et licences. RustSec n’est pas un audit du code local du moteur.
- Documenter les données temporaires et la récupération, permettre leur nettoyage
  volontaire et limiter les diagnostics à des journaux locaux non sensibles.

## Ensuite (P2)

Signature Authenticode si un certificat approprié est disponible ; fonctions de
caviardage définitif, compression, extraction d’images, pagination, filigranes,
fractionnement, comparaison et formulaires, après stabilisation du cœur.
Les fonctions vocales, de correction IA et pédagogiques restent du ressort des
autres outils Ergonatus plutôt que d’alourdir PDFinator.

## Avancées vérifiées

- 1.0.1 : récupération des brouillons avec leur mise en forme, reprise entre processus et contrôles GitHub.
- 1.0.2 : conservation des indices/exposants dans les exports Office et régressions synthétiques. La reconstruction générale des documents reste à améliorer.
- 1.0.3 : dissociation de groupes PDF compatibles, avec limites explicites et vérification du rendu.
- 1.0.4 : intégrité des sélections mixtes, refus de flux endommagés, budget de décompression partagé et conservation des flux intacts. Voir `SELECTION-SAFETY.md` pour les limites du contrôle.

Ces avancées couvrent des cas précis ; les autres points des lots restent ouverts.

## Validation suivie

| Environnement | État de référence |
| --- | --- |
| Windows local, Rust 1.99.1 | 1.0.4 : 182 tests locaux réussis ; V1 initiale : 158. |
| Exports | Six rendus LibreOffice comparés sur trois documents lors de la V1. |
| Word, imprimantes, NVDA, VM vierge | Recette dédiée encore nécessaire. |
| GitHub Actions | Le statut des exécutions réelles fait foi, pas la seule présence du YAML. |

Les numéros 11–136 de la proposition initiale servent de repères de travail ;
ils ne sont pas des numéros d’Issues et ne sont pas tous marqués comme réalisés.
