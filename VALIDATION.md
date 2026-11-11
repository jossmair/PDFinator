# Validation de PDFinator

## 1.0.8 — copie des pages entre onglets

La suite compte **210 tests**. Trois nouvelles simulations couvrent le dépôt sur
un onglet dans les trois espaces de travail, un groupe non consécutif, le
curseur de copie, le source et son historique intacts, le contenu réouvert,
l’annulation/rétablissement dans la destination, Échap, le dépôt hors cible et
le menu contextuel. Le dépôt ajoute en fin ; il n’active pas l’onglet au survol.

L’audit de la capture et des commandes est dans [AUDIT-INTERFACE.md](AUDIT-INTERFACE.md).
Ses propositions visuelles et recettes matérielles sont distinguées des
comportements vérifiés ; il ne certifie pas tous les PDF ou matériels Windows.

## 1.0.7 — insertion et stabilité de l'édition

La suite PDFinator compte **207 tests**, avec six nouveaux scénarios ciblés.
Les simulations vérifient le tracé d'un cadre avec les réglages déjà ouverts,
l'absence de création au clic simple, le point central et le focus lors du zoom,
et le placement du curseur au premier clic avant la saisie.

Le corpus natif reproduit un paragraphe dont la dernière partie est peinte après
la ligne suivante. Le test échoue avant le correctif et vérifie ensuite une seule
zone d'édition, la correction du texte, le voisin intact, l'annulation et la
réouverture. Un contrôle conserve les colonnes distantes et les cadres marqués
indépendants. La recherche est limitée aux 32 lignes récentes et garde les règles
d'alignement, de plan et d'espacement existantes.

La récupération conserve la stratégie de regroupement des brouillons de version 1.
Le nouveau format de brouillon, version 2, enregistre cette stratégie. Un test
restaure l'ancien index d'un fragment, son gras et son curseur, puis vérifie les
textes voisins et la réouverture après correction. Un état invalide rétablit
également le modèle précédent avant de proposer le texte de secours.

La suite complète du fournisseur tentée hors ligne requiert un checkout
`craft-docpath` absent du cache. Les tests de regroupement passent par les API
d'édition réellement utilisées par PDFinator. Les vérifications du fournisseur
qui dépendent de ce checkout restent hors du périmètre de cette recette.

Le contrôle RustSec du 11 novembre a signalé `async-executor` 1.14.0 retiré du
registre et l’avis RUSTSEC-2026-0486 sur `rustix` 1.1.5. Le verrouillage utilise
maintenant 1.14.1 et 1.1.6, respectivement. Ces changements sont soumis aux mêmes
contrôles GitHub et aux tests de l’application avant publication.

## 1.0.6 — groupes de pages

**201 tests automatisés**, dont treize nouveaux scénarios : quatre simulations
d'interface et neuf tests de document. Les gestes Ctrl/Maj, le glissement groupé,
le menu contextuel, la sélection par onglet, l'extraction et les raccourcis sont
exercés. Le test existant d'annulation du glissement a détecté un conflit avec le
nouvel Échap de sélection ; la priorité à l'annulation du glissement est rétablie.

Le corpus synthétique vérifie les liens internes explicites, un signet, les
commentaires, un champ AcroForm simple pendant le déplacement, les rotations,
les formats mixtes, la réouverture, l'historique et les refus de modification
des documents signés. Le rendu des copies et extractions est identique pixel par
pixel aux sources dans ce corpus. L'extraction vérifie aussi l'enregistrement
d'une nouvelle copie avec un nom accentué, sans toucher au document source.

Cette recette porte sur les groupes de pages ; elle n'ajoute pas de validation
Microsoft Word, NVDA, imprimante physique ou machine Windows vierge. Les limites
et les commandes sont détaillées dans [PAGES.md](PAGES.md).

## 1.0.5 — curseurs et fractions Office

**188 tests locaux réussis**, formatage et Clippy strict. Les variables des trois
PDF fournis ont été activées pour les tests optionnels d’export. Les scénarios
simulés vérifient la croix lors du déplacement des objets, la main lors du
panoramique du PDF, les poignées diagonales et le curseur normal dans la marge.

Quatorze exports ont été rendus dans LibreOffice et contrôlés indépendamment,
y compris les fractions isolées dont le DOCX sortait blanc. Les mesures et les
limites sont consignées dans [OFFICE-VALIDATION.md](OFFICE-VALIDATION.md).
Le statut GitHub fait foi pour les contrôles distants ; Microsoft Word et les
validations matérielles restent distincts de cette recette locale.

## 1.0.4 — intégrité des sélections

Vérifications locales sous Windows x64 le 28 octobre 2026 : **182 tests réussis**,
formatage et Clippy strict. Les quatre nouvelles régressions portent sur les
flux endommagés, la décompression cumulée, le nombre de flux/opérateurs et la
conservation des flux intacts après déplacement, réouverture et annulation.
Le test du flux endommagé échoue sur le code de sélection de la 1.0.3 et réussit
avec le correctif. Un refus conserve également la possibilité de rétablir.

Le [statut des contrôles GitHub](https://github.com/jossmair/PDFinator/actions)
fait foi pour la validation distante. Les limites de ces protections ciblées
sont décrites dans [SELECTION-SAFETY.md](SELECTION-SAFETY.md).

Les vérifications historiques ci-dessous restent utiles pour leur périmètre ;
elles ne valent pas nouvelle recette Word, imprimantes, lecteurs d’écran ou
installation sur une machine vierge.

## Archive de validation 1.0.0

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
