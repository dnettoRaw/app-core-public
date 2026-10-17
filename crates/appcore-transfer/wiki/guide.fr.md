# Transfert borné et vérifié

Release candidate **`appcore-transfer 1.0.0-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.


La beta `0.1.0-beta.1` transfère des objets opaques immuables depuis des sources explicitement autorisées. Les descripteurs proviennent d’une application ou d’un déploiement de confiance. L’intégrité SHA-256 n’authentifie pas une release.

## Reprise et limites

Une racine privée existante est obligatoire : Unix 0700 du propriétaire, ou ACL Windows réservée au propriétaire. Le stockage conserve un verrou exclusif interprocessus. Les blocs vérifiés survivent au redémarrage ; corruption et versions inconnues échouent explicitement. Les fichiers temporaires orphelins reconnus sont supprimés sous verrou. Le TTL supprime uniquement les objets incomplets. La rétention des objets complets appartient au déploiement.

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

## Transport et responsabilités

Activez peer-rpc pour utiliser PeerBlockSource et TransferDispatcher sur les streams Peer RPC V2 signés existants. Le dépôt autorise tenant, cluster, origine, cible et objet. Les timeouts I/O doivent respecter le délai coopératif du transfert. Les providers respectent l’annulation ; tous les workers sont joints. Découverte et relais appartiennent au déploiement ; Sync contrôle la réplication et Update l’authenticité, l’activation et le rollback.

Windows cross-compilation is checked; native Windows recovery and physical mesh performance remain release qualification work. MCU remains planned. No installation or artifact activation occurs here.

## Création avec token élevé et guards Windows

Cette section décrit des changements de source non publiés. Vérifiez le package exact du registre avant adoption par les consommateurs.

Seuls les nouveaux fichiers/répertoires privés reçoivent le propriétaire utilisateur courant explicite et une DACL protégée avant validation stricte. Les paths existants non sûrs restent refusés. Roots et sessions d’objets Windows conservent les guards Security contre remplacement d’ancêtres/répertoire d’objet pendant le travail. Security dépendance Windows seulement ; `peer-rpc` reste optionnel, aucune dépendance Update. Les tests natifs exigent les tokens limité/élevé ; les fixtures utilisent des paths Temp Windows sans préfixe verbatim. Le path vérifié après drop du store reste protégé par deployment et revérifié à l’ouverture.

## Candidat de publication — 2026-10-14

`0.1.0-beta.2`: modifications propres.

## Politique courante des sources (source non publiée)

`AuthorizedBlockSource`, `TransferSourceScopeV1` et `TransferSourceAuthorizer` ajoutent une frontière explicite de politique au provider choisi. Le scope lie tenant, cluster, Core demandeur et Core fournisseur (profil ASCII de 128 octets) ; le callback reçoit descriptor complet et `TransferControl`. L’implémentation vérifie permission courante objet/redistribution, expiration/révocation et disponibilité de l’autorité dans le délai restant. Autorité absente refuse l’accès. Le discovery reste au déploiement : résoudre annonces/identités Core authentifiées et bornées en endpoints, autoriser le descriptor exact et choisir au plus 16 providers distincts avec scope. Disponibilité annoncée et intégrité SHA-256 n’autorisent pas la redistribution.

Le wrapper vérifie la politique avant chaque requête de bloc et avant acceptation des octets tamponnés par le moteur. Un refus met la source en quarantaine pour cette invocation et sélectionne une autre source choisie, conservant les blocs vérifiés. Si toutes refusent, `transfer` retourne `TransferError::Unauthorized` ; un refus ne compte jamais comme seconde source approuvée. Limites retry/concurrence/storage inchangées. `quarantined_sources` inclut désormais les refus de politique. Peer RPC V2 mappe refus typés d’autorisation/identité vers Unauthorized, hashes stream invalides vers Integrity et transport vers Source, sans logs des détails distants ni changement de protocole.

Utiliser les wrappers via `transfer` : son buffer rejette les octets si la dernière vérification échoue. Un sink direct arbitraire peut déjà avoir reçu des octets avant la seconde vérification. Les checks sont des observations, pas une révocation distante atomique ; une requête déjà autorisée en cours ne peut être empêchée rétroactivement. `TransferRepository::open_authorized` du serveur authentifié reste obligatoire et indépendant de la politique cliente. Aucun fallback implicite endpoint/protocole ni installation.

APIs additives non publiées. `TransferError` est non exhaustif ; un refus auparavant réduit à épuisement générique peut maintenant retourner Unauthorized. Adapter explicitement les diagnostics consommateurs. Formats persistants, wire V2 et `SourceBinding::approved` restent inchangés ; le constructeur inférieur demeure une assertion du demandeur fiable. Exécution ACL Windows limited/elevated et preuve DNET réelle restent une qualification séparée, non prouvée par les tests locaux.

Les checks de source gouvernent les nouvelles lectures provider. Terminer depuis des blocs locaux déjà vérifiés ne contacte ni ne réautorise les providers ; le déploiement autorise séparément l’usage du cache et chaque requête de redistribution côté serveur.
