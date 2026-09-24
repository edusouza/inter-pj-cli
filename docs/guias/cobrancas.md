# Cobranças

Cobranças são boletos com Pix que a empresa emite para os clientes: o cliente paga pelo boleto, em qualquer banco, ou pelo Pix, lendo o QR Code, e o dinheiro entra na conta. Emitir não tira dinheiro da conta, mas a cobrança vai para o cliente de verdade. Por isso os trilhos são os dos pagamentos, menos o limite por operação: o resumo, a confirmação num terminal (ou `--sim`) e `--simular`.

Os exemplos são da Empresa Exemplo Ltda, uma empresa fictícia, no perfil de produção. Emitir precisa do escopo `boleto-cobranca.write`, e consultar e listar, do `boleto-cobranca.read`.

- [Emitir uma cobrança](#emitir-uma-cobrança)
- [A cobrança emitida](#a-cobrança-emitida)
- [O PDF do boleto](#o-pdf-do-boleto)
- [Por um arquivo, esperando a emissão](#por-um-arquivo-esperando-a-emissão)
- [Depois do vencimento](#depois-do-vencimento)
- [As cobranças de um período](#as-cobranças-de-um-período)
- [O resumo por situação](#o-resumo-por-situação)
- [Alterar o valor ou o vencimento](#alterar-o-valor-ou-o-vencimento)
- [Cancelar](#cancelar)
- [Quando o resultado fica incerto](#quando-o-resultado-fica-incerto)
- [Testar no sandbox](#testar-no-sandbox)

## Emitir uma cobrança

A nota NF-0924 da Cliente Exemplo Ltda vence em 30 dias, com 2% de desconto para quem pagar até 5 dias antes, e 2% de multa e 1% de juros ao mês para quem pagar depois:

```console
$ inter-pj cobranca emitir --seu-numero NF-0924 --valor 1.200,00 --vencimento 2026-10-24 \
    --pagador-documento 11.222.333/0001-81 --pagador-nome "Cliente Exemplo Ltda" \
    --pagador-endereco "Avenida Brasil" --pagador-numero 1200 --pagador-complemento "sala 3" \
    --pagador-bairro Centro --pagador-cidade "Belo Horizonte" --pagador-uf MG \
    --pagador-cep 30110-000 --pagador-email financeiro@cliente.example \
    --desconto 2% --desconto-dias 5 --multa 2% --juros 1% --dias-agenda 30
*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***
Cobrança a emitir
  Ambiente      PRODUÇÃO (conta real)
  Seu número    NF-0924
  Valor         R$ 1.200,00 (mil e duzentos reais)
  Vencimento    24/10/2026
  Pagador       Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço      Avenida Brasil, 1200, sala 3 - Centro - Belo Horizonte/MG - CEP 30110-000
  Contato       financeiro@cliente.example
  Desconto      2% para pagamentos até 19/10/2026
  Multa         2%
  Juros         1% ao mês
  Cancelamento  23/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
Emitir a cobrança? [s/N] s
Cobrança solicitada: a emissão termina em instantes.
Código  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Acompanhe com: inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
```

São obrigatórios o seu número (até 15 caracteres, como o número da nota), o valor (de R$ 2,50 a R$ 99.999.999,99), o vencimento (hoje ou depois) e, do pagador, o CPF ou o CNPJ, o nome, o endereço, a cidade, a UF e o CEP; o número, o complemento, o bairro, o e-mail e o telefone são opcionais. `--desconto`, `--multa` e `--juros` aceitam um percentual (`2%`) ou um valor (`4,00`): os juros são ao mês, em percentual, ou por dia, em valor. `--mensagem`, repetida, imprime até 5 linhas no boleto, e `--receber-com boleto` ou `pix` restringe as formas de pagamento.

A emissão termina depois do pedido: a API responde com o código da cobrança, e o boleto e o Pix ficam prontos em instantes.

## A cobrança emitida

`cobranca consultar` mostra a situação, os valores e os encargos, o boleto e o Pix, e `--qrcode` desenha o QR Code do Pix no terminal, para o cliente ler com o celular:

```console
$ inter-pj cobranca consultar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --qrcode
Cobrança NF-0924
  Situação    a receber
  Valor       R$ 1.200,00
  Vencimento  24/10/2026
  Pagador     Cliente Exemplo Ltda (11.222.333/0001-81)
  Emitida em  24/09/2026
  Tipo        simples
  Desconto    2% até 5 dias antes do vencimento
  Multa       2%
  Juros       1% ao mês
  Código      0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

Boleto
  Nosso número      0012345701
  Linha digitável   07790.00116 12001.234579 01000.000008 1 16090000120000
  Código de barras  07791160900001200000001112001234570100000000

Pix
  Copia e cola  00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0924empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304E4FE
  txid          cobv0924empresaexemplo20260924

█████████████████████████████████████████████████████████
█████████████████████████████████████████████████████████
████ ▄▄▄▄▄ █▄▀▄▄▀ ▀█▄▄ ▀▄ ▀█▄██ ▀▄▄█▄█▄▄▄▀▀▀ █ ▄▄▄▄▄ ████
████ █   █ █▄▄▄ █▀█▀██▄▄▄▄█  ▄▀█ ▄█▄▄ ▀██▄█ ▄█ █   █ ████
████ █▄▄▄█ █▀ ▀▄▄██▀▄▄ █▀▄ ▄▄▄ ▄█  ▀▄ ▄█▄▀▀███ █▄▄▄█ ████
████▄▄▄▄▄▄▄█▄▀▄▀▄▀▄▀ █ ▀ ▀ █▄█ █ ▀▄▀ █▄▀ █▄▀▄█▄▄▄▄▄▄▄████
████▄█▄▀▀█▄▄█ ▄▄▄▄█▀█▀ █ ▀▄▄ ▄▄▄ █▀ ██  ▀▄ ▀█▀█▄▀█▄▀ ████
█████ ▄▄█ ▄ █▄    ▄█ ▀▄▄█ ████▄██▀▀  ▀▀▀ ▀█▄██▄█▄▄▀  ████
████▄▀▄▄ ▄▄▀▀▄ ▄█▄ █▀▀▄█▄▄▄▀▄▄█▀▀▄▀   ▀ ▀█▀▄█   ▀▀██▄████
████▀ █▀▄ ▄▀ ▄▀▄▄▀██ ▀▄█▄█▄▀▀▄▄██ ▀▀▄▄▄▀  ▀  ▄ ▀ ▄█▄▄████
████▄██ ▀ ▄▄ ▄█▄█     ▀█ ▀▄▄▀▄█▄▄▄█▄ █ ▄██▄███▄▄▀▄█▄▄████
████▀▄ ▄  ▄▀▄ ▀  █ ▀▄▄ ███▀▀ ██▄▄▀███▀▄█▀▄█  █  ▄██▄ ████
████ ▄ ▄▀█▄▄██▀▀▄█▀▄▀█▀█▀█▀ ▄█▄▄▄▄  ▄██▄▀█▄▄▄▄▄ ▄ █▀▀████
████  ▀▀ ▄▄▄ ▄ ▄ ██▄ █▄▄▄  ▄▄▄ █ ██  ▀█▀▄▀█  ▄▄▄ ▄ ▄▄████
████▄▄▀  █▄█  ▀▄▀█▀▀▄████▄ █▄█ ▄▄█▄ ▀█▀▄▀▄▀▄ █▄█ ▀█▀█████
█████▀ ▄▄▄▄▄▄▄  ▀▄███▀▄▄ █ ▄  ▄ ▀██▀▄██  █▀     ▄▄▀▄▄████
█████ ▄█▄▀▄ ▄▀ █▄▀█▀ ▄   ▀▀█▄▄▄█▄▄  ▄█▀ ▀█  █▄█ ▀▀███████
████▄█▄▀▀▄▄▀█▀ ▄ ▀▀▀ ▄█ ▀▀▄█▄ ▀▄███▄ █▄▄ ▀▄▄ ▄▀ ▄▀▀ ▄████
████▄▀ █▀▄▄▀▄▀█▄ ▄▀▄██▄▀▀▀█▀▄▀ ▀█▄ ▀▄▄▄█ ██▄▀ ▄██▀██▄████
████ ▀▀▄ ▄▄█▀▄  ▄ █▀▄▀█▄ ▄█▄▀  ▄▀    ▄█▄▄█  █ █ █▄▄ ▄████
████▀█▀▀ █▄█▄█▄▀▀▄█▄▀▀▄█▀█▄█▀██  ▄▄▄██▄▄▄█▄▀▄▀ ██ ▄ ▀████
█████ ▀▀█▄▄█▄▀ █▄▄  ▀▀▀█ █ ▄███▄▄▄  ▄██  █  ▀  ▄▀██▄▀████
████▄▄▄███▄█▀▄ ██▄█▄ ▀██▀  ▄▄▄ ▄▄██  █▄▄ ▄▄▄ ▄▄▄ █▀  ████
████ ▄▄▄▄▄ █▄█ ▄ ▀▄▄▄██▄█  █▄█ ▄█ ▄ ▀██▀ █▀▄ █▄█ ▄▄▄█████
████ █   █ ██ ▄██▀▀ ▀▄ █▄      ▄▄▄  ██▄  ▄▄ ▄ ▄  ▀██▄████
████ █▄▄▄█ █▄ █▄ █▀ ▄  █▄▄█▄█ ▀ █ █▄▄▀ █ ▄▄█▀ █▄ ▀  ▄████
████▄▄▄▄▄▄▄█▄▄██▄███▄▄▄██▄█▄█▄▄▄▄▄█▄███▄▄▄▄█▄▄█▄█▄█▄▄████
█████████████████████████████████████████████████████████
▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
```

Antes de desenhar, a CLI confere o copia e cola (o CRC16). Em um terminal, o QR Code sai preto no branco, qualquer que seja o tema; sem cores, como aqui, com `NO_COLOR` ou com a saída redirecionada, os módulos claros é que são desenhados, como no `qrencode -t UTF8`, e o código fica certo em terminais de fundo escuro. Para imprimir ou enviar ao cliente, `--qrcode-png` grava o QR Code numa imagem.

## O PDF do boleto

`cobranca pdf` grava o boleto em PDF, como o cliente o recebe:

```console
$ inter-pj cobranca pdf 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d
Cobrança salva em cobranca-0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d.pdf (0,6 KB)
```

A imagem e o PDF são gravados com permissão `600` e não sobrescrevem um arquivo que já existe sem `--sobrescrever`; `-` os envia para a saída padrão.

## Por um arquivo, esperando a emissão

A cobrança pode vir de um arquivo JSON com os campos da API, que aceita também o beneficiário final e a nota fiscal. `cobranca modelo` imprime um exemplo com todos os campos, de dados fictícios, vencendo em 30 dias:

```console
$ inter-pj cobranca modelo > cobranca.json
```

Para a nota NF-0925 da Beltrana de Tal, o arquivo `cobranca.json` fica assim:

<!-- guia: arquivo cobranca.json -->
```json
{
  "seuNumero": "NF-0925",
  "valorNominal": "890,00",
  "dataVencimento": "2026-10-09",
  "numDiasAgenda": 30,
  "pagador": {
    "cpfCnpj": "012.345.678-90",
    "nome": "Beltrana de Tal",
    "endereco": "Rua dos Timbiras",
    "numero": "45",
    "cidade": "Belo Horizonte",
    "uf": "MG",
    "cep": "30140-060",
    "email": "beltrana@cliente.example"
  },
  "multa": {"codigo": "PERCENTUAL", "taxa": 2},
  "mora": {"codigo": "TAXAMENSAL", "taxa": 1},
  "mensagem": {"linha1": "Referente à NF 0925"}
}
```

Com `--aguardar`, a CLI consulta a cobrança a cada 6 segundos até a emissão terminar e a mostra, com o QR Code se `--qrcode` ou `--qrcode-png` forem pedidos:

```console
$ inter-pj cobranca emitir --arquivo cobranca.json --aguardar --qrcode-png pix.png
*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***
Cobrança a emitir
  Ambiente      PRODUÇÃO (conta real)
  Seu número    NF-0925
  Valor         R$ 890,00 (oitocentos e noventa reais)
  Vencimento    09/10/2026
  Pagador       Beltrana de Tal (012.345.678-90)
  Endereço      Rua dos Timbiras, 45 - Belo Horizonte/MG - CEP 30140-060
  Contato       beltrana@cliente.example
  Multa         2%
  Juros         1% ao mês
  Cancelamento  08/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
  Mensagem      Referente à NF 0925
Emitir a cobrança? [s/N] s
Cobrança NF-0925
  Situação    a receber
  Valor       R$ 890,00
  Vencimento  09/10/2026
  Pagador     Beltrana de Tal (012.345.678-90)
  Emitida em  24/09/2026
  Tipo        simples
  Multa       2%
  Juros       1% ao mês
  Código      6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17

Boleto
  Nosso número      0012345712
  Linha digitável   07790.00116 12001.234579 12000.000005 7 15940000089000
  Código de barras  07797159400000890000001112001234571200000000

Pix
  Copia e cola  00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0925empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304825F
  txid          cobv0925empresaexemplo20260924
QR Code salvo em pix.png (25,9 KB)
```

No arquivo, campos desconhecidos são recusados, e as mensagens apontam o campo (`cobranca.json, campo "pagador.cep": ...`); os valores podem ser números (`890.00`) ou textos (`"890,00"`), o CEP e o CPF ou o CNPJ podem ter pontuação, e `tipoPessoa` pode ficar de fora, pois vem do documento. A chave de acesso da nota fiscal é conferida: o dígito verificador, o número e a série. Com `--arquivo -`, a cobrança vem da entrada padrão, e a confirmação exige `--sim`.

Com `--aguardar`, a CLI sai com o código 5 se a emissão falhar e com o 8 se o tempo acabar (`--timeout`, de 60 segundos por padrão), e a emissão precisa também do escopo `boleto-cobranca.read`.

## Depois do vencimento

`--dias-agenda` (`numDiasAgenda` no arquivo) é por quantos dias depois do vencimento a cobrança não paga continua valendo, de 0 a 60. O padrão da API, 0, cancela a cobrança no vencimento: um pagamento atrasado não é aceito, e a multa e os juros nunca chegam a valer. O resumo avisa:

```console
$ inter-pj cobranca emitir --seu-numero NF-0926 --valor 450,00 --vencimento 2026-10-15 \
    --pagador-documento 123.456.789-09 --pagador-nome "Fulano de Tal" \
    --pagador-endereco "Rua da Bahia" --pagador-numero 1000 --pagador-cidade "Belo Horizonte" \
    --pagador-uf MG --pagador-cep 30160-011 --multa 2% --juros 1% --simular
*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***
Cobrança a emitir
  Ambiente      PRODUÇÃO (conta real)
  Seu número    NF-0926
  Valor         R$ 450,00 (quatrocentos e cinquenta reais)
  Vencimento    15/10/2026
  Pagador       Fulano de Tal (123.456.789-09)
  Endereço      Rua da Bahia, 1000 - Belo Horizonte/MG - CEP 30160-011
  Multa         2%
  Juros         1% ao mês
  Cancelamento  no vencimento, se não for paga: pagamentos atrasados não são aceitos
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
aviso: multa e juros não chegam a valer: sem --dias-agenda (numDiasAgenda), a cobrança é cancelada no vencimento
Simulação: nada foi enviado.

POST https://cdpj.partners.bancointer.com.br/cobranca/v3/cobrancas

{
  "dataVencimento": "2026-10-15",
  "mora": {
    "codigo": "TAXAMENSAL",
    "taxa": 1
  },
  "multa": {
    "codigo": "PERCENTUAL",
    "taxa": 2
  },
  "numDiasAgenda": 0,
  "pagador": {
    "cep": "30160011",
    "cidade": "Belo Horizonte",
    "cpfCnpj": "12345678909",
    "endereco": "Rua da Bahia",
    "nome": "Fulano de Tal",
    "numero": "1000",
    "tipoPessoa": "FISICA",
    "uf": "MG"
  },
  "seuNumero": "NF-0926",
  "valorNominal": 450
}
```

Foi o que aconteceu com a nota NF-0805 do Fulano de Tal, emitida sem `--dias-agenda`: vencida sem pagamento, ela expirou, e a de setembro, com 30 dias, ainda pode ser paga, com a multa e os juros. O resumo também avisa quando o prazo do desconto já passou e quando a cobrança vence hoje, o que só é aceito até as 19h59 (horário de Brasília).

## As cobranças de um período

`cobranca listar` mostra as cobranças de um período, pelo vencimento (o padrão), pela emissão ou pelo pagamento (`--filtrar-por`); sem datas, as com vencimento nos últimos 30 dias:

```console
$ inter-pj cobranca listar --inicio 2026-08-01 --fim 2026-10-31
Cobranças com vencimento de 01/08/2026 a 31/10/2026

Vencimento  Seu número  Pagador               Situação                                  Valor  Código
10/08/2026  NF-0815     Beltrana de Tal       recebida                              R$ 890,00  8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61
20/08/2026  NF-0805     Fulano de Tal         expirada (cancelada sem pagamento)    R$ 300,00  5c2e8a41-7d3b-4f6e-9a1c-2b4d6f8e0a13
15/09/2026  NF-0830     Fulano de Tal         atrasada                              R$ 450,00  2a4c6e8f-0b2d-4f6a-8c0e-4a6c8e0f2b43
09/10/2026  NF-0925     Beltrana de Tal       a receber                             R$ 890,00  6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17
10/10/2026  NF-0910     Cliente Exemplo Ltda  a receber                           R$ 2.350,00  9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25
24/10/2026  NF-0924     Cliente Exemplo Ltda  a receber                           R$ 1.200,00  0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d

6 cobranças · R$ 6.080,00 · recebido R$ 890,00

$ inter-pj cobranca listar --situacao atrasada
Cobranças com vencimento de 26/08/2026 a 24/09/2026 (atrasada)

Vencimento  Seu número  Pagador        Situação      Valor  Código
15/09/2026  NF-0830     Fulano de Tal  atrasada  R$ 450,00  2a4c6e8f-0b2d-4f6a-8c0e-4a6c8e0f2b43

1 cobrança · R$ 450,00

$ inter-pj cobranca listar --filtrar-por pagamento --inicio 2026-08-01 --fim 2026-09-30
Cobranças pagas de 01/08/2026 a 30/09/2026

Vencimento  Seu número  Pagador          Situação      Valor  Código
10/08/2026  NF-0815     Beltrana de Tal  recebida  R$ 890,00  8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61

1 cobrança · R$ 890,00 · recebido R$ 890,00
```

Os filtros são `--situacao` (`a-receber`, `recebida`, `atrasada`, `cancelada`, `expirada`, `marcada-recebida`, `em-processamento`, `falha-emissao` ou `protesto`), `--pagador` (parte do nome), `--documento` (o CPF ou o CNPJ, conferido), `--seu-numero` e `--tipo` (`simples`, `parcelada` ou `recorrente`). A listagem lê todas as páginas, de 1.000 cobranças cada; `--pagina N` (a primeira é 0), com `--itens-por-pagina`, traz uma só, e `--ordenar-por`, com `--decrescente`, escolhe a ordem. Em `--formato csv`, as colunas têm os nomes da API, com os campos aninhados separados por ponto (`pagador.nome`, `boleto.linhaDigitavel`, `pix.pixCopiaECola`).

## O resumo por situação

`cobranca sumario` soma as cobranças do período por situação, com os mesmos filtros:

```console
$ inter-pj cobranca sumario --inicio 2026-08-01 --fim 2026-10-31
Cobranças com vencimento de 01/08/2026 a 31/10/2026

Situação                            Quantidade        Valor
a receber                                    3  R$ 4.440,00
atrasada                                     1    R$ 450,00
recebida                                     1    R$ 890,00
expirada (cancelada sem pagamento)           1    R$ 300,00
Total                                        6  R$ 6.080,00
```

## Alterar o valor ou o vencimento

Uma cobrança ainda não paga pode ter o valor e o vencimento alterados; o resto, a API não altera. A Cliente Exemplo pediu para pagar a nota NF-0910 dez dias depois, com o valor corrigido. A CLI primeiro consulta a cobrança e mostra o que vai mudar:

```console
$ inter-pj cobranca editar 9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25 --valor 2.400,00 --vencimento 2026-10-20
Cobrança a alterar
  Ambiente    PRODUÇÃO (conta real)
  Seu número  NF-0910
  Situação    a receber
  Valor       R$ 2.350,00 → R$ 2.400,00
  Vencimento  10/10/2026 → 20/10/2026
  Pagador     Cliente Exemplo Ltda (11.222.333/0001-81)
  Código      9d7b5f3e-1c0a-4e8f-9b7d-5f3e1c0a8e25
aviso: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento
Alterar a cobrança? [s/N] s
Alteração em processamento.
Código da alteração  3c5e7a9b-1d2f-4a6c-8e0b-2d4f6a8c0e19

Acompanhe com: inter-pj cobranca edicao 3c5e7a9b-1d2f-4a6c-8e0b-2d4f6a8c0e19 --aguardar
```

A alteração é processada depois do pedido. `cobranca edicao` mostra em que pé ela está e, com `--aguardar` (aceito também por `editar`), consulta a cada 6 segundos até o fim, saindo com o código 0 quando a alteração é feita, 5 quando não é e 8 quando o tempo acaba (`--timeout`, de 60 segundos por padrão):

```console
$ inter-pj cobranca edicao 3c5e7a9b-1d2f-4a6c-8e0b-2d4f6a8c0e19 --aguardar
Alteração feita: a consulta pode levar até 30 minutos para mostrar o novo valor ou vencimento.
Código da alteração  3c5e7a9b-1d2f-4a6c-8e0b-2d4f6a8c0e19
```

Mesmo feita, a alteração pode levar até 30 minutos para aparecer em `cobranca consultar`. O novo valor vai de R$ 2,50 a R$ 99.999.999,99, e o novo vencimento é hoje ou depois. Uma cobrança paga, cancelada ou expirada é recusada antes de qualquer alteração:

```console
$ inter-pj cobranca editar 8e1f3a5c-7b9d-4e2f-8a4c-6e8f0a2c4e61 --valor 900,00 --sim
erro: a cobrança já foi paga: não pode ser alterada
```

## Cancelar

A Beltrana de Tal desistiu do pedido da nota NF-0925. O cancelamento pede um motivo, de até 50 caracteres, e, como a alteração, mostra a cobrança antes de pedir a confirmação:

```console
$ inter-pj cobranca cancelar 6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17 --motivo "Pedido cancelado pela cliente"
Cobrança a cancelar
  Ambiente    PRODUÇÃO (conta real)
  Seu número  NF-0925
  Situação    a receber
  Valor       R$ 890,00
  Vencimento  09/10/2026
  Pagador     Beltrana de Tal (012.345.678-90)
  Código      6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17
  Motivo      Pedido cancelado pela cliente
Cancelar a cobrança? [s/N] s
Cancelamento solicitado.

Confira com: inter-pj cobranca consultar 6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17

$ inter-pj cobranca consultar 6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17
Cobrança NF-0925
  Situação    cancelada
  Valor       R$ 890,00
  Vencimento  09/10/2026
  Motivo      Pedido cancelado pela cliente
  Pagador     Beltrana de Tal (012.345.678-90)
  Emitida em  24/09/2026
  Tipo        simples
  Multa       2%
  Juros       1% ao mês
  Código      6f4d2b0e-8c6a-4e4f-9d2b-0e8c6a4f2d17

Boleto
  Nosso número      0012345712
  Linha digitável   07790.00116 12001.234579 12000.000005 7 15940000089000
  Código de barras  07797159400000890000001112001234571200000000

Pix
  Copia e cola  00020101021226810014br.gov.bcb.pix2559qrcodepix.inter.example/cobv/cobv0925empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304825F
  txid          cobv0925empresaexemplo20260924
```

Os dois comandos, `editar` e `cancelar`, pedem a confirmação; sem um terminal, exigem `--sim`, e sem ele nem a consulta é feita. Precisam do escopo `boleto-cobranca.write`, além do `boleto-cobranca.read` para a consulta, e a API aceita até 10 alterações por minuto.

## Quando o resultado fica incerto

Esta API não tem chave de idempotência, mas, por 30 minutos, recusa outra cobrança com o mesmo seu número, valor, vencimento e pagador. Se a resposta da emissão se perder (um tempo esgotado, um erro 5xx), a cobrança pode ter sido emitida: a CLI sai com o código 9 e mostra como procurá-la.

```console
$ inter-pj cobranca emitir --seu-numero NF-0927 --valor 640,00 --vencimento 2026-10-27 \
    --pagador-documento 11.222.333/0001-81 --pagador-nome "Cliente Exemplo Ltda" \
    --pagador-endereco "Avenida Brasil" --pagador-numero 1200 --pagador-cidade "Belo Horizonte" \
    --pagador-uf MG --pagador-cep 30110-000 --dias-agenda 30 --sim
*** PRODUÇÃO: a cobrança vai para o cliente de verdade ***
Cobrança a emitir
  Ambiente      PRODUÇÃO (conta real)
  Seu número    NF-0927
  Valor         R$ 640,00 (seiscentos e quarenta reais)
  Vencimento    27/10/2026
  Pagador       Cliente Exemplo Ltda (11.222.333/0001-81)
  Endereço      Avenida Brasil, 1200 - Belo Horizonte/MG - CEP 30110-000
  Cancelamento  26/11/2026, 30 dias após o vencimento, se não for paga
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
erro: POST /cobranca/v3/cobrancas respondeu 504 (tempo esgotado no gateway)
dica: a cobrança pode ter sido emitida; por 30 minutos, a API recusa outra com o mesmo seu número, valor, vencimento e pagador
dica: confira antes de tentar de novo: inter-pj cobranca listar --filtrar-por emissao --seu-numero NF-0927

$ inter-pj cobranca listar --filtrar-por emissao --seu-numero NF-0927
Cobranças emitidas de 26/08/2026 a 24/09/2026 (seu número NF-0927)

Vencimento  Seu número  Pagador               Situação       Valor  Código
27/10/2026  NF-0927     Cliente Exemplo Ltda  a receber  R$ 640,00  7a9c1e3f-5b7d-4f9a-8c2e-4f6a8c0e2b58

1 cobrança · R$ 640,00
```

A cobrança foi emitida: não emita de novo. Passados os 30 minutos, a API aceitaria uma segunda cobrança igual, e o cliente receberia duas.

## Testar no sandbox

No sandbox, `cobranca pagar` paga uma cobrança, com o boleto ou o Pix, para testar o fluxo inteiro: emitir, pagar, consultar e, com um webhook cadastrado, receber a notificação. Em produção, quem paga é o cliente, e o comando é recusado antes de qualquer requisição:

```console
$ inter-pj cobranca pagar 0b7e4c1a-5d3f-4a2b-9c8d-7e6f5a4b3c2d --com pix
erro: cobranca pagar existe só no sandbox, para testes: em produção, quem paga é o cliente, com o boleto ou o Pix
```

No perfil do sandbox, com uma cobrança de teste:

```console
$ inter-pj -p sandbox cobranca emitir --seu-numero TESTE-1 --valor 10,00 --vencimento 2026-09-30 \
    --pagador-documento 123.456.789-09 --pagador-nome "Fulano de Tal" \
    --pagador-endereco "Rua da Bahia" --pagador-cidade "Belo Horizonte" --pagador-uf MG \
    --pagador-cep 30160-011
Cobrança a emitir
  Ambiente      sandbox (dados fictícios)
  Seu número    TESTE-1
  Valor         R$ 10,00 (dez reais)
  Vencimento    30/09/2026
  Pagador       Fulano de Tal (123.456.789-09)
  Endereço      Rua da Bahia - Belo Horizonte/MG - CEP 30160-011
  Cancelamento  no vencimento, se não for paga: pagamentos atrasados não são aceitos
  Recebimento   boleto e Pix (se a conta tiver chave Pix)
Emitir a cobrança? [s/N] s
Cobrança solicitada: a emissão termina em instantes.
Código  1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74

Acompanhe com: inter-pj -p sandbox cobranca consultar 1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74

$ inter-pj -p sandbox cobranca pagar 1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74 --com pix
Cobrança paga no sandbox, com o Pix.

Confira com: inter-pj -p sandbox cobranca consultar 1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74

$ inter-pj -p sandbox cobranca consultar 1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74
aviso: ambiente sandbox — os dados retornados são fictícios
Cobrança TESTE-1
  Situação    recebida
  Valor       R$ 10,00
  Vencimento  30/09/2026
  Recebido    R$ 10,00 por Pix em 24/09/2026
  Pagador     Fulano de Tal (123.456.789-09)
  Emitida em  24/09/2026
  Tipo        simples
  Código      1e3a5c7d-9f1b-4d3e-8a5c-7e9b1d3f5a74

Boleto
  Nosso número      0012345734
  Linha digitável   07790.00116 12001.234579 34000.000009 3 15850000001000
  Código de barras  07793158500000010000001112001234573400000000

Pix
  Copia e cola  00020101021226830014br.gov.bcb.pix2561qrcodepix.inter.example/cobv/cobvteste1empresaexemplo202609245204000053039865802BR5920EMPRESA EXEMPLO LTDA6014BELO HORIZONTE62070503***6304A4D7
  txid          cobvteste1empresaexemplo20260924
```

O pagamento precisa do escopo `boleto-cobranca.write`.
