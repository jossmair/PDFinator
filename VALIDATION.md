# Validation de PDFinator 1.0.0

Vérifications effectuées sous Windows x64 le 25 octobre 2026.

## Contrôles réussis

- **158 tests automatisés**, sans échec : moteur PDF, sauvegarde/réouverture,
  sélection, texte, images, tracés, pages, interface egui simulée, OCR et export.
- Analyse Clippy stricte des cibles du projet. Des avertissements restent dans
  une dépendance tierce du chemin d’impression.
- Cinq tests d’export exécutés avec les fichiers PDF réels configurés, en plus
  de la passe générale.
- Six exports Office ouverts dans LibreOffice, rendus en PDF et comparés aux
  originaux sur trois mises en page : tableaux/fractions, figures géométriques,
  colonnes et illustrations. Les six sorties restent sur la page attendue.
- OCR Windows avec accents français ; conversion d’un schéma réel contenant
  aussi du texte PDF existant, en conservant ce dernier.
- Récupération de deux pages et d’un brouillon Unicode dans un nouveau processus
  après arrêt du processus initial sans nettoyage.

Les régressions ajoutées vérifient notamment l’annulation pendant une sauvegarde,
l’attente de fin d’export avant fermeture, le maintien de la fenêtre après une
erreur d’export, les conflits de sauvegarde via un alias de chemin, la réouverture
après changement de nom et le compteur sans document.

## Portée

Les tests d’interface automatisés comprennent des événements egui simulés ; ils
ne représentent pas tous des interactions de bout en bout dans Windows. Les
comparaisons Office utilisent LibreOffice, pas Microsoft Word. Elles mesurent
des positions et vérifient la structure éditable et les fractions, sans garantir
une identité parfaite pour tous les PDF.

Aucun travail d’impression papier n’a été envoyé. Le test d’installation est
isolé dans un dossier temporaire, sans remplacer l’installation de l’utilisateur.
Une installation standard en VM vierge, tous les pilotes d’imprimante, toutes les
échelles d’affichage Windows et un corpus exhaustif de PDF ne sont pas couverts.
Les exécutables ne sont pas signés Authenticode. Aucun audit récent des avis de
vulnérabilités des dépendances n’est revendiqué.

Les archives diffusées conservent les mentions légales requises. Les documents
utilisés pour les tests et les données d’authentification ne sont pas publiés.
