# appcore-transfer

La beta `0.1.0-beta.1` transfère des objets opaques immuables depuis des sources explicitement autorisées. Les descripteurs proviennent d’une application ou d’un déploiement de confiance. L’intégrité SHA-256 n’authentifie pas une release.

[Guide](wiki/guide.fr.md) · [Basic](wiki/examples/basic.fr.md) · [Intermediate](wiki/examples/intermediate.fr.md)

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

Activez peer-rpc pour utiliser PeerBlockSource et TransferDispatcher sur les streams Peer RPC V2 signés existants. Le dépôt autorise tenant, cluster, origine, cible et objet. Les timeouts I/O doivent respecter le délai coopératif du transfert. Les providers respectent l’annulation ; tous les workers sont joints. Découverte et relais appartiennent au déploiement ; Sync contrôle la réplication et Update l’authenticité, l’activation et le rollback.

## Création avec token élevé et guards Windows

Cette section décrit des changements de source non publiés. Vérifiez le package exact du registre avant adoption par les consommateurs.

Seuls les nouveaux fichiers/répertoires privés reçoivent le propriétaire utilisateur courant explicite et une DACL protégée avant validation stricte. Les paths existants non sûrs restent refusés. Roots et sessions d’objets Windows conservent les guards Security contre remplacement d’ancêtres/répertoire d’objet pendant le travail. Security dépendance Windows seulement ; `peer-rpc` reste optionnel, aucune dépendance Update. Les tests natifs exigent les tokens limité/élevé ; les fixtures utilisent des paths Temp Windows sans préfixe verbatim. Le path vérifié après drop du store reste protégé par deployment et revérifié à l’ouverture.

## Candidat de publication — 2026-10-14

`0.1.0-beta.2`: modifications propres.
