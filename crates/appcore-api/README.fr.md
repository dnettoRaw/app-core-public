# appcore-api

L'entrée command/query admet au plus 16 requêtes par hôte avant de collecter
ou décoder les corps. Les clones du routeur partagent cette limite, contrairement
aux hôtes construits séparément. La saturation renvoie HTTP 503 sans file
d'attente ; la réception a un délai de 10 secondes (408), et les corps trop
volumineux renvoient 413. Chaque corps respecte `max_payload_bytes` : les octets
bruts sont bornés à 16 fois cette valeur, pas le RSS total. Les objets décodés,
réponses et allocations du transport s'y ajoutent. Health/status contournent
cette limite. La limite distincte de dispatch bloquant par processus demeure.

Tests locaux:

```bash
cargo test -p appcore-api
```

**Responsabilité :** host HTTP command/query/status et DTOs de transport.

**Dépendances internes :** `appcore-core`, `appcore-security` et
`appcore-supervisor`.

**API principale :** `CommandRequest`/`CommandResponse`,
`QueryRequest`/`QueryResponse`, erreurs de validation, `CommandEndpoint`,
`QueryEndpoint`, `ApiRouter`, `ApiRequest`/`ApiResponse`, `RuntimeHttpHost`,
`HttpApiConfig`, statut statique, policy capability pour commands et queries
applicatives, vérification token et vue du sync log.

À utiliser pour les routes Runtime et queries applicatives enregistrées. Ne pas
ajouter resources REST produit ou schémas métier. Les nouvelles applications
utilisent les contrats via `appcore-sdk`; la composition HTTP est explicite.

Les queries applicatives sont autorisées par la policy capability composée
avant le router. Les queries de statut Runtime restent hors du catalogue
applicatif.

Le dispatch command/query partage 16 slots blocking dans le processus. Le pool
Tokio applique le même plafond, des stacks de 1 Mio et retire les threads
inactives après cinq secondes. La saturation renvoie HTTP 503 avant la file.

Les hosts Runtime gèlent l'enregistrement des queries de `ApiRouter` après le
bootstrap. Les clones du router partagent les endpoints via `Arc` ; la façade
directe, le HTTP et le peer RPC libèrent le mutex d'état du host avant d'appeler
l'endpoint, ce qui permet l'exécution concurrente de queries indépendantes.
`query_names_iter` expose le registre gelé par référence pour la validation ;
`query_names` reste l'API owned déterministe aux frontières de sortie. La
composition root emploie la vue empruntée : un manifest valide ne clone ni ne
trie le catalogue complet des queries.

Le `ReloadableRuntimeHttpHost`, opt-in de `1.0.2-rc`,
conserve un listener pendant la validation de santé et la commutation atomique
des générations de routing. Les requêtes déjà admises terminent sur leur ancien
router; la génération précédente est drainée sous délai. Un échec de prepare,
du health gate après commutation ou du drain conserve ou restaure la génération
précédente. Les générations sont monotones, les reloads sont sérialisés et les
snapshots ne contiennent que des compteurs bornés. Le owner conserve au plus
une génération active et une en drain ; une génération défaillante bloque un
autre reload jusqu'à la libération de sa dernière requête. Les snapshots sans
payload exposent l'admission et l'in-flight actif/en drain sans conserver
d'historique. Une annulation après la commutation restaure synchroniquement la
génération précédente. Un changement d'adresse échoue explicitement et exige
une génération de listener préparée par la composition root.
`RuntimeHttpHost` reste inchangé.

Les composition roots qui doivent valider le bind avant le démarrage peuvent
appeler `run_on_listener_until_shutdown` avec un listener TCP déjà lié. Le host
en prend possession et le shutdown reste gracieux.

Lorsqu'il est composé avec `appcore-sync 1.0.2-rc`,
`SyncLogView::len` et `is_empty` sont faillibles. Le status JSON privé retourne
`sync_log_len: null` avec
`sync_log_observation_ok: false` lorsque la persistance active ne peut pas être
observée ; il ne substitue jamais un ancien compteur statique.

La query intégrée `runtime.audit` plafonne `limit` à 1 000. Elle capture des
snapshots partagés des enregistrements et entrées sous des locks courts, puis
matérialise la page récente demandée après leur libération ; elle ne clone
jamais en profondeur les files complètes de 10 000 éléments pour une réponse
bornée.

`runtime.events` suit la même règle : elle emprunte au plus les 1 000 événements
les plus récents d'un snapshot partagé après libération des locks du host et de
l'event bus. Le format de réponse reste inchangé et continue d'omettre les
payloads opaques.

La limite configurée s'applique au corps HTTP complet avant la
désérialisation JSON par Axum. Les routes protégées acceptent exactement un
header bearer `Authorization` bien formé; les doublons échouent fermés.

Le host TCP intégré ferme une connexion après 10 secondes sans progression de
lecture, y compris lorsqu'un client s'arrête pendant les headers HTTP. Comme
aucune requête n'existe avant la fin des headers, ce cas ferme le socket au
lieu de renvoyer un statut HTTP. Une requête déjà formée dont le corps s'arrête
reçoit toujours HTTP 408 du middleware d'entrée.

`QueryRequest::validate` mesure le JSON structuré avec un writer compteur
borné, sans allouer une copie encodée complète. La limite V1 exacte et la
méthode compatible `payload_bytes()` restent inchangées ; le HTTP ne valide
qu'une fois avant le dispatch blocking.

Le router possède un seul `RuntimeStaticInfo` immuable partagé ; cloner l'état
de la requête ne copie ni les listes de peers, ni les seeds DNS, ni les paths ou
les chaînes d'identité. Le dispatch blocking prend possession des requêtes
command/query. L'audit query ne conserve que l'ID et le nom bornés pendant le
traitement du payload.
Les chemins command owned utilisent `CommandRequest::into_envelope`, qui valide
les mêmes champs V1 et transfère l'allocation du payload dans `CommandEnvelope`
sans copier ses octets. `to_envelope` reste disponible aux callers empruntés.

`CommandTokenVerifier` possède aussi des méthodes additives pour les requêtes
empruntées. Leurs defaults matérialisent `RequestValidationDetails` et appellent
les méthodes owned existantes, donc les verifiers existants gardent leur
comportement. Le verifier du Runtime les surcharge pour hasher directement le
texte ou le JSON structuré, sans copie owned du payload.

`HttpCommandAuth::default()` exige l'authentification et échoue fermé tant
qu'aucun vérificateur de token n'est configuré ; `HttpCommandAuth::required`
en installe un explicitement. `insecure_local_for_testing()` n'existe que dans
les tests du crate ou les builds debug avec `insecure-testing`, et les hosts
intégrés refusent cette policy sur un listener non-loopback. Un reload ne peut
pas changer la frontière d'authentification. `/v1/health` reste public par
contrat mais ne renvoie que `status` ; les détails du Supervisor restent
authentifiés. Les refus command sont audités sans credentials, payload ni clé
d'idempotence. Le TLS entrant reste une frontière du deployment.

**Maturité :** surface HTTP V1 RC stricte et stable.

## Documentation stable

Identifiant stable : **ACR-009**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-009). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
