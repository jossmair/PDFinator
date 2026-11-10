# Travailler sur plusieurs pages

Dans les miniatures de Lecture, Modifier ou Pages :

- Un clic sélectionne une page.
- **Ctrl + clic** ajoute ou retire une page de la sélection.
- **Maj + clic** sélectionne une plage depuis la dernière page choisie.
- **Ctrl + Maj + clic** ajoute cette plage à la sélection existante.
- Les cadres bleus indiquent les pages sélectionnées.

Glissez une miniature sélectionnée : les pages sélectionnées se déplacent
ensemble, dans leur ordre d'origine. Le repère d'insertion indique leur place ;
l'aperçu indique leur nombre. Glisser une page extérieure à la sélection repart
de cette page seule. Échap annule un glissement.

Le clic droit conserve le groupe si la page en fait partie, sinon il sélectionne
uniquement la page visée. Le menu propose duplication, suppression, rotation,
déplacement et **Extraire vers un nouveau PDF**. Une page blanche s'insère après
la page visée, même quand plusieurs pages sont sélectionnées.

Les copies sont insérées ensemble après la dernière page sélectionnée. Les
déplacements numériques désignent la position de la première page du groupe
dans le document obtenu. Chaque opération groupée s'annule en une seule fois.
Après annulation ou rétablissement, la sélection revient à la page active pour
éviter de réutiliser des indices qui désigneraient d'autres pages.

En mode Pages, **Ctrl+A** sélectionne toutes les pages, **Maj+flèche gauche/droite**
étend une plage, **Échap** revient à la page active et **Suppr** supprime la
sélection. Le document conserve toujours au moins une page. Les raccourcis ne
suppriment pas de pages pendant la saisie dans un champ de texte.

L'extraction ouvre un nouvel onglet non enregistré ; **Ctrl+S** permet de choisir
son fichier. Le document source et son historique restent intacts. La sélection
de pages est indépendante pour chaque onglet pendant la session ; elle n'est pas
enregistrée dans les brouillons de récupération. Les permissions PDF de copie
s'appliquent à l'extraction ; les mutations respectent les permissions et la
confirmation de modification des documents signés.

## Périmètre vérifié

Les tests synthétiques couvrent les pages non consécutives, les rotations et
formats mixtes, les liens internes explicites, un signet, les commentaires, un
champ AcroForm simple lors du déplacement, les copies indépendantes, la
réouverture et l'historique. Les rendus des copies et extractions sont comparés
pixel par pixel à leurs sources. Cela ne valide pas toutes les structures de
formulaires, actions JavaScript, portfolios, liens externes ou documents balisés.
Les transferts de pages par glisser-déposer entre deux PDF restent à développer.
