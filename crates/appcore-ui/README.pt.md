# appcore-ui

Módulo de UI independente, beta pública `0.1.0-beta.1`.

O módulo deve renderizar páginas nativas definidas em Rust e páginas web em
HTML/TypeScript. Ciclo de vida das janelas, renderização, ações, estado visual e
bridge com o Runtime pertencem ao appcore-ui. Layouts e schemas de negócio
pertencem à aplicação. Deployment seleciona explicitamente o backend de plataforma,
fornece autenticação e possui arranque/shutdown; o SDK não inicia um host de UI.

A implementação atual fornece contratos UI V1 limitados para commands, queries
e eventos, capabilities, ownership autenticado da superfície, cancelamento,
deadlines, backpressure e cliente TypeScript tipado. Commands preservam idempotência
e emissão de eventos pelo Runtime. Estes testes da bridge não provam renderização.

Os previews nativo e web local abaixo renderizam em macOS. A bridge Runtime pode ser ligada explicitamente.
A tela externa foi verificada num perfil sintético e isolado. Outras plataformas são trabalho futuro. Preferir recursos do sistema operativo, reutilizar
contratos existentes e justificar cada dependência externa. O módulo não exige
um shell de aplicação externo nem um framework de renderização empacotado.

UI V1 é independente das versões de manifests e protocolo peer. Entradas não
suportadas falham com `NO MORE SUPPORTED PLEASE UPDATE`. Notificações de UI são
limitadas e efémeras; reconstruir estado por query após reconexão ou perda
observável. Cancelamento não reverte um efeito já produzido.

## Preview nativo em fonte

`UiNativePageV1` declara controlos nativos em Rust. `UiNativePage::new`
(ou `decode`) valida versão 1, IDs únicos, geometria finita a partir do canto
superior esquerdo, 512 controlos, 256 KiB codificados, 4096 bytes por label e
até 4.194.304 píxeis lógicos de viewport. Este perfil inicial usa frames fixos;
ainda não é um sistema completo de layout responsivo. Debug não expõe conteúdo.

Em macOS, a feature explícita `native-macos` fornece `UiNativeApplication` e
`UiNativeWindow`. Deployment cria um controller por processo com orçamento de
janelas físicas (1–8), abre páginas validadas, processa lotes limitados (primeira espera até um segundo; depois até 64 eventos
com orçamento cooperativo de 2 ms) e consome ativações nativas. A fila por janela
admite 1–256 ativações. Overflow e panics capturados no hook de close têm
contadores. As ativações identificam bindings da aplicação; não dispensam a
autorização do Runtime nem executam commands por si. Close chama o hook uma
vez; Drop liberta janelas e controlos retidos. Uma janela fechada mantém a sua
vaga física até o handle ser descartado.

O backend usa três bindings diretos opcionais do sistema (`objc2`,
`objc2-app-kit`, `objc2-foundation`), cinco pacotes ativos incluindo transitivos. São
chamadas tipadas às APIs do SO, sem outro framework de renderização. Unsafe fica
isolado nas fronteiras documentadas de FFI/callback. A bridge e o modelo de página
por defeito não exigem estes pacotes. Outras plataformas nativas continuam trabalho futuro.

Executar o preview em fonte no macOS:

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

O preview tem duração limitada, mostra texto/botão nativos, emite apenas IDs de
controlos/ações e fecha normalmente. A janela macOS foi inspecionada visualmente;
o botão entregou `state.inspect` e o encerramento terminou com código 0. Esta
prova de renderização é distinta da qualificação do consumidor abaixo.

## Recursos web locais

`UiWebBundle` recebe HTML, JavaScript compilado, CSS, PNG/JPEG e WOFF2 imutáveis
produzidos pelo build da aplicação. Não acede a ficheiros ou à rede. Paths são
identificadores relativos ASCII explícitos; traversal, encoding, URLs e query
strings são rejeitados. Lookup não possui fallback. Há limites de 512 assets,
2 MiB por asset e 16 MiB retidos no total, MIME explícito e validação UTF-8 dos
textos. Estes limites abrangem bytes de fonte retidos, não imagens/fontes
descodificadas nem memória do engine do sistema.

A política inicial nega ligações remotas, scripts/styles inline, frames, forms e
plugins. O preview abaixo aplica CSP e nega navegação externa;
o modelo de dados sozinho não renderiza páginas web. A qualificação abaixo verifica uma tela externa num consumidor isolado. Esta
fronteira de recursos não acrescentou dependências.

## Preview web direto em fonte

Em macOS, a feature explícita `web-macos` fornece `UiWebWindow`, usando o
componente web do sistema, um binding opcional de plataforma (`objc2-web-kit`)
e o binding de callback blocks (`block2`). Não empacota browser ou outro shell
de aplicação. O crate por defeito continua sem estas dependências de plataforma.

