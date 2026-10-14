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
