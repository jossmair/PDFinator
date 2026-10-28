# Intégrité des sélections mixtes

Un lecteur PDF peut afficher une partie d’un flux endommagé. Cette tolérance ne
doit pas servir à remplacer le contenu d’origine pendant une modification.
L’inventaire de sélection décode donc strictement les flux de la page avant
d’interroger les autres inventaires. Une erreur refuse la sélection ou
l’opération ; le PDF courant et son historique restent inchangés.

Les sélections mixtes ont un budget de décompression total de **16 Mio par page**,
partagé entre tous ses flux, et au plus **4 096 flux**. Après analyse, les contenus
dépassant **100 000 opérateurs** sont refusés. Le déplacement réutilise les octets
déjà validés pour conserver tels quels les flux qui n’ont pas changé.

Ces limites concernent ce chemin d’édition. Elles ne constituent pas une limite
globale de mémoire du lecteur : l’analyse des opérateurs précède leur comptage,
et les ressources imbriquées suivent aussi les chemins propres au moteur.
L’ouverture pour lecture et les autres outils ne sont pas couverts par ces
seuls contrôles. Un audit de résistance de l’ensemble du moteur reste nécessaire.

## Régressions reproductibles

`tests/selection.rs` vérifie :

- un flux ASCIIHex contenant un tracé valide suivi de données invalides,
  accepté pour l’affichage mais refusé par le décodage strict ; sélection,
  déplacement, suppression et aperçu ne modifient ni le PDF ni l’historique ;
- le partage du budget entre plusieurs petits flux compressés, sans se limiter
  au poids du fichier sur disque ;
- les plafonds du nombre de flux et d’opérateurs ;
- la conservation des octets d’un flux intact après déplacement d’un tracé,
  la réouverture et l’annulation/rétablissement.

Le premier test a été exécuté contre la 1.0.3 : il échouait parce que l’inventaire
acceptait les données endommagées. Les autres tests de sélection continuent à
couvrir les courbes, découpes, textes, commentaires et quatre rotations de page.
