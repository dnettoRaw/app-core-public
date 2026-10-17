# Migration et récupération Update du host

Source non publiée ; API de récupération additive et admission automatique plus stricte. Aucun changement rétroactif d’un paquet publié. Choisir une future version immuable contenant les modifications, adapter les matches exhaustifs `UpdateError` lors de la migration explicite 2.x et conserver une résolution registry-only.

## Gates de cutover

1. Inventorier build/paquet installé, activation V1 en attente, journal host, fence/reçu coordinateur, snapshot et instance. Suspendre installations et drainer les écritures sous locks installation/storage. Résoudre les ambiguïtés par observation du host ; ne pas réinterpréter une phase locale `prepared` comme `Prepared` Runtime V2.
2. Conserver ancien exécutable/configuration, sauvegarde vérifiée et journaux. Résoudre/archiver l’ancienne tentative avant de choisir une racine V2 séparée. Une autorité lifecycle par couche ; ni installateurs anciens/nouveaux concurrents ni conversion automatique des chaînes de journaux. La migration des schemas reste applicative.
3. Lier attempt Runtime et `host_binding` à l’installation, fence coordinateur, instance et snapshot originaux. Le binding opaque borné peut identifier un enregistrement fiable du host, sans identifiants secrets. Stage uniquement d’artefacts authentifiés/compatibles, même application/canal/target, version croissante et build/digest distincts. Une migration de canal exige sa procédure explicite séparée.
4. Persister intention, drain/snapshot et fermeture des écritures avant activation. Après réponse perdue, observer octets/signature et tentative/instance/snapshot exacts sous le même lock. Seule une preuve active exacte peut appeler `reconcile_activated` ; toute ambiguïté reste bloquée/manuelle. Ensuite health réel et commit explicite. Consulter le reçu coordinateur original après report perdu, sans réserver une autre tentative pour masquer l’incertitude.
5. Conserver artefacts/données de rollback jusqu’au health et à une finalisation récupérable. Réobserver l’identité restaurée avant `RecordRolledBack`. Après acceptation de nouvelles écritures, restaurer un ancien snapshot peut les perdre : cela exige un plan de données compatible applicatif, pas un rollback automatique de l’ancien journal.

## Adapters séparés

| Host | Preuve observée requise | Frontière de migration |
| --- | --- | --- |
| Desktop | Paquet/build installé, tentative, installation/snapshot, processus et renderer prêts | Conserver handoff de plateforme, UI candidate, annulation et identité successeur ; sortie processus ou HTTP health seul insuffisant |
| Service binaire / déploiement systemd | Exécutable/build vérifié, instance supervisée, propriété exclusive service/données, heartbeat/readiness | Conserver parent/child, stop/start bornés et drain d’orphelins ; unit active ne prouve pas le build exact |
| Déploiement container | Digest image immuable, identité container, opération de copie/volume candidat, gate fermé | Conserver frontière committed/released et volumes ; répéter les reports finaux sans restaurer d’anciennes données après réouverture des écritures |

Les adapters restent au déploiement ; Runtime n’importe ni installateur, ni gestionnaire de processus, ni client container, ni schema applicatif. Reçu Runtime V2, journal host et reçu distant décrivent des faits différents. `adapter.commit` s’exécute avant le reçu terminal Runtime ; entrer dans ce callback n’autorise pas la réouverture des écritures. Garder finalisation distante et point irréversible de libération explicites par profil, sans mapper toutes les phases dans un enum unique.

## Qualification et rollback

Qualifier troncature/annulation, signature invalide, disque plein, interruption installation/boot/commit et réponses finales perdues. Vérifier identité exacte, écritures bloquées en cas d’incertitude, health avant finalisation et rollback conservant les données. Les adapters simulés locaux ne qualifient que le contrat ; desktop natif, service Linux et container réel ont leurs gates séparés.

Le rollback de migration arrête le candidat puis restaure un ensemble exécutable/journal/données antérieur compatible sous propriété exclusive après inspection du host réel. Ne pas mêler preuve V2 et processus 1.x, supprimer un reçu en attente, réutiliser attempt/fence ni redémarrer l’ancien writer pour contourner la récupération. Une preuve insuffisante maintient les écritures bloquées pour revue manuelle.