O host serve apenas recursos do bundle numa origem local aleatória por janela.
Emite CSP e nosniff, rejeita navegação externa/popups/frames, credenciais, ports,
query e encoding em URLs, e usa perfil de dados não persistente. Admite 16 tasks
de recursos, observando cancelamento entre callbacks de resposta. Perda do engine
ou carregamento inicial acima de 30 segundos fecha a janela quando o deployment
continua a processar eventos. Não há retry automático ou host oculto. Estes
limites não abrangem toda a memória do engine do sistema.

```sh
cargo run -p appcore-ui --features web-macos --example web_page
```

A janela real carregou HTML/CSS/JavaScript, o botão atualizou o documento, uma
navegação externa foi rejeitada e close deixou zero janelas retidas. O preview
exercita ações DOM locais. A bridge descrita abaixo pode ser ligada
antes do carregamento; a tela existente foi qualificada num perfil isolado.

## Bridge Runtime no host web

Antes do primeiro pump, chamar `window.attach_surface(factory)`. A factory recebe
ID/origem reais da janela e devolve uma superfície com essa vinculação exata,
catálogo declarado e autorização obrigatória do deployment. O host verifica URL
do documento/main frame e schema; JSON da página não escolhe caller ou principal.
Documentos com bridge não navegam/recarregam implicitamente noutra geração.
Reconexão cria nova janela e superfície.

Replies Promise ficam numa fila limitada: orçamento de operações da superfície,
duas vagas reservadas para close/cancel e 16 MiB de pedidos. O pump explícito
processa uma passagem por tick. Autorização e backend devem ser não bloqueantes,
ou o deployment deve isolar trabalho bloqueante com limites. Não inicia executor
ou threads. Close rejeita Promises pendentes, cancela a superfície e liberta
subscrições; não reverte efeitos já realizados.

O cliente TypeScript exporta `connectUiWeb`, usando metadata pública do host e
transporte Promise nativo. Mantém operações tipadas, erros controlados, deadlines,
backpressure e chaves de idempotência. IDs usam a API de bytes aleatórios
criptográficos do sistema, sem biblioteca adicional.

```sh
npm ci
npm run build:ui
cargo run -p appcore-ui --features web-macos --example web_bridge
```

Esta fixture SDK explicitamente identificada usa o cliente TypeScript real no
componente web do sistema. Query, command, subscrição e replay com a mesma chave
foram verificados: um efeito, um evento Runtime e uma notificação UI. Autoridade
forjada e ID de outra superfície foram rejeitados. Close deixou zero pedidos
pendentes e janelas retidas. A autorização da fixture não é login de produção.
Um consumidor externo independente reutilizou a tela de estado da sincronização
e o CSS existentes. A janela real macOS apresentou estado sintético, realizou
duas queries pela bridge e terminou com código 0, zero pedidos pendentes e zero
janelas retidas. Isto valida um consumidor de fonte, sem dados/autenticação de
produção ou migração completa da aplicação. Gates gerais no plano de implementação.

## Exemplos interativos e funções nativas

O perfil nativo inclui agora texto editável, checkbox, dropdown, slider e progresso determinado, além de texto/botões. `elements::{text,button,input,toggle,select,slider,progress}` são construtores Rust puros; valide a página completa. `UiNativeWindow::{set_label,set_value,set_enabled,value}` atualizam/leem controlos existentes com validação tipada e limites de página/label, sem alterar ações ou conceder autorização. Dropdowns admitem 1–64 labels únicos não vazios; o input é limitado ao ler, sem garantir toda a memória dos controlos de edição do SO.

A galeria usa Editing → Review → Applied, revisões, campos bloqueados e comandos tipados por script. Não há IA ligada. O runner tem física determinística, colisão/score/restart/pausa. O exemplo 3D usa WebGL direto do sistema, perspetiva/profundidade e seis trajetórias de Lorenz, integração RK4, 24.576 pontos limitados, órbita/zoom, isolamento de trajetória, pausa e avanço por passos; não é engine 3D nativa Rust.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

Os quatro exemplos foram renderizados e usados em janelas macOS reais; fechar cada um terminou 0 com zero janelas retidas. A proposta de componentes/IA é roadmap, não protocolo de tools implementado.

## Resposta à interação

O pump serve o run loop mesmo sob carga de input e a bridge antes/depois do dispatch nativo. A drenagem para quando a fila de ativações enche; consuma `pending_activations` com `next_activation` depois do pump. Tracking/callbacks do sistema podem exceder o orçamento cooperativo. A WebView aceita o primeiro clique físico ao ativar uma janela inativa; muda o foco, não a autorização. Os testes físicos receberam dez de dez ações nos exemplos nativo e web, e um clique ao alternar janelas.

```toml
[dependencies]
appcore-ui = { version = "=0.1.0-beta.1", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.1) · [API](https://docs.rs/appcore-ui/0.1.0-beta.1/appcore_ui/)
