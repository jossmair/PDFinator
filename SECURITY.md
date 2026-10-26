# Signaler un problème de sécurité

Pour un fichier susceptible d’exposer une corruption mémoire, une fuite ou une
écriture non autorisée, **ne publiez pas le PDF ni les détails d’exploitation dans
une Issue publique**. Utilisez « Report a vulnerability » dans l’onglet Security du
dépôt si cette fonction est disponible. Sinon, ouvrez seulement une Issue demandant
au mainteneur un canal privé, sans document, données personnelles ni démonstration.
Ne supposez pas qu’une adresse privée ou un délai de réponse est garanti.

Précisez la version de PDFinator, Windows, les étapes et l’impact observé. Un
document synthétique minimal est préférable à un document personnel. Ne joignez
jamais mots de passe, clés, certificats privés ou données d’élèves.

## Contrôles et limites

- Cargo.lock est versionné ; le contrôle RustSec consulte les avis actuels sans
  ignorer les vulnérabilités automatiquement.
- Les actions tierces du workflow sont figées par SHA ; les travaux s’exécutent
  sur des runners hébergés, avec `contents: read` et sans persistance des identifiants
  de checkout. Les contributions externes ne reçoivent aucun secret du projet.
- Les correctifs et bibliothèques de `vendor/` demandent aussi une revue : un audit
  de Cargo.lock ne prouve pas l’absence de vulnérabilité dans leur code.
- Les exécutables V1 ne sont pas signés Authenticode et la robustesse face à tous
  les PDF hostiles n’est pas certifiée. Une version de développement corrigée ne
  signifie pas que les anciens exécutables ont déjà été remplacés.

Les journaux de récupération sont locaux et peuvent contenir le document, le texte
non enregistré et les programmes de police nécessaires à sa restauration. Ils ne
sont pas envoyés par les contrôles GitHub. Ne les joignez pas à un rapport public.
