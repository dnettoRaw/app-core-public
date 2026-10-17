# Migração e recuperação de Update no host

Fonte não publicada; API aditiva de recuperação e admissão automática mais restrita. Versões publicadas não recebem alterações retroativas. Selecionar futura versão imutável que as inclua, atualizar matches exaustivos de `UpdateError` na migração explícita 2.x e manter consumidor apenas registry.

## Gates de cutover

1. Inventariar build/pacote instalado, ativação V1 pendente, journal host, fence/recibo do coordenador, snapshot e instância. Suspender instalações e drenar escritas sob locks de instalação/storage. Resolver ambiguidades observando o host; não reinterpretar fase local `prepared` como `Prepared` Runtime V2.
2. Conservar executável/configuração antigos, backup verificado e journals. Resolver/arquivar tentativa antiga antes de selecionar root V2 separado. Uma fonte autoritativa de lifecycle por camada; nunca executar instaladores antigos/novos em paralelo nem converter strings dos journals automaticamente. Migração de schemas fica na aplicação.
3. Vincular attempt Runtime e `host_binding` à instalação, fence do coordenador, instância e snapshot originais. O binding opaco limitado pode identificar registo confiável do host; sem credenciais. Stage apenas de artefactos autenticados/compatíveis, mesma aplicação/canal/target, versão avançada e build/digest distintos. Migrar canal é procedimento explícito separado.
4. Persistir intenção, drenar/snapshot e selar escritas antes de ativar. Após resposta perdida, observar bytes/assinatura e tentativa/instância/snapshot exatos sob o mesmo lock. Só evidência ativa exata pode chamar `reconcile_activated`; ambiguidade mantém bloqueio/revisão manual. Depois health real e commit explícito. Consultar recibo original do coordenador após report perdido; não reservar tentativa nova para ocultar incerteza.
5. Conservar artefactos/dados de rollback até health e conclusão recuperável. Re-observar identidade restaurada antes de `RecordRolledBack`. Depois de aceitar novas escritas, restaurar snapshot antigo pode perdê-las: exige plano de dados compatível da aplicação, não rollback automático do journal anterior.

## Adapters separados

| Host | Evidência observada exigida | Fronteira de migração |
| --- | --- | --- |
| Desktop | Pacote/build instalado, tentativa, instalação/snapshot, processo e renderer prontos | Preservar handoff de plataforma, UI candidata, cancelamento e sucessor; exit de processo ou HTTP health isolado não basta |
| Serviço binário / deployment systemd | Executável/build verificado, instância supervisionada, posse exclusiva de serviço/dados, heartbeat/readiness | Preservar parent/child, stop/start limitado e drain de órfão; unit active não prova build exato |
| Deployment container | Digest imutável da imagem, identidade do container, operação de cópia/volume candidato, gate fechado | Preservar fronteira committed/released e volumes; repetir reports finais sem restaurar dados antigos depois de reabrir escritas |

Adapters pertencem ao deployment; Runtime não importa instalador, gestor de processos, cliente de containers ou schema da aplicação. Recibo Runtime V2, journal host e recibo remoto descrevem factos distintos. `adapter.commit` corre antes do recibo terminal Runtime; entrar nesse callback não autoriza reabrir escritas. Manter finalização remota e ponto irreversível de libertação explícitos em cada perfil, sem mapear todas as fases num único enum.

## Qualificação e rollback

Qualificar truncamento/cancelamento, assinatura inválida, disco cheio, interrupções de instalação/boot/commit e respostas finais perdidas. Verificar identidade exata, bloqueio na incerteza, health antes de conclusão e rollback preservando dados. Adapters simulados locais qualificam apenas o contrato; desktop nativo, serviço Linux e container real têm gates separados.

Rollback da migração da biblioteca interrompe o candidato e restaura executável/journal/dados anteriores compatíveis sob posse exclusiva, após observar o host real. Não misturar evidência V2 com processo 1.x, apagar recibo pendente, reutilizar attempt/fence ou reiniciar writer antigo para contornar recuperação. Evidência insuficiente mantém escritas bloqueadas para revisão manual.
