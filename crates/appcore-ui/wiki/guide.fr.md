# appcore-ui

Module UI indépendant, bêta publique `0.1.0-beta.2`.

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
appcore-ui = { version = "=0.1.0-beta.2", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.2) · [API](https://docs.rs/appcore-ui/0.1.0-beta.2/appcore_ui/)

## Candidat de publication — 2026-10-14

`0.1.0-beta.2`: alignement des dépendances.
