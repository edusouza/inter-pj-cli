# Receitas

Como usar a `inter-pj` em rotinas: scripts, cron e CI. Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção; os blocos de terminal rodam nos testes, como os dos [guias](guias/README.md), e os scripts mostram como juntar os comandos.

- [A conciliação do dia](#a-conciliação-do-dia)
- [Cobranças de uma planilha](#cobranças-de-uma-planilha)
- [Num cron ou num CI](#num-cron-ou-num-ci)
- [As notificações que não chegaram](#as-notificações-que-não-chegaram)

## A conciliação do dia

Todo dia, o extrato completo do dia anterior vai para o sistema contábil em CSV, com o identificador de cada transação e, de um Pix, o `endToEndId`:

```console
$ inter-pj extrato completo --inicio 2026-09-02 --fim 2026-09-02 --todas-paginas --formato csv > extrato-2026-09-02.csv
$ cat extrato-2026-09-02.csv
idTransacao,dataTransacao,dataInclusao,tipoTransacao,tipoOperacao,titulo,descricao,numeroDocumento,valor,contraparte,documentoContraparte,endToEndId,codigoBarras
310000115,2026-09-02,2026-09-02,PIX,C,Pix recebido,Cliente Exemplo Ltda,,1500.00,Cliente Exemplo Ltda,11222333000181,E12345678202609021215Po0iU9yT8rE,
```

Os Pix recebidos no dia trazem o txid da cobrança que pagaram, que liga o Pix do extrato à cobrança do sistema de vendas:

```console
$ inter-pj pix recebidos listar --inicio 2026-09-02 --fim 2026-09-02 --formato csv
endToEndId,txid,valor,horario,chave,infoPagador,valorDevolvido
E12345678202609021215Po0iU9yT8rE,pedido1053empresaexemplo2026,1500.00,2026-09-02T12:15:38.000Z,pix@empresa.example,Pedido 1053,0.00
```

Num script, para o cron da manhã:

```sh
#!/bin/sh
# conciliacao.sh: o extrato completo e os Pix recebidos de ontem, em CSV.
set -eu
ontem=$(TZ=America/Sao_Paulo date -d yesterday +%F)
destino=/srv/conciliacao
inter-pj extrato completo --inicio "$ontem" --fim "$ontem" --todas-paginas --formato csv \
  > "$destino/extrato-$ontem.csv.tmp"
mv "$destino/extrato-$ontem.csv.tmp" "$destino/extrato-$ontem.csv"
inter-pj pix recebidos listar --inicio "$ontem" --fim "$ontem" --formato csv \
  > "$destino/pix-$ontem.csv.tmp"
mv "$destino/pix-$ontem.csv.tmp" "$destino/pix-$ontem.csv"
```

As datas são dias do banco, em Brasília, qualquer que seja o fuso da máquina, e o `TZ` faz o `date` concordar: num servidor em UTC, entre as 21h e a meia-noite de Brasília, `date` sem ele já está no dia seguinte. O arquivo vai primeiro para um `.tmp` e só troca de nome quando o comando termina bem, para que uma falha no meio não deixe um extrato pela metade no lugar do certo; com `set -e`, o script para no primeiro erro e sai com o código dele (veja os [códigos de saída](../README.md#códigos-de-saída)). `--separador ';'` faz o CSV do Excel em português, e `date -d` é do GNU: no macOS, `date -v-1d +%F`.

## Cobranças de uma planilha

As cobranças com vencimento do Pix vão de uma planilha para a API num lote só, conferido inteiro antes do envio: `pix lote-cobv criar --arquivo`, no guia [Cobranças Pix](guias/cobrancas-pix.md#lotes-de-cobranças-com-vencimento). As cobranças com boleto são uma por pedido, e um script emite as linhas da planilha uma a uma. A planilha das notas de outubro, `notas.csv`, com o valor em ponto decimal:

<!-- guia: arquivo notas.csv -->
```csv
nota,documento,nome,valor,vencimento,endereco,cidade,uf,cep
NF-0924,11.222.333/0001-81,Cliente Exemplo Ltda,1200.00,2026-10-24,Avenida Brasil 1200,Belo Horizonte,MG,30110-000
NF-0925,012.345.678-90,Beltrana de Tal,890.00,2026-10-09,Rua dos Timbiras 45,Belo Horizonte,MG,30140-060
```

Cada linha vira um `cobranca emitir`, com `--sim`, porque não há um terminal para confirmar, e `--dias-agenda 30`, para que a cobrança aceite pagamentos até 30 dias depois do vencimento:

```console
$ inter-pj cobranca emitir --seu-numero NF-0924 --valor 1200.00 --vencimento 2026-10-24 \
    --pagador-documento 11.222.333/0001-81 --pagador-nome "Cliente Exemplo Ltda" \
    --pagador-endereco "Avenida Brasil 1200" --pagador-cidade "Belo Horizonte" --pagador-uf MG \
    --pagador-cep 30110-000 --dias-agenda 30 --sim
*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***
Cobrança a emitir
  Ambiente      PRODUÇÃO (conta real)
  Seu número    NF-0924
  Valor         R$ 1.200,00 (mil e duzentos reais)
  Vencimento    24/10/2026
  Pagador       Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço      Avenida Brasil 1200 - Belo Horizonte/MG - CEP 30110-000
  Cancelamento  23/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
Cobrança solicitada: a emissão termina em instantes.
Código  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Acompanhe com: inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
```

O script, que para no primeiro resultado incerto:

```sh
#!/bin/sh
# emitir.sh: uma cobrança para cada linha de notas.csv, depois do cabeçalho.
set -u
tail -n +2 notas.csv | while IFS=, read -r nota documento nome valor vencimento endereco cidade uf cep; do
  status=0
  inter-pj cobranca emitir --seu-numero "$nota" --valor "$valor" --vencimento "$vencimento" \
    --pagador-documento "$documento" --pagador-nome "$nome" --pagador-endereco "$endereco" \
    --pagador-cidade "$cidade" --pagador-uf "$uf" --pagador-cep "$cep" --dias-agenda 30 --sim || status=$?
  case $status in
    0) ;;
    9) echo "$nota: o resultado ficou incerto; confira com o comando da dica antes de repetir" >&2
       exit 9 ;;
    *) echo "$nota: não emitida (código $status)" >&2 ;;
  esac
done
```

O código 9 diz que a cobrança pode ter sido emitida: repetir às cegas pode emitir outra, e o script termina, para que alguém confira com o comando da dica. Os outros erros, como um CEP inválido, não emitem nada, e o script segue para a próxima linha. Com `--aguardar`, cada comando espera a emissão terminar, e sai com o código 5 quando a cobrança não é emitida. O `read` do shell não entende as aspas do CSV: na planilha, nenhum campo pode ter uma vírgula.

## Num cron ou num CI

Sem um arquivo de configuração, as variáveis de ambiente dizem tudo: `INTER_CLIENT_ID`, `INTER_CLIENT_SECRET`, `INTER_CERTIFICADO`, `INTER_CHAVE_PRIVADA`, `INTER_AMBIENTE` e, numa integração com mais de uma conta, `INTER_CONTA_CORRENTE`. Elas valem mais que o arquivo, e `config mostrar` diz de onde veio cada valor:

```console
$ INTER_AMBIENTE=sandbox inter-pj config mostrar
Perfil               padrao (arquivo)
Arquivo              /home/voce/.config/inter-pj/config.toml
Ambiente             sandbox (variável INTER_AMBIENTE)
client_id            **********************uias (arquivo)
client_secret        definido (oculto) (variável INTER_CLIENT_SECRET)
Certificado          /home/voce/inter/certificado.crt (arquivo)
Chave privada        /home/voce/inter/chave.key (arquivo)
Conta corrente       (não definido)
Escopos adicionais   (não definido)
Limite por operação  R$ 20.000,00 (arquivo)
URL base             (não definido)
```

Sem um terminal, o que pede confirmação é recusado sem `--sim`, e nada é enviado:

```console
$ inter-pj pix enviar --chave fornecedor@empresa.example --valor 10,00
*** PRODUÇÃO: este Pix movimenta dinheiro da conta real ***
Pix a enviar
  Ambiente               PRODUÇÃO (conta real)
  Chave Pix              fornecedor@empresa.example (e-mail)
  Valor                  R$ 10,00 (dez reais)
  Quando                 agora
  Chave de idempotência  8eaa3008-7c43-4491-a4f3-170f4fb4d190
erro: confirmação necessária: execute em um terminal, sem redirecionar a entrada nem a saída de erros, para ver o resumo e responder; ou use --sim para confirmar sem perguntar
```

Num job do GitHub Actions, com o certificado e a chave guardados como segredos do repositório:

```yaml
- name: Saldo do dia
  env:
    INTER_CLIENT_ID: ${{ secrets.INTER_CLIENT_ID }}
    INTER_CLIENT_SECRET: ${{ secrets.INTER_CLIENT_SECRET }}
    INTER_CERTIFICADO: ${{ runner.temp }}/inter.crt
    INTER_CHAVE_PRIVADA: ${{ runner.temp }}/inter.key
    INTER_CACHE_DIR: ${{ runner.temp }}/inter-cache
    CERTIFICADO_PEM: ${{ secrets.INTER_CERTIFICADO }}
    CHAVE_PEM: ${{ secrets.INTER_CHAVE_PRIVADA }}
  run: |
    umask 077
    printf '%s\n' "$CERTIFICADO_PEM" > "$INTER_CERTIFICADO"
    printf '%s\n' "$CHAVE_PEM" > "$INTER_CHAVE_PRIVADA"
    inter-pj saldo --json
```

O certificado e a chave vão para arquivos que só o job lê (`umask 077`), pelas variáveis, e não pelo texto do script. O token vale uma hora e fica no cache, e o Inter aceita só 5 pedidos de token por minuto: num job com vários passos, `INTER_CACHE_DIR` num diretório do job faz os passos usarem o mesmo token. `--json` dá a saída com os nomes de campo da API, para ler com `jq`, e as cores só aparecem num terminal. Num script, o [código de saída](../README.md#códigos-de-saída) diz o que aconteceu: 6 é uma falha que certamente não fez nada, e pode ser tentada de novo mais tarde; 9, um envio de resultado incerto, que precisa ser conferido antes. Para experimentar um script sem mexer na conta, use o perfil do sandbox (`-p sandbox` ou `INTER_PERFIL=sandbox`) ou `--simular`, que mostra a requisição sem enviar nada.

## As notificações que não chegaram

O histórico dos callbacks mostra as notificações que o servidor da empresa não recebeu. `--falhas` deixa só as tentativas que falharam, e a dica traz o reenvio das operações que ficaram sem nenhuma entrega:

```console
$ inter-pj webhook cobranca callbacks --inicio 2026-08-21 --fim 2026-08-21 --falhas
Callbacks do webhook de cobranças de 21/08/2026 00:00 a 21/08/2026 23:59 (só as falhas)

Disparo              Tentativa  Entregue  HTTP  Código da cobrança                    Erro
21/08/2026 06:00:02          5  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 04:00:02          4  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 03:00:02          3  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:30:02          2  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request
21/08/2026 02:10:02          1  não        400  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13  Bad Request

5 tentativas · 0 entregues · 5 falharam

Sem entrega no período: 1 operação. Para pedir o reenvio:
  inter-pj webhook cobranca reenviar 5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13
```

Uma falha só não é um problema: o Inter tenta de novo até 4 vezes, em até 4 horas, e a notificação pode chegar numa das tentativas seguintes. O que importa são as operações sem nenhuma entrega. No `--json`, cada tentativa traz o payload da notificação e se ela foi entregue (`sucesso`); um script agrupa as tentativas pelo código da cobrança e avisa das que não chegaram:

```sh
#!/bin/sh
# callbacks.sh: avisa das cobranças cujas notificações, desde ontem, não chegaram em nenhuma tentativa.
set -eu
ontem=$(TZ=America/Sao_Paulo date -d yesterday +%F)
historico=$(inter-pj webhook cobranca callbacks --inicio "$ontem" --json)
sem_entrega=$(printf '%s\n' "$historico" | jq -r '
  [.callbacks[] | {codigo: .payload[].codigoSolicitacao, sucesso}]
  | group_by(.codigo) | map(select(all(.sucesso | not)) | .[0].codigo) | .[]')
if [ -n "$sem_entrega" ]; then
  printf 'Cobranças sem notificação desde %s:\n%s\n' "$ontem" "$sem_entrega" |
    mail -s "Inter: notificações sem entrega" financeiro@empresa.example
fi
```

Sem `--fim`, o período vai até agora: rodando de manhã, as novas tentativas das notificações de ontem já terminaram, e só as das últimas 4 horas ainda podem ser entregues depois. O histórico fica numa variável antes do `jq`, para que um erro da CLI pare o script com o código dele, em vez de passar ao `jq` um histórico vazio. Corrigido o servidor, `webhook cobranca reenviar` com os códigos pede o reenvio, como no guia [Webhooks](guias/webhooks.md#pedir-o-reenvio). Nos webhooks do Pix e do Banking, o código de cada operação é outro campo do payload, o mesmo que `reenviar` recebe: o txid de cada Pix, o código da solicitação de um Pix enviado ou o da transação de um boleto pago.
