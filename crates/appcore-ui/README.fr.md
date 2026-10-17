# appcore-ui

Release candidate **`appcore-ui 1.0.0-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.

Module UI indépendant, release candidate `1.0.0-rc.1`.

Le module doit rendre des pages natives définies en Rust et des pages web en
HTML/TypeScript. Lifecycle des fenêtres, rendu, actions, état visuel et bridge
Runtime appartiennent à appcore-ui. Layouts et schemas métier restent dans
l’application. Deployment sélectionne explicitement le backend de plateforme,
fournit l’authentification et possède startup/shutdown ; le SDK ne lance aucun host UI.

L’implémentation actuelle fournit contrats UI V1 bornés pour commands, queries
et événements, capabilities, ownership authentifié, annulation, deadlines,
backpressure et client TypeScript typé. Commands conservent idempotence et émission
d’événements par le Runtime. Ces tests de bridge ne prouvent pas le rendu.

Les previews natif et web local ci-dessous rendent sur macOS. Le bridge Runtime peut être attaché explicitement.
L’écran externe a été vérifié dans un profil synthétique isolé. Les autres plateformes restent des travaux futurs. Préférer les services du système, réutiliser les
contrats existants et justifier chaque dépendance externe. Aucun shell externe
ou framework de rendu embarqué n’est exigé par ce module.

UI V1 est indépendant des versions de manifests et du protocole peer. Les entrées
non supportées échouent avec `NO MORE SUPPORTED PLEASE UPDATE`. Notifications UI
bornées et éphémères : reconstruire l’état par query après reconnexion ou perte
observable. Annulation ne revient pas sur un effet déjà produit.

## Preview natif en source

`UiNativePageV1` déclare des contrôles natifs en Rust. `UiNativePage::new`
(ou `decode`) valide version 1, identifiants uniques, géométrie finie depuis le
coin supérieur gauche, 512 contrôles, 256 KiB encodés, 4096 octets par label et
au plus 4 194 304 pixels logiques de viewport. Ce profil initial utilise des
frames fixes ; ce n’est pas encore un layout responsive complet. Debug ne révèle
pas le contenu de la page.

Sur macOS, la feature explicite `native-macos` fournit `UiNativeApplication` et
`UiNativeWindow`. Deployment crée un controller par processus avec budget de
fenêtres physiques (1–8), ouvre les pages validées, pompe des lots bornés (première attente jusqu’à une seconde ; puis 64 événements
au maximum avec budget coopératif de 2 ms) et consomme les activations natives.
La file par fenêtre admet 1–256 activations. Overflow et panics interceptés dans
le hook close ont des compteurs. Les activations identifient des bindings ; elles
ne remplacent pas l’autorisation Runtime et n’exécutent pas de commands seules.
Close appelle le hook une fois ; Drop libère les fenêtres et contrôles retenus.
Un handle fermé garde son permis physique jusqu’à sa destruction.

Le backend utilise trois bindings système directs optionnels (`objc2`,
`objc2-app-kit`, `objc2-foundation`), cinq packages actifs avec les transitifs. Ils
fournissent des appels OS typés, sans autre framework de rendu. Unsafe reste
isolé aux frontières FFI/callback documentées. Bridge et modèle de page par défaut
n’exigent pas ces packages. Les autres plateformes natives restent des travaux futurs.

Exécuter le preview en source sur macOS :

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

Le preview a une durée bornée, affiche textes/bouton natifs, émet seulement les
IDs de contrôles/actions et ferme normalement. La fenêtre réelle a été inspectée ;
le bouton a livré `state.inspect` et la fermeture est sortie avec code 0. Cette
preuve de rendu est distincte de la qualification consommateur ci-dessous.

## Ressources web locales

`UiWebBundle` reçoit HTML, JavaScript compilé, CSS, PNG/JPEG et WOFF2 immuables
produits par le build de l’application. Aucun accès fichiers ou réseau implicite.
Paths relatifs ASCII explicites : traversal, encoding, URLs et query strings
sont rejetés. Lookup sans fallback. Limites : 512 assets, 2 MiB par asset et
16 MiB retenus au total, MIME explicite et validation UTF-8 des textes. Ces budgets
bornent les octets source, pas images/fonts décodées ou mémoire du moteur système.

La politique initiale refuse connexions distantes, scripts/styles inline, frames,
forms et plugins. Le preview ci-dessous applique CSP et refuse
la navigation externe ; le modèle seul n’est pas un renderer web. La qualification ci-dessous vérifie l’écran externe dans un consommateur isolé. Aucune nouvelle
dépendance n’a été ajoutée pour cette frontière de ressources.

## Preview web direct en source

Sur macOS, la feature explicite `web-macos` fournit `UiWebWindow` via le composant
web système, un binding optionnel (`objc2-web-kit`) et le binding de callback
blocks (`block2`). Aucun browser runtime ou shell d’application n’est embarqué.
Le crate par défaut reste sans ces dépendances de plateforme.

Le host sert seulement le bundle sur une origine locale aléatoire par fenêtre.
Il émet CSP/nosniff, refuse navigation externe/popups/frames, credentials, ports,
queries et encoding d’URL, et utilise un profil non persistant. Au plus 16 tasks
de ressources sont admises ; annulation vérifiée entre callbacks de réponse.
Perte du moteur ou chargement initial au-delà de 30 secondes ferme la fenêtre
lorsque deployment continue le pump. Aucun retry ou host caché. Ces limites
ne bornent pas toute la mémoire du moteur système.

```sh
cargo run -p appcore-ui --features web-macos --example web_page
```

La fenêtre réelle a chargé HTML/CSS/JavaScript, le bouton a modifié le document,
la navigation externe a été refusée et close a laissé zéro fenêtre retenue.
Le preview exerce des actions DOM locales. Le bridge ci-dessous peut être attaché
avant chargement ; l’écran existant a été qualifié dans un profil isolé.

## Bridge Runtime dans le host web

Avant le premier pump, appeler `window.attach_surface(factory)`. La factory reçoit
ID/origine natifs réels et retourne une surface avec ce binding exact, catalogue
déclaré et autorisation obligatoire du deployment. Le host vérifie URL du document,
main frame et schema ; JSON ne choisit ni caller ni principal. Une page avec bridge
ne navigue/recharge pas implicitement vers une nouvelle génération. Reconnexion
crée une nouvelle fenêtre et surface.

Les replies Promise restent bornés : budget de surface, deux places réservées
close/cancel et 16 MiB de requests. Le pump explicite effectue une passe par tick.
Autorisation/backend doivent être non bloquants, ou deployment doit fournir une
frontière bloquante bornée. Aucun executor/thread n’est lancé implicitement.
Close rejette les Promises pendantes, annule la surface et libère les abonnements,
sans revenir sur les effets déjà réalisés.

Le client TypeScript exporte `connectUiWeb`, avec metadata publique du host et
transport Promise natif. Operations typées, erreurs contrôlées, deadlines,
backpressure et clés d’idempotence sont préservées. IDs issus de l’API système
de bytes aléatoires cryptographiques, sans nouvelle bibliothèque.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

Cette fixture SDK explicitement nommée utilise le vrai client TypeScript dans le
composant web système. Query, command, abonnement et replay de même clé vérifiés :
un effet, un événement Runtime et une notification UI. Autorité forgée et autre
ID de surface refusés. Close a laissé zéro request/fenêtre retenue. L’autorisation
de fixture n’est pas un login de production. Un consommateur externe indépendant réutilise l’écran existant de synchronisation
et son CSS. La vraie fenêtre macOS a affiché des données synthétiques, exécuté
deux queries par le bridge et terminé avec code 0, zéro requête pendante et zéro
fenêtre retenue. Ceci qualifie un consommateur source, sans données/authentification
de production ni migration complète de l’application. Gates généraux dans le plan.

## Exemples interactifs et fonctions natives

Le profil natif inclut texte éditable, checkbox, dropdown, slider et progression déterminée, en plus du texte/boutons. `elements::{text,button,input,toggle,select,slider,progress}` sont des constructeurs Rust purs ; validez la page entière. `UiNativeWindow::{set_label,set_value,set_enabled,value}` modifient/lisent les contrôles avec validation typée et budgets page/label, sans modifier actions ni autorisation. Dropdowns : 1–64 labels uniques non vides ; l’input est borné à la lecture, sans garantir toute la mémoire des contrôles d’édition système.

La galerie utilise Editing → Review → Applied, révisions, champs bloqués et commands typés par script. Aucun modèle IA connecté. Le runner a physique déterministe, collision/score/restart/pause. L’exemple 3D utilise WebGL système direct, perspective/profondeur et six trajectoires de Lorenz, intégration RK4, 24 576 points bornés, orbite/zoom, isolation de trajectoire, pause et avance par pas ; ce n’est pas un moteur 3D natif Rust.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

Les quatre exemples ont été rendus et utilisés dans de vraies fenêtres macOS ; fermer chacun termine 0 avec zéro fenêtre retenue. La proposition composants/IA est un roadmap, pas un protocole tools implémenté.

## Réactivité des interactions

Le pump sert le run loop sous charge d’input et le bridge avant/après dispatch natif. Le drainage s’arrête si la file d’activations est pleine ; consommez `pending_activations` par `next_activation` après pump. Tracking/callbacks système peuvent dépasser le budget coopératif. La WebView accepte le premier clic physique activant une fenêtre inactive ; changement de focus, pas d’autorisation. Les tests physiques ont reçu dix actions sur dix dans les exemples natif/web, plus un clic en changeant de fenêtre.

```toml
[dependencies]
appcore-ui = { version = "=1.0.0-rc.1", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.2) · [API](https://docs.rs/appcore-ui/0.1.0-beta.2/appcore_ui/)

## Candidat de publication — 2026-10-14

`0.1.0-beta.2`: alignement des dépendances.

La galerie 3D en source ajoute une galaxie déterministe de 12 000 étoiles, un terrain procédural et une surface ondulée dans le vertex shader (8 192 triangles chacun). Sélection, pause/pas/reset et orbite/zoom utilisent le renderer WebGL borné. Ces exemples ne prouvent ni compute GPU natif Rust, ni fluides physiques, ni performance 4K.

La galerie source démarre dans la Ville fractale : 391 boîtes (4 692 triangles), trois niveaux de tours récursives, passerelles, éclairage directionnel par face et brouillard. Cliquer le canvas donne le focus ; WASD vole, Q/E change l’altitude, Shift accélère, glisser oriente, R revient au départ et Esc libère le focus. Coordonnées bornées, sans collision solide. Les touches sont effacées à la perte de focus/visibilité/contexte. Cet exemple récursif fini utilise WebGL ; la 3D native Rust reste prévue.

Correction shaders : uniforms et varyings partagés ont une précision explicite identique dans les deux étages WebGL ; les erreurs de compilation/liaison affichent un diagnostic borné du driver. Un test vérifie cette interface.

Qualification macOS après correction : ville rendue, avance W, altitude E, retour R et passage aux vagues animées exercés dans la fenêtre web réelle du système. Cela ne qualifie pas toutes les scènes ni la récupération après perte du dispositif.

Prototype natif source : `gpu-macos` attache une surface Metal empruntée à une fenêtre. Une tentative par fenêtre, renderer thread principal, deux soumissions maximum, callbacks, suspension à taille zéro et backpressure resize. `native_gpu` possède le pump et présente un clear animé ; scène/compute restent à faire. Perte de surface : reconfiguration explicite ; perte du device : nouvelle génération par le deployment. Acquisition/configuration peut attendre ; aucune latence stricte garantie.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_gpu
```

Qualification présentation : permits RAII testés pour échec/drop et concurrence de 16 threads. Fenêtre macOS réelle : 10 clics physiques plus 10 activations natives (compteur 20), zoom/restauration et fermeture avec frames=7622, clicks=20, retained_windows=0. Fenêtres minimisées exposent une taille graphique zéro ; qualification minimisation/restauration, perte injectée et recréation reste à faire.

Qualification récupération réussie sur Metal réel : six cycles minimisation/restauration, deux pertes de surface injectées, destruction explicite du device avec callback et recréation dans une nouvelle fenêtre. Générations : 202 et 213 frames, pending_before_drop=0 et retained_windows=0. Destruction délibérée, sans preuve de reset physique GPU ou perte spontanée du compositor. Hooks opt-in `qualification` / `gpu-qualification`, defaults préservés.

```sh
cargo run -p appcore-ui --features gpu-qualification --example native_gpu_recovery
```

Lorenz natif connecté : RK4 GPU, quads instanciés, caméra perspective, profondeur, six couleurs et 24 576 échantillons bornés. Contrôles natifs pause/reprise, pas, reset, orbite/inclinaison, zoom, isolation. Vérification macOS réelle : attracteur complet, pause, isolation, caméra, reset, un pas passant de 240 à 288 échantillons (8×6). Reprise/fermeture : frames=6747, actions=9, retained_windows=0. Aucun readback de particules par frame ; admission compute/dessin commune. Warmup de huit pas par frame présenté. ECS/monde/virtualisation/physique massive restent à faire.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_lorenz
```

`GpuMeshBatch` fournit géométrie statique opaque partagée, poses/couleurs, frustum GPU conservateur et draw_indexed_indirect. `GpuMeshScene` compose jusqu’à 64 batches fixes et 65536 instances réservées : prepass frustum → Hi-Z actuel par maximum → requêtes conservatrices → indices compactés → dessin indirect final. Buffers/profondeurs prepass et final sont distincts ; la pyramide est reconstruite une fois par frame, avec padding lointain aux bords impairs. Les cas coupés par caméra/near-plane ou viewport restent visibles. Les groupes suivent l’ordre/identité des descripteurs ; les groupes vides remettent les compteurs à zéro. La réservation comprend deux profondeurs, buffers et mips ; la scène standalone exclut la couleur du client. La présentation native admet cette couleur aussi. L’implémentation initiale duplique la géométrie entre prepass/final, comptée dans le budget.

Les tests de pixels Metal réels vérifient profondeur/couleur, exclusion/réapparition par masque, composition multi-mesh, groupes invalides, budget et clear du frame vide. La qualification native a présenté 256 cubes + 64 pyramides, reçu dix clics physiques consécutifs sur l’orbite et exercé pause, zoom, orbite, reset, reprise et agrandissement/restauration de fenêtre. Shutdown : frames=3102, actions=15, retained_windows=0. Qualification précédente : six cycles minimiser/restaurer, deux pertes de surface injectées, destruction volontaire du device/callback et recréation explicite (202/213 frames, zéro pending/owners). Ce n’est pas un reset physique GPU. Hooks opt-in qualification/gpu-qualification.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_meshes
```

LorenzParameters expose sigma (1–20), rho (1–50), beta (0.5–6) et rejette les valeurs non finies. reset_with_parameters/configure_lorenz redémarrent trajectoires et historique après fin ; reset conserve les coefficients. Timestep fixe 0.005 ; ces bornes ne garantissent pas toute stabilité numérique. Tests Metal : coefficients personnalisés comparés à une référence f64, rejet sans modification. Le bouton natif Params parcourt trois presets bornés. Qualification en fenêtre réelle : 0→1→2→0, historique remis à zéro en pause, pas GPU de 48 échantillons, reprise et fermeture.

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

La fermeture native expose abort_close en cas d’erreur ou de délai de drainage expiré. Elle ferme et détruit cette génération du périphérique sans attente GPU ni mise à jour artificielle des frames terminées ; les appels répétés sont sans effet supplémentaire. Drop détruit aussi le périphérique avant la libération du propriétaire emprunté de la surface, y compris lors des retours anticipés sur erreur. Utiliser begin_close/finish_close pour le drainage normal. La destruction appelle le backend sans garantie absolue de durée ; elle ne prouve pas l’exécution ou l’affichage des frames en attente. Après abandon, finish_close renvoie RendererClosed même après un nouvel appel à begin_close ; le travail abandonné ne peut pas être présenté comme un drainage normal réussi.

L’admission du redimensionnement Lorenz compte désormais l’ancienne texture Depth32Float encore retenue, les nouvelles cibles couleur/depth et les buffers fixes de simulation/caméra avant de reconfigurer la surface. Des dimensions identiques réutilisent le depth. Un refus du pic conserve la scène et la configuration précédentes ; des dimensions nulles suspendent toujours le rendu. Les tests purs couvrent agrandissement/réduction, limites exactes et dimensions identiques. La qualification native refuse aussi un agrandissement dont l’état final tient mais dont le pic dépasse le budget, puis présente la scène Lorenz conservée.

La feature optionnelle gpu-qualification ajoute UiNativeWindow::qualification_content_size(width, height), en points logiques. Elle modifie la taille réelle du contenu AppKit dans les limites originales de la page et du plafond de surface de 4194304 points, rejetant les tailles invalides avant mutation et les fenêtres fermées comme indisponibles. Elle ne redimensionne pas le renderer : le propriétaire pompe, lit graphics_size en pixels physiques et reconfigure explicitement après la fin GPU. native_gpu_recovery exerce désormais neuf redimensionnements réels de fenêtre Lorenz, trois cycles Lorenz de minimisation/restauration, conservation de l’historique en pause, reset puis un pas, avec 21 frames soumises/terminées et aucune fenêtre retenue. Les tailles invalides conservent la taille physique. Cette qualification automatique du cycle de vie ne prouve ni qualité visuelle ni latence d’entrée physique.


Les exemples native_lorenz, native_meshes et native_lod affichent un HUD lisible : présentations réussies/seconde, intervalle moyen/p95 entre présentations (128 échantillons), durée CPU de present, RAM résidente du processus (RSS), payload GPU géré, résolution physique, adaptateur et compteurs soumis/terminés. Actualisation deux fois par seconde. CPU submit ne mesure pas le temps GPU ; GPU payload est une comptabilité de budget excluant pilote/compositeur/buffers supplémentaires de swapchain, pas une mesure VRAM. Ces métriques ne prouvent ni scanout ni 4K60.

Cliquer sous le HUD active la caméra : glisser pour orbiter, W/S ou molette pour zoomer, A/D ou flèches gauche/droite pour orbiter, flèches haut/bas pour incliner. Échap, Tab, clic sur les contrôles et perte de focus libèrent la navigation et effacent les touches maintenues. Les raccourcis système restent disponibles. Navigation orbitale bornée, sans vol libre ni manipulation des objets. La navigation manuelle arrête l’orbite automatique des maillages ; Resume la relance. La simulation Lorenz reste indépendante. Les fenêtres restent ouvertes jusqu’à fermeture ; E3DN_DEMO_SECONDS=30 (1–600) permet une exécution temporisée.

Avec gpu-macos, enable_scene_input(overlay_height) active la navigation à état fixe ; take_scene_input(elapsed) consomme les deltas, plafonne elapsed à 50 ms et expose le focus sans texte brut des touches. native_process_resident_bytes lit le RSS courant via l’API système Mach ; les erreurs restent explicites. Les labels sur les graphiques ont un fond opaque pour rester lisibles.
