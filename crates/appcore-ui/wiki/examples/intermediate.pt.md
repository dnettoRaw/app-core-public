# Página web — fluxo Runtime

Compile o cliente tipado e execute a fixture SDK com catálogo de capabilities e autorização explícita da superfície. Executa uma query, um command idempotente e uma subscrição. Repetir a chave mantém um efeito e um evento Runtime. Close rejeita pedidos pendentes e liberta a superfície.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

[Rust](../../examples/web_bridge.rs) · [SDK](../../examples/web-bridge/runtime.rs) · [Client](../../examples/web-bridge/page.js) · [Guide](../guide.pt.md)

A sessão da fixture serve apenas para qualificação. Um deployment real deve autorizar o seu principal, tenant e capabilities e isolar providers bloqueantes. Não inicia executor escondido.

Estes exemplos estão incluídos no pacote beta público para macOS. Renderers de outras plataformas são trabalho futuro.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

A galeria usa Editing → Review → Applied, revisões, campos bloqueados e comandos tipados por script. Não há IA ligada. O runner tem física determinística, colisão/score/restart/pausa. O exemplo 3D usa WebGL direto do sistema, perspetiva/profundidade e seis trajetórias de Lorenz, integração RK4, 24.576 pontos limitados, órbita/zoom, isolamento de trajetória, pausa e avanço por passos; não é engine 3D nativa Rust.
