# Migration depuis appcore-bin

Remplacez la dépendance et les imports de l'application sans recréer l'ancien
host.
La dernière version de `appcore-bin` dans le registre n'est qu'un avis de
retrait. Le nouveau code doit dépendre directement de `appcore-sdk`.

1. Dépendez de `appcore-sdk` et activez uniquement les capabilities utilisées.
2. Importez `Application` et les contrats de registre depuis `appcore_sdk`.
3. Conservez `application.toml`, `deployment.toml` et le code métier.
4. Utilisez `App::prepare` pour valider et collecter les enregistrements.
5. Laissez l'exécutable de déploiement résoudre les providers, appeler
   `prepare_with_deployment`, démarrer les workers et gérer le shutdown.

Il n'existe ni alias de compatibilité pour `appcore_bin`, ni CLI Runtime dans
le SDK, ni sélection implicite de provider. Les opérations de host supprimées
doivent échouer à la frontière du déploiement au lieu d'être déduites.
