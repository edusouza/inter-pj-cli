# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- Biblioteca: `boleto::CodigoBarras` valida e decodifica localmente a linha digitável (47 dígitos para boletos, 48 para contas e tributos) e o código de barras (44 dígitos): dígitos verificadores (módulos 10 e 11), conversão entre linha e código, banco, segmento, valor e vencimento, considerando o reinício do fator de vencimento em 22/02/2025 (#23).
- Biblioteca: `Banking::pagar_boleto` (boletos, contas e tributos com código de barras, com validação local do valor), `Banking::pagamentos` (filtros por período, tipo de data, código e transação) e `Banking::cancelar_pagamento` (agendamentos) (#23, #24).
- Biblioteca: `Banking::pagar_darf` (DARF sem código de barras, com validação local do código da receita, da referência, dos textos e dos valores) e `Banking::darfs` (filtros por período, código da receita e solicitação) (#25).
- Biblioteca: `Banking::enviar_lote` (lotes de 2 a 150 boletos e DARFs, validados item a item) e `Banking::consultar_lote` (status do lote e de cada pagamento) (#26).

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
