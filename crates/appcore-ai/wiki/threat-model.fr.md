# Threat model IA

[English](threat-model.en.md) | [Português](threat-model.pt.md) |
[Guide](guide.fr.md) | [LLM génératifs](generative-llm.fr.md)

Périmètre : `appcore-ai 0.1.0-beta.3`, backends Candle et OpenAI-compatible
optionnels, composant `appcore-bin` opt-in et frontières Swarm expérimentales.
La crate ne prétend ni sandbox processus ni zero trust.

| Menace | Contrôle | Limite résiduelle |
|---|---|---|
| modèle malveillant/remplacé/empoisonné | taille exacte + SHA-256, provenance optionnelle, vérification avant activation | la policy choisit les publishers fiables |
| traversal/symlink | nom par digest, racine canonique, open no-follow, validation metadata/taille du handle, temporaire exclusif et activation atomique sans remplacement | l'administrateur local garde l'autorité hôte |
| decompression bomb/tensor géant | format natif non compressé ; artefact, dimensions, classes, inputs, outputs, RAM/VRAM bornés | les formats externes exigent un parser backend sûr |
| metadata fausse/custom ops | validation registre et `ModelSecurityPolicy`, formats provider refusés par défaut | un format arbitraire peut exécuter du code backend |
| fuite prompt/credential | `Debug` expurgé, observations sans payload, secret references uniquement | un callback application peut toujours mal journaliser |
| isolation tenant | contexte authentifié, grants exacts et validation tenant bridge | l'adaptateur hôte appartient à la base de confiance |
| crash backend natif | feature optionnelle, entrées bornées, erreurs traduites | Candle reste in-process, sans crash sandbox |
| abus/contrôle par le probe matériel | queries OS/sysfs/NVML bornées en lecture seule, sans shell/WMI ni API d'écriture fan/fréquence/voltage/puissance | administrateur local et kernel/pilote restent fiables ; diagnostic révèle la capacité agrégée |
| épuisement/DoS | probe single-flight hors lock, governor, admission, deadline, annulation et bornes fixes files/batches/registres/routes/résidents/peers/transferts | `Unrestricted` réduit volontairement la marge |
| poisoning training | dataset borné explicite, seed reproductible, resume/checkpoint vérifiés | le Runtime ne connaît pas la qualité du dataset |
| peer malveillant/discovery compromis | authenticator AppCore, annonces expirantes strictement plus récentes, rejet des claims dupliqués, grants et plafonds | les métriques annoncées peuvent mentir |
| artefact empoisonné/retenu | digest/taille/provenance bout en bout, timeout, stores alternatifs bornés | la fausse disponibilité consomme le budget retry |
| replay | bridge exige nonce, expiration et replay protection Peer RPC AppCore | aucun second replay store dans cette crate |
| churn/fausse disponibilité | lease, health/coût et failover borné | un travail lancé peut échouer |
| résultat distant non fiable | cible authentifiée, réponse bornée, diagnostic explicite | la correction générique n'est pas prouvable cryptographiquement |

## Menaces LLM génératives supplémentaires

| Menace | Contrôle implémenté ou exigé du deployment | Limite résiduelle |
|---|---|---|
| model server exposé | bind loopback, authentification hôte et firewall deployment | l'administrateur local contrôle le processus |
| prompt injection déclenche un tool | tools et autorisation dans l'application ; output jamais transformé automatiquement en commande | le contenu non fiable influence toujours le modèle |
| chat template/tokenizer remplacé | binding exact, digest/révision et digest par range du bundle | HTTP générique ne prouve pas les octets chargés par le serveur externe |
| DoS contexte/KV cache | tokens, contexte, sequences, file et mémoire bornés avant dispatch | seul l'engine connaît la tokenisation exacte |
| option engine ignorée | négociation de capability et erreur explicite pour sampling/tools non supportés | les API OpenAI-compatible ne sont pas sémantiquement identiques |
| output partiel après annulation | streaming opt-in vérifie l'annulation entre chunks bornés et applique une backpressure synchrone ; réponses complètes bornées | output déjà livré irrévocable ; l'application marque le stream annulé incomplet |
| erreur provider divulgue des données privées | seuls statut exact et `Retry-After` borné en secondes traversent la frontière | bodies provider indisponibles même pour le diagnostic |
| JSON provider remplace la policy centrale | paramètres validés rejettent clés réservées, profondeur/nodes excessifs et contrôles | le owner deployment doit tester la sémantique provider |
| engine natif compromis | processus isolé, path immutable, user non privilégié et Supervisor | le sandbox fort appartient à la deployment |
| range de modèle segmenté corrompu | bundle lié à l'identité complète, ranges bornés sans chevauchement, SHA-256 par segment | NVMe et admin local restent fiables |

Le serveur LLM ne reçoit aucun filesystem tool direct par défaut. La
compatibilité HTTP OpenAI est un transport, pas une boundary de sécurité ni une
preuve d'équivalence du sampling.

Invariants : confidentialité et resource mode locaux gagnent toujours ; aucun
peer ne force `Unrestricted` ; contribution non extensible à distance ; aucun
modèle dans le RPC générique ; aucune donnée brute dans la télémétrie ; artefact
signé fail-closed ; capacité inconnue jamais illimitée.

Le fixture corrompu est
[`tests/fixtures/corrupt-native-linear-v1.artifact`](../tests/fixtures/corrupt-native-linear-v1.artifact).
Des sweeps et trois cibles `cargo-fuzz` exercent le parser natif, les frontières
de contrat et le décodeur OpenAI-compatible borné. Les tests font aussi
concourir 32 writers sur un artefact et rejettent un symlink Unix pour les
lectures full/range et le test d'existence.
