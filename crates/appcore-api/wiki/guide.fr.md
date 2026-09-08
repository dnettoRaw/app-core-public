# appcore-api

L'entrée command/query admet au plus 16 requêtes par hôte avant de collecter
ou décoder les corps. Les clones du routeur partagent cette limite, contrairement
aux hôtes construits séparément. La saturation renvoie HTTP 503 sans file
d'attente ; la réception a un délai de 10 secondes (408), et les corps trop
volumineux renvoient 413. Chaque corps respecte `max_payload_bytes` : les octets
bruts sont bornés à 16 fois cette valeur, pas le RSS total. Les objets décodés,
réponses et allocations du transport s'y ajoutent. Health/status contournent
cette limite. La limite distincte de dispatch bloquant par processus demeure.

Les observations de `appcore-sync 1.0.2-rc` sont faillibles. Status privé
et diagnostics exposent `sync_log_len: null` avec
`sync_log_observation_ok: false` lorsque le provider actif ne peut pas être lu,
sans annoncer un état ancien.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

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

Les hosts Runtime gèlent l'enregistrement des queries de `ApiRouter` après le
bootstrap. Les clones du router partagent les endpoints via `Arc` ; la façade
directe, le HTTP et le peer RPC libèrent le mutex d'état du host avant
l'exécution. Les queries indépendantes s'exécutent en parallèle ; un appel
tardif à `register_query` échoue avec `router_frozen`.
`query_names_iter` emprunte le registre gelé pour la validation interne, tandis
que `query_names` conserve l'ordre owned déterministe aux frontières de sortie.
Le check du manifest parcourt donc les noms sans cloner tout le catalogue.

Dans `1.0.2-rc`,
`ReloadableRuntimeHttpHost` fournit une transaction explicite de génération de
routing. `prepare` accepte seulement une génération plus récente sur la même
adresse liée. `reload` exécute `/v1/health` avant activation, commute
atomiquement le routing des nouvelles requêtes, vérifie encore la santé puis
draine l'ancien in-flight. Si la santé après commutation ou le drain échoue,
l'ancienne génération est restaurée et la génération défaillante ferme son
admission avant nettoyage. Une requête admise ne change jamais de router. Les
délais sont positifs et plafonnés à 60 secondes; les snapshots ne contiennent
aucune identité de requête. Le owner garde au plus une génération active et une
en drain. Une génération défaillante avec des requêtes bloque le reload suivant,
et le dernier permit libère le Router sans tâche de nettoyage.
`generation_snapshot` expose cet état borné sans payload ni historique. Une
annulation après la commutation restaure synchroniquement la génération
précédente avant la réouverture de l'admission.

Les changements d'adresse restent hors de cette primitive à listener stable.
La composition root doit préparer un second listener et le coordonner avec le
Supervisor existant. Il n'existe ni watcher automatique du manifest V1 ni
fallback.
Pour valider le bind avant le démarrage sur l'adresse stable, la composition
root peut transférer un listener TCP déjà lié via
`run_on_listener_until_shutdown`.

La limite configurée s'applique au corps HTTP complet avant la
désérialisation JSON par Axum. Les routes protégées acceptent exactement un
header bearer `Authorization` bien formé; les doublons échouent fermés.

Le host TCP intégré ferme la connexion après 10 secondes sans progression de
lecture, y compris avec des headers HTTP incomplets. Une réponse HTTP ne peut
pas être formée avant l'existence d'une requête : l'inactivité des headers
ferme donc le socket. L'inactivité du corps après une requête complète renvoie
toujours HTTP 408.

La validation de query structurée transmet le JSON à un writer compteur borné.
Elle applique ainsi la limite exacte d'octets sérialisés sans conserver un
`Vec<u8>` encodé, tandis que la méthode publique `payload_bytes()` reste
compatible. Le chemin HTTP ne valide qu'une fois avant le dispatch blocking.

Le router possède un seul `RuntimeStaticInfo` immuable partagé ; cloner l'état
de la requête ne copie ni les listes de peers, ni les seeds DNS, ni les paths ou
les chaînes d'identité. Le dispatch blocking prend possession des requêtes
command/query. L'audit query ne conserve que l'ID et le nom bornés pendant le
traitement du payload.
Utilisez `CommandRequest::into_envelope` lorsque le caller possède la requête :
la validation V1 est conservée et l'allocation UTF-8 existante est déplacée vers
le payload binaire du core. `to_envelope` reste compatible avec l'emprunt.

`CommandTokenVerifier` possède aussi des méthodes additives pour les requêtes
empruntées. Leurs defaults matérialisent `RequestValidationDetails` et appellent
les méthodes owned existantes, donc les verifiers existants gardent leur
comportement. Le verifier du Runtime les surcharge pour hasher directement le
texte ou le JSON structuré, sans copie owned du payload.

Le dispatch command/query partage 16 permits blocking dans le processus. Le
runtime utilise au plus 16 threads blocking avec des stacks de 1 Mio et retire
les threads inactives après cinq secondes. Un gate plein renvoie HTTP 503 avant
l'admission.

La query intégrée `runtime.audit` plafonne `limit` à 1 000. Elle obtient des
snapshots partagés des enregistrements et entrées sous des locks courts, puis
matérialise seulement la page récente demandée après leur libération. Aucune
file complète de 10 000 éléments n'est clonée en profondeur. La sélection
partagée de 1 000 sur 10 000 a mesuré 2,06 us p50 et 11,88 Mio de RSS de pic,
contre 4,16 ms et 20,33 Mio pour les copies owned complètes.

`runtime.events` utilise la même frontière de snapshot, limite toujours la page
récente à 1 000 et omet les payloads opaques de sa réponse inchangée.
Sélectionner 1 000 sur 10 000 événements a mesuré 2,39 us p50 et 8,48 Mio de RSS
de pic, contre 2,09 ms et 14,59 Mio pour cloner tout l'historique.

`HttpCommandAuth::default()` exige l'authentification et échoue fermé tant
qu'aucun vérificateur de token n'est configuré. Seul
`insecure_local_for_testing()` désactive explicitement l'authentification
command/query pour des tests locaux contrôlés. `/v1/health` reste public par
contrat. Les refus d'autorisation command sont audités avec des métadonnées
normalisées, sans credentials, payload ni clé d'idempotence.

**Maturité :** surface HTTP V1 RC stricte et stable.
