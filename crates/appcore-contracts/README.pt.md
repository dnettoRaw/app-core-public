# appcore-contracts

**Responsabilidade:** manifests e policies estáveis, independentes de
implementação.

**Dependências internas:** nenhuma.

**API principal:** `ApplicationManifestV1`, `DeploymentManifestV1`,
`DeploymentManifestBuilder`, `RuntimeManifestV1`, `RuntimeMode`,
`RuntimeOperationalMode`, policies de capability/storage/leadership/job/
scheduler/health/update/module, configuração de provider/network/TLS/volume/
environment e `ContractError`.

Use para parse, build e validação de contratos portáteis. Preserve nomes
serializados e significados. Não adicione transport, filesystem, processo ou
negócio.

**Maturidade:** superfície RC estável. Mudanças V1 devem ser aditivas e
compatíveis na linha 1.0.
