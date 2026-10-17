# Texto Unicode integral no PDF (correção 1.0.0-rc.2)

A release candidate `1.0.0-rc.2` inclui as alterações de conteúdo variável, altura intrínseca, paginação adaptativa, diagnóstico, ordem de pintura e Unicode PDF abaixo. Notas anteriores de código não publicado descrevem etapas de desenvolvimento. Selecionar o pacote exato após verificação; a migração do consumidor continua separada.

A exportação Editable e Hybrid associa agora cada cluster de um glifo à sua
sequência UTF-8 lógica completa em `ToUnicode`, em vez de só ao primeiro carácter.
O writer PDF existente emite os destinos UTF-16, incluindo pares substitutos.
Os limites dos clusters seguem os offsets lógicos ordenados em bytes, independentemente
da ordem visual RTL. Fronteiras UTF-8 inválidas, offsets finais ou fora do texto
devolvem `ExportUnsupported`, sem omitir silenciosamente o mapeamento.

Um glifo pode corresponder a `fi` e ao carácter literal `ﬁ`, ou a texto composto
e decomposto. Um mapa por fonte não representa todas essas ocorrências.
Runs ambíguos ou com vários glifos por cluster conservam o texto lógico original
em spans PDF `/ActualText`. Não se associa o texto completo de um cluster a cada
um dos seus glifos. Não é aplicada normalização Unicode.
Os leitores que extraem esses casos contextuais devem respeitar `/ActualText`.

Mantêm-se PDF 1.7, fontes, IDs, avanços e geometria. Sem novas dependências,
campos públicos, opções YAML ou APIs. As fontes Standard 14 WinAnsi conservam
o percurso existente. Flattened continua a produzir contornos sem texto pesquisável.
A correção atua no mapeamento da exportação, mantendo layout, shaping, políticas
de corte e posicionamento bidi. Extratores RTL podem devolver ordem visual com
controlos de direção; o exemplo verifica a sequência integral não acentuada nas
formas lógica ou visual.

As strings dos CMaps partilham o orçamento explícito
`ResourceLimits.max_output_bytes`, verificado antes de alocar. O índice temporário
de clusters é linear no número limitado de glifos; ordenação e pesquisa acrescentam
trabalho sem repetir shaping. Continuam ativos os limites de cena, texto e output.
Não há promessa de RSS constante ou ganho de velocidade.

Executa `cargo run -p appcore-filemaker --example pdf_unicode --locked` para criar
amostras Editable, Hybrid e Flattened. Incluem `fi/fl/ffi/ffl`, palavras correntes,
ligaturas literais, acentos compostos/decompostos, um cluster isolado com vários
glifos e recurso de fonte próprio, e clusters árabes.

Depois executa
`python3 crates/appcore-filemaker/wiki/audit/verify-pdf-unicode.py`.
Exige Poppler, renderiza cada modo, confere os escalares latinos exatos sem
normalização NFC/NFKC, verifica o exemplo RTL e a ausência de texto no Flattened,
e compara os pixels Hybrid/Flattened. Auditoria, preflight, extração e revisão
visual continuam a ser verificações distintas.

O verificador do corpus misto confere agora todo o texto dos elementos e das
células, além dos IDs e totais. PDFs existentes precisam de nova exportação.
O consumidor fixado no registry só recebe esta correção numa nova RC exata
publicada e qualificada. Sem publicação de pacotes, deploy da wiki ou migração
do consumidor externo. O rollback seleciona a release qualificada anterior do
exportador explicitamente.
