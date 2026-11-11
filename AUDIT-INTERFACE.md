# Audit fonctionnel et visuel — 11 novembre 2026

Périmètre : version 1.0.7, capture fournie, lecture des commandes et de leurs
chemins de code, tests de document et simulations des gestes de souris. Ce bilan
ne remplace pas une recette avec Word, NVDA, des imprimantes physiques ou une
machine Windows vierge. Les propositions visuelles ne sont pas des bugs reproduits.

## Défaut confirmé et correction

Le glissement d’une miniature vers un autre onglet n’avait aucun destinataire :
le code ne traitait que le réordonnancement du PDF courant. Le dépôt sur un autre
onglet copie maintenant le groupe sélectionné à la fin du PDF d’arrivée. Le
source conserve ses pages et son historique. L’onglet cible est encadré et le
curseur indique la copie ; le PDF d’arrivée s’ouvre sur les pages ajoutées.
Une annulation retire tout l’ajout. Le clic droit des miniatures propose aussi
« Copier vers un autre PDF ». Échap et un dépôt hors des cibles ne copient rien.
Depuis la 1.0.9, maintenir le survol d’un onglet ouvre le PDF cible en gardant
les pages en main. Un repère entre ses miniatures permet l’insertion à une
position choisie. Échap ou le dépôt hors cible annule et revient au source ; les
brouillons et styles du document survolé ne sont pas validés par le survol.

L’aide « ? » utilisait la couleur de texte du thème clair sur la barre sombre :
sa couleur devient blanche pour rendre la commande lisible.

## Améliorations à prioriser

| Priorité | Constat / besoin | Recommandation |
| --- | --- | --- |
| P1 visuel | Les cadres de tous les blocs occupent beaucoup la page dans la capture. | Contour net sur la sélection, discret au survol ; éviter de cadrer en permanence tous les paragraphes. Corrigé en 1.0.9 : découverte et sélection contrôlées par simulation. |
| P1 visuel | Le pictogramme W désigne une commande qui exporte aussi en ODT. | Corrigé en 1.0.9 : pictogramme commun, choix DOCX / ODT conservé. |
| P1 accessibilité | Les croix d’onglets portent le nom générique « Fermer » ; leur info-bulle nomme le document. | Nom accessible « Fermer [document] », puis recette clavier et NVDA. Un nom accessible ne prouve pas à lui seul le fonctionnement avec NVDA. |
| P1 fonctionnel | La conversion Office et l’édition des structures PDF complexes restent limitées au corpus testé. | Ajouter des régressions avec vérification du rendu, Word en complément de LibreOffice ; ne pas promettre une reconstruction parfaite de tout PDF. |
| P1 performances | La lecture de fichiers reste limitée à 256 Mio et les copies de pages s’exécutent dans le fil d’interface. | Mesurer plusieurs centaines de pages et les images lourdes, puis rendre les grosses opérations asynchrones ; éviter de relever la limite sans mesure mémoire. |
| P1 recette | Imprimantes réelles, Word, NVDA, postes verrouillés et mises à l’échelle multiples ne sont pas intégralement validés. | Recette matérielle dédiée, distincte des tests automatisés. |
| P2 pages | Le dépôt sur onglet ajoute en fin, sans choisir immédiatement un interstice. | Corrigé en 1.0.9 : survol, repère et insertion ; Échap et dépôt hors cible vérifiés. |

## Ce que je simplifierais, et ce que je conserverais

- Garder les actions de pages détaillées dans le clic droit plutôt que rajouter
  une rangée de boutons permanents. Le nouveau transfert suit cette règle.
- Alléger d’abord les encadrements permanents ; aucune suppression générale
  d’outils n’est justifiée par leur seule ressemblance.
- Conserver **Main** (déplacer la vue), **Sélectionner** et **Déplacer** (objets).
  Clarifier leurs noms et curseurs ; supprimer la Main casserait un usage distinct.
- Conserver **Enregistrer sous**, **Annuler / Rétablir**, **Page entière** et
  **Adapter à la largeur** : ces actions ont des résultats différents.
- Ne pas ajouter pour l’instant de fonctions IA, pédagogiques ou vocales dans
  cette barre : stabiliser les gestes, la fidélité et l’accessibilité en premier.

## Contrôles du correctif

Les nouvelles simulations utilisent de vrais événements de souris et les bornes
accessibles des commandes. Elles couvrent Lecture / Modifier / Pages, un groupe
non consécutif, le curseur de copie, la destination active, le contenu réouvert,
le source inchangé, l’annulation / rétablissement, Échap, un dépôt hors cible et
le menu contextuel. Les pages importées utilisent les chemins d’extraction et de
concaténation existants ; leurs limites sur les formulaires complexes et les
structures PDF rares restent décrites dans PAGES.md.
