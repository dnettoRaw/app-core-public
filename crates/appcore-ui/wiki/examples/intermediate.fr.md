# Page web — workflow Runtime

Compilez le client typé, puis lancez la fixture SDK avec catalogue de capabilities et autorisation explicite de la surface. Elle exécute une query, un command idempotent et une souscription. Répéter la clé conserve un effet et un événement Runtime. Close rejette les requêtes pendantes et libère la surface.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

[Rust](../../examples/web_bridge.rs) · [SDK](../../examples/web-bridge/runtime.rs) · [Client](../../examples/web-bridge/page.js) · [Guide](../guide.fr.md)

La session de la fixture sert uniquement à la qualification. Un deployment réel doit autoriser son principal, tenant et capabilities et isoler les providers bloquants. Aucun executor caché ne démarre.

Ces exemples sont inclus dans le paquet bêta public pour macOS. Les renderers des autres plateformes restent des travaux futurs.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

La galerie utilise Editing → Review → Applied, révisions, champs bloqués et commands typés par script. Aucun modèle IA connecté. Le runner a physique déterministe, collision/score/restart/pause. L’exemple 3D utilise WebGL système direct, perspective/profondeur et six trajectoires de Lorenz, intégration RK4, 24 576 points bornés, orbite/zoom, isolation de trajectoire, pause et avance par pas ; ce n’est pas un moteur 3D natif Rust.

La galerie 3D en source ajoute une galaxie déterministe de 12 000 étoiles, un terrain procédural et une surface ondulée dans le vertex shader (8 192 triangles chacun). Sélection, pause/pas/reset et orbite/zoom utilisent le renderer WebGL borné. Ces exemples ne prouvent ni compute GPU natif Rust, ni fluides physiques, ni performance 4K.

La galerie source démarre dans la Ville fractale : 391 boîtes (4 692 triangles), trois niveaux de tours récursives, passerelles, éclairage directionnel par face et brouillard. Cliquer le canvas donne le focus ; WASD vole, Q/E change l’altitude, Shift accélère, glisser oriente, R revient au départ et Esc libère le focus. Coordonnées bornées, sans collision solide. Les touches sont effacées à la perte de focus/visibilité/contexte. Cet exemple récursif fini utilise WebGL ; la 3D native Rust reste prévue.

La feature optionnelle native-input-timing de appcore-ui ajoute des temps locaux sans modifier UiNativeActivation ou UI V1. next_timed_activation fournit le temps depuis callback et, uniquement pendant un dispatch souris/clavier corrélé par le pump, depuis événement. Les callbacks d’accessibilité non corrélés restent None. Le contexte est restauré après dispatch imbriqué/unwind ; une horloge invalide ne produit pas un zéro fictif. Le temps depuis mouse-down inclut maintien/tracking jusqu’au relâchement. L’uptime brut n’est jamais sérialisé ni journalisé.

NativeRenderer.frame_progress expose les IDs soumis/terminés propres à la génération via callbacks de queue, sans recul ni wrap. La fin GPU observée par polling n’est ni présentation du compositor ni scanout physique. La sonde native_meshes conserve 256 échantillons/marqueurs au maximum, expose overflow/attente et associe chaque action au premier frame soumis ensuite ; les états intermédiaires peuvent être agrégés. Retour de present et fin observée sont des endpoints distincts. Ce profil optionnel utilise systemUptime pour mesurer des durées locales ; inclure examples/native-gpu/PrivacyInfo.xcprivacy (raison 35F9.1) dans le bundle de l’application selon [les raisons d’API Apple](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype).

Qualification : Metal, Apple M1, 1024×768 physiques, 320 instances/2 maillages, attente pump 8 ms. Dix clics par coordonnées via événements natifs et dix activations d’accessibilité ont livré 20 actions. p50/p95/p99 depuis événement natif : action 1.549/19.964/19.964 ms ; retour de present 13.326/30.874/30.874 ms ; fin GPU observée 23.511/42.509/42.509 ms (n=10). Callback→action agrégé 2.289/103.954/104.105 ms (n=20) ; accessibilité sans timestamps inventés (uncorrelated=10). Overflow/attente zéro ; submitted=completed=3435, retained_windows=0. Cet échantillon réduit ne qualifie ni 4K60 ni une latence universelle.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_meshes
```

GpuMeshScene::new_lod compose désormais des batches LOD avec buffers de sélection prepass/final distincts, pyramide Hi-Z actuelle partagée et bounds conservateurs de tous les niveaux. encode_lod reçoit LodSceneFrame avec matrice/view/groupes explicites cohérents ; encode statique sur scène LOD est rejeté avant writes. Géométrie encore dupliquée par passe et comptabilisée. set_lod_resident modifie la disponibilité des deux passes sans éviction. Construction/encode statiques conservent leur contrat. NativeRenderer expose enable_mesh_lods, present_mesh_lods et modifications après fin des frames, avec ownership de surface, admission, pic de resize et suivi de fin GPU existants.

native_lod ajoute 256 sphères raffinées/proxies octaédriques et 64 boîtes/proxies tétraédriques, orbite/zoom/pause, budgets 4/12/32/96 pixels et Fine on/off logique. Reducer pur : commandes inconnues rejetées ; alignement caméra/projection et contrôles bornés testés. Metal réel 512×512, deux groupes LOD : perte/restauration du détail, reset vide, admission et mode explicite. Processus natif exécuté jusqu’à deadline de 180 secondes : frames=3239, submitted=completed=3239, retained_windows=0 ; actions=0. Qualification visuelle/input/resize de ce nouvel exemple encore en attente : automatisation native cgWindowNotFound pour cette app et l’app de contrôle déjà qualifiée. Ne qualifie ni 4K60, streaming, compilation meshlet ni éviction mémoire.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_lod
cargo test -p appcore-render --features metal --test lod_scene -- --include-ignored
```


Les exemples native_lorenz, native_meshes et native_lod affichent un HUD lisible : présentations réussies/seconde, intervalle moyen/p95 entre présentations (128 échantillons), durée CPU de present, RAM résidente du processus (RSS), payload GPU géré, résolution physique, adaptateur et compteurs soumis/terminés. Actualisation deux fois par seconde. CPU submit ne mesure pas le temps GPU ; GPU payload est une comptabilité de budget excluant pilote/compositeur/buffers supplémentaires de swapchain, pas une mesure VRAM. Ces métriques ne prouvent ni scanout ni 4K60.

Cliquer sous le HUD active la caméra : glisser pour orbiter, W/S ou molette pour zoomer, A/D ou flèches gauche/droite pour orbiter, flèches haut/bas pour incliner. Échap, Tab, clic sur les contrôles et perte de focus libèrent la navigation et effacent les touches maintenues. Les raccourcis système restent disponibles. Navigation orbitale bornée, sans vol libre ni manipulation des objets. La navigation manuelle arrête l’orbite automatique des maillages ; Resume la relance. La simulation Lorenz reste indépendante. Les fenêtres restent ouvertes jusqu’à fermeture ; E3DN_DEMO_SECONDS=30 (1–600) permet une exécution temporisée.

Avec gpu-macos, enable_scene_input(overlay_height) active la navigation à état fixe ; take_scene_input(elapsed) consomme les deltas, plafonne elapsed à 50 ms et expose le focus sans texte brut des touches. native_process_resident_bytes lit le RSS courant via l’API système Mach ; les erreurs restent explicites. Les labels sur les graphiques ont un fond opaque pour rester lisibles.
