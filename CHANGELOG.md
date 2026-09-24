# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- Biblioteca: `InterClient::pix`, a API Pix, com as cobranças imediatas (`cob`): `Pix::criar_cob` (com o seu `txid`, que torna segura a repetição de uma criação de resultado incerto), `criar_cob_sem_txid`, `revisar_cob` (inclusive a remoção), `consultar_cob` (com os Pix que a pagaram e suas devoluções) e `listar_cobs`/`listar_todas_cobs` por período de criação, com filtros de CPF/CNPJ, location e status. `CobSolicitada::validar` confere localmente a chave Pix, os valores (até 10 dígitos e 2 casas), a expiração, o devedor, os textos, as informações adicionais e as regras do Pix Saque e do Pix Troco; `Txid` valida (26 a 35 letras e dígitos) e gera txids (#34).

## [0.5.0] - 2026-09-24

Cobranças: boletos com Pix para os clientes da empresa, da emissão ao cancelamento, com o QR Code do Pix no terminal e em PNG.

### Adicionado

- `inter-pj cobranca emitir`: emite uma cobrança com boleto e Pix pelas opções (casos simples) ou por `--arquivo` (JSON com os campos da API, inclusive beneficiário final e nota fiscal, ou `-` para a entrada padrão), conferida localmente com mensagens que apontam a opção ou o campo; campos desconhecidos no arquivo são recusados. O resumo mostra o valor por extenso, até quando vale o desconto e quando a cobrança não paga é cancelada, e avisa sobre multa e juros que não chegam a valer (`--dias-agenda` 0 cancela a cobrança no vencimento), prazo de desconto já passado e vencimento no mesmo dia (só até as 19h59). Confirmação, `--sim` e `--simular`; sem chave de idempotência, um resultado incerto vem com o comando que procura a cobrança antes de emiti-la de novo. Com `--aguardar [--timeout 60s]`, espera a emissão e mostra o boleto e o Pix, com `--qrcode` e `--qrcode-png` (#28).
- `inter-pj cobranca modelo`: imprime um arquivo de exemplo para `cobranca emitir --arquivo`, com dados fictícios e vencimento em 30 dias (#28).
- `inter-pj cobranca consultar <codigo>`: situação, valores, encargos, pagador, boleto (linha digitável formatada e código de barras) e Pix de uma cobrança, em texto ou JSON; `--qrcode` desenha o QR Code do Pix no terminal (preto no branco em qualquer tema; sem cores com `NO_COLOR` ou saída redirecionada) e `--qrcode-png` o grava em PNG, depois de conferir o CRC do copia e cola (#29, #31).
- `inter-pj cobranca pdf <codigo> [--saida ARQUIVO|-] [--sobrescrever]`: PDF da cobrança com o boleto e o QR Code, gravado com permissão 600 (#29).
- `inter-pj cobranca listar`: cobranças de um período por vencimento (padrão), emissão ou pagamento, com filtros de situação, pagador, CPF/CNPJ, seu número e tipo, ordem e todas as páginas (ou `--pagina`), em texto com totais, JSON ou CSV com os nomes da API (#29).
- `inter-pj cobranca sumario`: quantidade e valor das cobranças do período por situação, com total, em texto, JSON ou CSV (#29).
- `inter-pj cobranca cancelar <codigo> --motivo TEXTO`: cancela uma cobrança depois de consultá-la, mostrá-la e pedir confirmação; cobranças pagas, canceladas ou expiradas são recusadas sem nenhuma alteração, e, sem terminal nem `--sim`, nenhuma requisição é feita (#30).
- `inter-pj cobranca editar <codigo> [--valor] [--vencimento]`: altera o valor e o vencimento, os únicos campos que a API aceita, mostrando o antes e o depois; com `--aguardar`, acompanha a alteração até o fim, saindo com 5 se ela não foi feita e 8 se o tempo acabou (#30).
- `inter-pj cobranca edicao <codigo-edicao> [--aguardar]`: mostra em que pé está uma alteração (#30).
- `inter-pj cobranca pagar <codigo> --com boleto|pix`: paga uma cobrança no sandbox, para testar o fluxo completo; em produção, é recusado antes de qualquer requisição (#32).
- Biblioteca: `InterClient::cobranca`, com `Cobranca::emitir` (cobrança com boleto e Pix, conferida localmente: seu número, valor, pagador com CPF/CNPJ, UF e CEP, desconto, multa, mora, mensagem, formas de recebimento e nota fiscal, cuja chave de acesso confere número e série; o tipo de pessoa vem do documento) e `Cobranca::consultar` (situação, valores, encargos, boleto, Pix e nota fiscal) (#28).
- Biblioteca: `Cobranca::listar` e `listar_todas` (período por vencimento, emissão ou pagamento, situação, pagador, seu número, tipo e ordenação, com páginas de até 1.000), `sumario` (quantidade e valor por situação), `pdf`, `cancelar` (com o motivo conferido por `cobranca::motivo_cancelamento`), `editar` (vencimento e valor) com `consultar_edicao`, e `pagar_no_sandbox`, recusado fora do sandbox sem nenhuma requisição (#29, #30, #32).

### Alterado

- Os códigos de saída 5 e 8 valem também para `cobranca emitir`, `cobranca editar` e `cobranca edicao` com `--aguardar` (cobrança não emitida ou alteração não feita; tempo esgotado).

## [0.4.0] - 2026-09-23

Pagamentos: boletos, contas e tributos, DARF e lotes, com os trilhos de segurança do Pix.

### Adicionado

- `inter-pj pagamento boleto pagar <codigo>`: paga ou agenda (`--data`) boletos, contas de consumo e tributos pela linha digitável ou pelo código de barras, conferidos localmente. Valor e vencimento vêm do código quando ele os traz, e o resumo avisa, mostrando os dois lados, quando `--valor` ou `--vencimento` diferem dele, e também quando o pagamento fica para depois do vencimento; `--beneficiario` pede à API que confira o CPF/CNPJ de quem recebe. Mesmos trilhos do Pix: confirmação, `--sim`, `--simular` e `limite_por_operacao`. Como a API não tem chave de idempotência, um resultado incerto vem com o comando que confere o pagamento antes de repeti-lo (#23).
- `inter-pj pagamento boleto listar`: pagamentos por código de barras de um período (até 90 dias; padrão: incluídos nos últimos 30 dias), por data de inclusão, pagamento ou vencimento (`--filtrar-por`), código (`--codigo`) ou transação (`--codigo-transacao`), em texto, JSON ou CSV (#24).
- `inter-pj pagamento boleto cancelar <codigo-transacao>`: cancela um agendamento depois de mostrá-lo (beneficiário, valor, data e status) e pedir confirmação; sem terminal, exige `--sim` e não faz nenhuma requisição (#24).
- `inter-pj pagamento darf pagar`: DARF sem código de barras pelas opções ou por `--arquivo` (JSON com os campos da API, ou `-` para a entrada padrão), validado localmente com mensagens que apontam o campo; campos desconhecidos no arquivo são recusados. O resumo mostra principal, multa, juros e o total por extenso, e avisa sobre DARF vencido sem acréscimos. Mesmos trilhos dos demais pagamentos (#25).
- `inter-pj pagamento darf listar`: DARFs pagos em um período, ou incluídos nos últimos 30 dias, por código da receita ou da solicitação, em texto, JSON ou CSV (#25).
- `inter-pj pagamento lote enviar --arquivo`: lotes de 2 a 150 boletos, contas, tributos e DARFs a partir de um arquivo JSON (os campos da API, com `tipoPagamento`) ou de uma planilha CSV (`,` ou `;`, com ou sem BOM, como o Excel salva). O lote inteiro é conferido antes do envio, e todos os problemas são listados com a linha ou a posição e o campo, sem enviar nada; o resumo mostra os totais por tipo, cada pagamento e avisos (vencimentos passados, valores diferentes dos do código, pagamentos repetidos no arquivo). Mesmos trilhos dos demais pagamentos (#26).
- `inter-pj pagamento lote consultar <id-lote>`: status do lote e de cada pagamento; com `--aguardar [--timeout 5m]`, sai com 0 (processado sem erro), 5 (algum pagamento não foi feito) ou 8 (tempo esgotado) (#26).
- `inter-pj pagamento lote modelo [json|csv]`: arquivos de exemplo com dados fictícios e os códigos de teste do sandbox. No CSV, as células que o Excel estraga ao abrir o arquivo (números longos em notação científica, códigos da receita sem o zero à esquerda) são recusadas com a explicação (#26).
- Biblioteca: `boleto::CodigoBarras` valida e decodifica localmente a linha digitável (47 dígitos para boletos, 48 para contas e tributos) e o código de barras (44 dígitos): dígitos verificadores (módulos 10 e 11), conversão entre linha e código, banco, segmento, valor e vencimento, considerando o reinício do fator de vencimento em 22/02/2025 (#23).
- Biblioteca: `Banking::pagar_boleto` (boletos, contas e tributos com código de barras, com validação local do valor), `Banking::pagamentos` (filtros por período, tipo de data, código e transação) e `Banking::cancelar_pagamento` (agendamentos) (#23, #24).
- Biblioteca: `Banking::pagar_darf` (DARF sem código de barras, com validação local do código da receita, da referência, dos textos e dos valores; `PagamentoDarfError::campo` diz qual campo falhou) e `Banking::darfs` (filtros por período, código da receita e solicitação) (#25).
- Biblioteca: `Banking::enviar_lote` (lotes de 2 a 150 boletos e DARFs, validados item a item) e `Banking::consultar_lote` (status do lote e de cada pagamento) (#26).

### Alterado

- `limite_por_operacao` vale também para os pagamentos: boletos, DARFs e cada pagamento de um lote.
- Os códigos de saída 5 e 8 valem também para `pagamento lote consultar --aguardar` (lote processado com erro; tempo esgotado).

### Corrigido

- A saída em texto neutraliza caracteres de controle vindos da API (nomes, descrições, mensagens de erro): sequências de escape e quebras de linha não chegam mais ao terminal, onde poderiam reescrever a tela ou simular linhas do extrato e das consultas. JSON e CSV continuam com o texto original.

## [0.3.0] - 2026-09-23

Pix: envio e consulta, com trilhos de segurança.

### Adicionado

- `inter-pj pix enviar`: Pix por chave (`--chave`), código copia e cola (`--copia-e-cola`) ou dados bancários (`--ispb`, `--agencia`, `--conta`, `--tipo-conta`, `--documento`, `--nome`), com descrição e agendamento (`--data`) (#20).
- Trilhos de segurança do envio (#19):
  - validação local da chave (CPF/CNPJ com dígitos verificadores, inclusive o CNPJ alfanumérico; e-mail; celular `+55`; chave aleatória), do valor, da descrição e dos dados bancários;
  - resumo antes do envio, com o valor por extenso e o ambiente de produção em destaque;
  - confirmação `[s/N]` digitada em um terminal (respostas de *pipe* não valem) ou `--sim` para scripts;
  - `--simular`, que mostra a requisição sem enviar nada;
  - chave de idempotência exibida e `--id-idempotente` para repetir um envio interrompido sem pagar duas vezes, com aviso quando o resultado é incerto (tempo esgotado, `5xx`);
  - `limite_por_operacao` no perfil, que vale mesmo com `--sim`.
- Copia e cola decodificado e conferido localmente (CRC16): o resumo mostra recebedor, cidade, chave ou cobrança, identificador e mensagem; o valor do código prevalece em códigos estáticos; textos do código passam por um filtro de caracteres de controle (#20).
- `inter-pj pix consultar <codigo>`: status, recebedor, erros e histórico de um Pix enviado; `--aguardar [--timeout 60s]` consulta a cada 6 s até um status final (#21).
- Valores em reais aceitos como `150,00`, `1.500,00` ou `150.00`, recusando formas ambíguas (`1.500`).
- Códigos de saída 7 (operação cancelada na confirmação) e 8 (tempo de espera esgotado em `pix consultar --aguardar`).
- Biblioteca: `Banking::enviar_pix` (com `x-id-idempotente`; repetido automaticamente só quando certamente não foi processado) e `Banking::consultar_pix`; `documento::Documento`, `pix::ChavePix`, `pix::BrCode`, `IdIdempotente` e `Error::InvalidInput`.

## [0.2.0] - 2026-09-23

Extrato: consulta, extrato enriquecido, PDF e exportação.

### Adicionado

- `inter-pj extrato [--inicio] [--fim] [--dividir-periodo]`: transações do período (padrão: últimos 30 dias), com entradas, saídas e resultado; o limite de 90 dias da API é conferido antes da chamada e períodos maiores podem ser divididos (#13).
- `inter-pj extrato completo`: detalhes de cada transação (Pix, boletos, pagamentos, transferências, tarifas...), filtros `--tipo-operacao` e `--tipo-transacao`, paginação manual (`--pagina`, `--tamanho-pagina`) ou `--todas-paginas`, com modo *scroll* acima de 10.000 transações e dicas para os erros de scroll (#14).
- `inter-pj extrato pdf [--saida ARQUIVO|-] [--sobrescrever]`: PDF do período gravado com permissão 600, sem sobrescrever arquivos por engano (#15).
- `--formato csv` (RFC 4180) para `saldo` e `extrato`, com `--separador ';'` para o Excel em português e proteção contra fórmulas em textos de terceiros (#16).
- Retentativas automáticas com espera exponencial e `Retry-After` para `429`, `5xx` e falhas de conexão, somente em requisições seguras de repetir; `--tentativas N`, `INTER_TENTATIVAS` e `--sem-retentativa` (#17).
- Biblioteca: `Banking::extrato`, `extrato_completo`, `extrato_completo_todas`, `iniciar_scroll`, `continuar_scroll` e `extrato_pdf`; `Periodo`, `FiltroExtrato`, `TipoTransacao`, `TipoOperacao`, `Detalhe` e `RetryPolicy`.

### Alterado

- Tempo limite das requisições da CLI ampliado para 60 s (páginas de extrato e PDFs grandes).

## [0.1.0] - 2026-09-23

Primeira versão: autenticação e saldo.

### Adicionado

- `inter-pj saldo [--data AAAA-MM-DD]`: saldo disponível, bloqueios e limite, em texto (R$ no formato brasileiro) ou JSON com valores exatos (#8).
- `inter-pj auth token` (escopos, `--renovar`, `--exibir`) e `inter-pj auth limpar [--todos]` (#6, #7).
- `inter-pj config init|caminho|mostrar`: modelo de configuração com permissão 600, locais padrão e configuração efetiva com a origem de cada valor e segredos ocultos (#5).
- Perfis em arquivo TOML com precedência flag > variável de ambiente > arquivo; `client_secret` apenas por `INTER_CLIENT_SECRET` ou arquivo (#5, #4).
- Autenticação OAuth2 *client credentials* com mTLS via rustls, escopos mínimos por comando, renovação automática após `401` e verificação dos escopos concedidos (#6).
- Cache local de tokens por integração (arquivo 600, gravação atômica), respeitando o limite de 5 chamadas/min do endpoint de token (#7).
- Mensagens de erro em português a partir do formato *problem details* da API, com dicas, e códigos de saída documentados (#9).
- Biblioteca `inter-pj` reutilizável, com registro de endpoints verificado contra a especificação OpenAPI (#2, #10).
- CI com formatação, clippy, testes em Linux/macOS/Windows, MSRV 1.88, documentação, cargo-deny e gitleaks; workflow de release com binários e checksums (#3, #11).
