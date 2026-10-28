# Modifier les éléments d’un schéma groupé

Dans **Modifier**, faire un clic droit sur un schéma PDF groupé puis choisir
**Dissocier le schéma**. Ses textes, images et tracés compatibles deviennent des
objets séparés. Les traits pris en charge par l’outil Dessin peuvent ensuite être
déplacés et modifiés individuellement.

La commande conserve les instructions vectorielles et les polices du PDF. Elle
ne transforme pas le schéma en image et n’utilise pas l’OCR. Une seule occurrence
est modifiée : les autres utilisations du même schéma restent intactes, y compris
sur d’autres pages. L’opération peut être annulée puis rétablie.

Les enveloppes simples ajoutées lors d’un collage entre PDF sont retirées dans
la même opération. Un groupe interne possédant aussi ses propres textes ou
tracés peut rester groupé et être dissocié séparément.

## Périmètre actuel

La commande est proposée pour les groupes ordinaires à découpe rectangulaire,
avec ressources de police et d’image prises en charge. Le rendu est conservé,
y compris la partie cachée par le cadre de découpe. L’outil et le zoom actifs
restent inchangés.

Les groupes de transparence, structures balisées, ressources graphiques complexes
et cadres cisaillés ou tournés d’un angle oblique restent groupés. L’édition des
traits déformés différemment en largeur et en hauteur reste limitée. Une image
contenant un dessin imprimé reste une image ; ses traits ne deviennent pas des
vecteurs par cette commande.

Les cas refusés ne publient aucune modification partielle. Les flux sont décodés
strictement : 8 Mio pour les flux de page et 8 Mio pour l’ensemble des groupes
dissociés. Les opérateurs sont limités à 100 000 par contenu et les
enveloppes automatiques à 16 niveaux. Les cycles et flux endommagés sont refusés.

## Vérifications

Les tests synthétiques de `tests/forms.rs` comparent le rendu pixel par pixel
avant et après dissociation et vérifient ensuite l’édition réelle d’un trait ou
d’un texte. Ils couvrent les ressources partagées, les polices de groupes
imbriqués, les rotations à 90°, les échelles non uniformes, les opérateurs répartis
sur plusieurs flux, les blocs déjà modifiables, le collage entre PDF,
l’enregistrement/réouverture, l’annulation/rétablissement et les refus atomiques.

Un test d’interface utilise le vrai menu contextuel et vérifie le maintien de
l’outil et du zoom ainsi que la suppression de la sélection devenue obsolète.
