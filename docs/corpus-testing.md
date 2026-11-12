# Tester un corpus privé de PDF

Le programme de développement `corpus_probe` utilise les moteurs réels de
PDFinator. Il travaille en mémoire et écrit ses résultats dans le dossier
fourni. Utiliser un dossier de sortie neuf, distinct des documents originaux.
Ne jamais ajouter les documents privés ni leurs résultats au dépôt public.

```powershell
cargo build --locked --example corpus_probe
target/debug/examples/corpus_probe.exe exercise document.pdf test-output/case-001
target/debug/examples/corpus_probe.exe ocr document.pdf test-output/case-001 0
target/debug/examples/corpus_probe.exe docx document.pdf test-output/case-001
target/debug/examples/corpus_probe.exe odt document.pdf test-output/case-001
```

`exercise` examine la première page, la page centrale et la dernière page,
rend la première et la dernière, puis essaie une modification de texte et
deux déplacements. Chaque opération contrôle la réouverture, l'annulation et
le rétablissement par comparaison des octets. Un seul objet admissible est
essayé pour chaque type de modification : ce contrôle ne couvre pas tous les
objets d'un document. Examiner les statuts imbriqués, même si le statut général
est `ok`.

`ocr` reconnaît un rendu de la page demandée, à une définition plafonnée. Cela
ne vérifie pas à lui seul la conversion d'une image incorporée en blocs ni
l'exactitude des mots reconnus.

Les exports sont complets jusqu'à vingt pages. Au-delà, seules la première
page, la page centrale et la dernière page sont converties. Le JSON indique
explicitement cet échantillonnage. Faire ouvrir les exports par une application
Office indépendante, vérifier leur pagination et comparer les rendus aux PDF.
Une création de fichier réussie ne prouve pas sa fidélité.

Exécuter chaque document dans un processus distinct, avec une durée et une
concurrence bornées. Écrire les rapports JSON atomiquement. Ne pas lancer deux
campagnes dans le même dossier de résultats. Conserver les refus de modification
dans le bilan ; ils ne constituent pas une modification réussie.

Les tests synthétiques `white_slide_background_does_not_flatten_text_and_duplicate_tables`,
`centered_column_titles_keep_wider_tables_in_their_columns` et
`full_page_images_export_without_extra_page_carriers` reproduisent des classes
de problèmes sans distribuer de PDF privés. Le dernier produit deux exports
de deux pages à contrôler dans LibreOffice ou Word ; son succès Rust seul ne
vérifie pas la pagination dans ces logiciels.
