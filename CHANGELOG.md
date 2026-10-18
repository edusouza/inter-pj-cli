# Changelog

Todas as mudanças relevantes deste projeto são documentadas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto adota o [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- `inter-pj pagamento boleto pagar <codigo>`: paga ou agenda (`--data`) boletos, contas de consumo e tributos pela linha digitável ou pelo código de barras, conferidos localmente. Valor e vencimento vêm do código quando ele os traz, e o resumo avisa quando `--valor` ou `--vencimento` diferem dele ou quando o pagamento fica para depois do vencimento; `--beneficiario` pede à API que confira o CPF/CNPJ de quem recebe. Mesmos trilhos do Pix: confirmação, `--sim`, `--simular` e `limite_por_operacao`, que passa a valer também para pagamentos. Como a API não tem chave de idempotência, um resultado incerto vem com o comando que confere o pagamento antes de repeti-lo (#23).
- `inter-pj pagamento boleto listar`: pagamentos por código de barras de um período (até 90 dias; padrão: incluídos nos últimos 30 dias), por data de inclusão, pagamento ou vencimento (`--filtrar-por`), código (`--codigo`) ou transação (`--codigo-transacao`), em texto, JSON ou CSV (#24).
- `inter-pj pagamento boleto cancelar <codigo-transacao>`: cancela um agendamento depois de mostrá-lo (beneficiário, valor, data e status) e pedir confirmação; sem terminal, exige `--sim` e não faz nenhuma requisição (#24).
- Biblioteca: `boleto::CodigoBarras` valida e decodifica localmente a linha digitável (47 dígitos para boletos, 48 para contas e tributos) e o código de barras (44 dígitos): dígitos verificadores (módulos 10 e 11), conversão entre linha e código, banco, segmento, valor e vencimento, considerando o reinício do fator de vencimento em 22/02/2025 (#23).
- Biblioteca: `Banking::pagar_boleto` (boletos, contas e tributos com código de barras, com validação local do valor), `Banking::pagamentos` (filtros por período, tipo de data, código e transação) e `Banking::cancelar_pagamento` (agendamentos) (#23, #24).
- Biblioteca: `Banking::pagar_darf` (DARF sem código de barras, com validação local do código da receita, da referência, dos textos e dos valores) e `Banking::darfs` (filtros por período, código da receita e solicitação) (#25).
- Biblioteca: `Banking::enviar_lote` (lotes de 2 a 150 boletos e DARFs, validados item a item) e `Banking::consultar_lote` (status do lote e de cada pagamento) (#26).

### Corrigido

- Automação: um envio cujo resultado ficou incerto (tempo esgotado, erro 5xx, resposta ilegível depois do envio) sai com o novo código 9, e não mais com 6 ou 1, para que um script distinga "pode ter sido feito, confira" de "não foi feito, pode repetir" (um `429` ou uma conexão recusada continuam com 6). Vale para `pagamento boleto pagar` e para `pix enviar`. Quem decide é a biblioteca, pela fase em que a requisição falhou: uma falha ao pedir o token de acesso (tempo esgotado, resposta ilegível) não é mais tomada por um pagamento incerto, porque nada foi enviado; ela tem a sua própria variante, `Error::TokenTransport`, e o novo `Error::may_have_been_processed` diz se a operação pode ter sido processada (#335, #338).
- Os comandos que a saída de `pagamento boleto pagar` sugere (`Acompanhe com: ...`, `Para cancelar: ...`, `confira antes de tentar de novo: ...`) levam as opções globais que escolheram a conta na linha de comando: `-p`, `--config`, `--ambiente`, as credenciais e `--conta-corrente`. Sem elas, conferir um pagamento incerto feito com `-p filial` olharia o perfil padrão, não acharia nada e convidaria a pagar de novo. As que vêm de variáveis de ambiente continuam no shell por si. A dica diz também que a consulta precisa do escopo `pagamento-boleto.read` e que uma lista vazia logo depois do erro não prova que nada foi pago (#336).
- As datas seguem o calendário do banco: "hoje" é o dia em Brasília, qualquer que seja o fuso da máquina, em `pagamento boleto pagar` (agendamento, data passada, aviso de vencimento), nos períodos padrão das listagens e em `pix enviar`. Num servidor em UTC perto da meia-noite, um agendamento para amanhã era tomado por um pagamento de hoje e pago na hora (#337).
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
- Biblioteca: `documento::Documento` valida CPF e CNPJ (inclusive o CNPJ alfanumérico), `pix::ChavePix` reconhece e normaliza chaves Pix (CPF, CNPJ, e-mail, celular `+55` e chave aleatória) e `pix::BrCode` decodifica o Pix copia e cola, conferindo o CRC16 (#20).
- Biblioteca: `Banking::enviar_pix` (por chave, dados bancários ou copia e cola, com `x-id-idempotente`) e `Banking::consultar_pix` (status e histórico), com validação local do pagamento e repetição automática só quando o envio certamente não foi processado (#20, #21).
- Site de documentação no GitHub Pages (`https://edusouza.github.io/inter-pj-cli/`), construído com o Zensical a partir de `site/`, para quem usa a CLI e para quem a desenvolve: primeiros passos, guias, a referência de comandos, a segurança, a arquitetura e como contribuir. A referência de comandos é gerada da ajuda do binário e as demais páginas derivadas (changelog, segurança, arquitetura, roadmap, contribuição e biblioteca) são cópias dos arquivos do repositório, com os links reescritos.
- O crate `sitegen`, que gera essas páginas e confere os comandos `inter-pj ...` citados nas páginas escritas à mão: um comando que não existe mais reprova a construção do site.
- O workflow `pages.yml`: constrói o site em modo estrito nos pull requests e o publica a partir da `main`.

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

## [0.1.1] - 2026-09-25

Diagnóstico do arquivo de configuração.

### Adicionado

- `inter-pj config verificar`: confere o arquivo de configuração e o perfil. Mostra cada problema com a linha e a correção: caminhos do Windows entre aspas duplas, sintaxe do TOML, o que falta no perfil, arquivos que não existem, certificado e chave aceitos pela biblioteca TLS, conta corrente e permissões. Sai com código 3 quando encontra um erro, e `--json` dá o resultado para scripts (#164).
- `inter-pj config verificar --corrigir`: troca as aspas duplas dos caminhos do Windows por aspas simples, mantendo recuo, comentários e fim de linha, e guarda o original em `config.toml.bak` (permissão 600, sem sobrescrever uma cópia anterior) (#164).

### Corrigido

- Um caminho do Windows entre aspas duplas (`certificado = "C:\Users\..."`) deixava de ser um erro em inglês sobre dígitos unicode. Agora a mensagem, em português, explica o escape do TOML, indica as aspas simples e sugere o `config verificar`. O conteúdo da linha continua sem aparecer, porque pode ter o `client_secret` (#164).
- Os comandos avisam quando um caminho lido do arquivo tem um caractere de controle. Isso acontece quando `\n` ou `\t` entre aspas duplas viram uma quebra de linha ou uma tabulação (#164).
- A linha informada num erro do arquivo de configuração saía com um a menos quando o erro apontava o início de uma linha, como numa chave desconhecida (#164).
- O modelo do `config init` e o README usam aspas simples nos caminhos e explicam o motivo (#164).

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
