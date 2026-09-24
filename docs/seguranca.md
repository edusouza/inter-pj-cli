# Revisão de segurança da 1.0.0

Este é o relatório da revisão de segurança feita antes da 1.0.0 ([#58](https://github.com/edusouza/inter-pj-cli/issues/58)). Ele traz:

- o modelo de ameaças;
- o que já estava protegido;
- cada achado, com a severidade e a correção;
- o que fica como risco aceito.

O texto descreve o código depois das correções, feitas em cinco PRs, de [#125](https://github.com/edusouza/inter-pj-cli/pull/125) a [#129](https://github.com/edusouza/inter-pj-cli/pull/129). A política em vigor está em [`SECURITY.md`](../SECURITY.md).

## Como foi feita

- **Três auditorias independentes do código**, uma por área:
  - segredos, cache de tokens, logs e arquivos;
  - os comandos que movimentam dinheiro ou mudam algo no banco;
  - a saída no terminal, o TLS e a cadeia de suprimentos.

  Cada achado foi conferido no código antes de entrar aqui; os que não se confirmaram ficaram de fora.
- **O [gitleaks](https://github.com/gitleaks/gitleaks) sobre todos os commits de todas as branches.** Foram 100 commits, sem nenhum vazamento, com as regras do projeto para `client_id`, `client_secret` e conta corrente.
- **Uma varredura de todos os arquivos de todos os commits** por CPFs e CNPJs com dígitos verificadores válidos, e-mails e telefones.
- **O cargo-deny** (vulnerabilidades, licenças e fontes) e a contagem das dependências.

## O que se protege

1. **O dinheiro da conta**: Pix, pagamentos de boletos, contas, tributos, DARFs e lotes, e devoluções. Também o que muda o funcionamento da conta: os webhooks, que recebem as notificações dos pagamentos, as cobranças e as recorrências do Pix Automático.
2. **As credenciais**: o `client_secret`, o certificado e a chave privada do mTLS e os tokens de acesso.
3. **Os dados da conta e de terceiros**: extratos, saldos, pagadores e recebedores, CPFs e CNPJs.
4. **O que a pessoa vê antes de confirmar**: o resumo é a última barreira antes de mover dinheiro, e só serve se mostrar a verdade.

## De quem

| Origem | O que pode fazer | Proteção |
| --- | --- | --- |
| Terceiros que escrevem dados que a CLI mostra: quem manda um Pix e escolhe a mensagem, o pagador de uma cobrança, quem emite um boleto ou um copia e cola | sequências de escape, texto invertido, linhas falsas no terminal; fórmulas numa planilha; um código adulterado | filtro dos textos de terceiros em toda a saída; JSON escapado; CSV seguro para planilhas; dígitos verificadores e CRC conferidos localmente |
| A API do Inter, ou quem estivesse no meio caso o TLS falhasse | mensagens de erro e status inesperados; respostas perdidas depois de um envio | TLS com verificação, mTLS, sem redirecionamentos; mensagens da API numa linha; resultado incerto tratado à parte |
| Outros usuários da mesma máquina | ler a configuração, a chave ou os tokens; plantar links simbólicos num diretório compartilhado | permissões `600`/`700`; arquivos criados de forma exclusiva ou trocados por *rename*; avisos de permissão |
| Quem prepara o ambiente de um script ou de um CI | variáveis de ambiente e arquivos de entrada | segredo nunca por flag; `INTER_BASE_URL` só local; confirmação só num terminal |
| Erros de quem usa, e da automação | valor digitado errado, repetições, fuso horário, linhas repetidas num arquivo | validação local, idempotência, código de saída 9, calendário do banco, lotes sem repetidos, limite por operação |
| A cadeia de suprimentos | uma dependência vulnerável, uma ferramenta ou um pacote trocado | cargo-deny a cada push e toda semana; ferramentas por checksum; actions por SHA; pacotes atestados |

Fica fora do escopo quem já executa código como o próprio usuário: ele lê a chave e o segredo como a CLI lê. Também ficam fora o Internet Banking e a API do Inter em si.

## O que já estava certo

**Dinheiro**

- Nenhuma operação que movimenta dinheiro sai sem confirmação explícita, dada depois de um resumo em stderr com o destino, o valor por extenso, a data e o ambiente (produção em destaque). A confirmação é um `s`/`sim` num terminal, ou `--sim`; sem terminal e sem `--sim`, a CLI recusa antes de qualquer requisição.
- `--simular` não monta o cliente nem pede token.
- Nenhum envio é repetido depois de um erro após o qual ele pode ter sido processado (tempo esgotado, `5xx`), só depois de um `429` ou de uma conexão recusada.
  - O Pix repete com a mesma chave de idempotência, mostrada no resumo.
  - As cobranças Pix, as devoluções e as cobranças recorrentes levam o id no caminho, e a API não as duplica.
  - Os pagamentos sem idempotência vêm com o comando que confere o resultado.
- **Valores**: `1.500` é recusado, por ser ambíguo, e mais de duas casas também. Os limites da documentação são conferidos.
- **Documentos e códigos**: os códigos de barras têm todos os dígitos verificadores conferidos, os CPFs e CNPJs também (inclusive o CNPJ alfanumérico), e o copia e cola tem o CRC conferido.
- **Devoluções**: uma devolução maior que o que resta do Pix é recusada.
- **Sandbox**: os comandos exclusivos do sandbox são recusados em produção antes de qualquer requisição, na CLI e na biblioteca.

**Credenciais e rede**

- O `client_secret` nunca é aceito por flag.
- Os tokens só aparecem com `auth token --exibir`.
- A conta corrente vai num cabeçalho sensível, é mascarada em `config mostrar` e nunca aparece numa mensagem de erro.
- TLS com rustls e as raízes do sistema, TLS 1.2 e 1.3, `https` obrigatório e sem redirecionamentos. Não há como desligar a verificação.
- Os logs (`-v`, `-vv`) são restritos aos crates do projeto: nenhum token, segredo, cabeçalho de autorização, corpo, conta, CPF/CNPJ, chave Pix ou valor aparece em nenhum nível.

**Repositório e CI**

- O CI roda com permissões mínimas, as actions são fixadas pelo SHA e o gitleaks varre todo o histórico.
- `unsafe_code = "forbid"`, e as dependências vêm só do crates.io.

## Achados e correções

Severidade: **média** quando o achado leva a dinheiro movido errado, credencial exposta ou tela enganosa num uso plausível; **baixa** quando depende de uma condição incomum; **informativa** quando é uma melhoria sem cenário de dano direto. Nenhum achado foi de severidade alta.

### Médios

| Achado | Correção | PR |
| --- | --- | --- |
| **O CSV não passava pelo filtro, nem no terminal.** A mensagem de um Pix de R$ 0,01 podia escrever na área de transferência (OSC 52, ligado por padrão em vários terminais) ou mover o cursor sobre os valores. | Num terminal, o CSV passa pelo mesmo filtro da saída em texto; num arquivo ou num pipe, fica como veio. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| **`INTER_BASE_URL` aceitava qualquer servidor `https`, em silêncio.** Esse servidor recebia o `client_secret`, os tokens e todas as requisições. O resumo e os comandos exclusivos do sandbox seguem o ambiente do perfil, então uma variável esquecida apontando para produção movia dinheiro real sob o rótulo "sandbox". | A variável existe para os testes: só aceita um servidor desta máquina (`localhost`, `127.0.0.0/8`, `::1`), lido pelo mesmo parser de URL do cliente. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| **Os códigos de saída não distinguiam "pode repetir" de "pode ter pago".** Um tempo esgotado ou um `5xx` depois de um Pix saía com 6, como um `429`. Um script que repetisse os envios com falha pagaria duas vezes, com uma chave de idempotência nova a cada vez. | Um resultado incerto sai com o código 9. O README mostra como repetir com segurança: gerar a chave antes e passar a mesma `--id-idempotente`. | [#127](https://github.com/edusouza/inter-pj-cli/pull/127) |
| **"Hoje" vinha do fuso da máquina.** Num servidor em UTC às 22h30 em Brasília (já o dia seguinte), `--data` com a data de amanhã em Brasília era tomada por hoje: o agendamento era descartado e o pagamento saía um dia antes. | "Hoje" é o dia do calendário do banco, UTC−3, sem horário de verão desde 2019. Ele vale para o agendamento, a conferência de datas passadas e os períodos padrão. O Pix Automático, que tinha um "hoje" próprio, o aviso do cancelamento depois das 22h e os dias das listagens do Pix e da expiração de uma solicitação, que eram os do fuso da máquina, passaram a segui-lo depois da revisão, quando os guias foram escritos. | [#127](https://github.com/edusouza/inter-pj-cli/pull/127), [#133](https://github.com/edusouza/inter-pj-cli/pull/133), [#158](https://github.com/edusouza/inter-pj-cli/pull/158) |
| **Um lote com o mesmo boleto ou DARF duas vezes só gerava um aviso**, e com `--sim` era pago em dobro. | O lote é recusado antes de qualquer requisição, com as linhas repetidas, a menos que se use `--permitir-repetidos`. | [#127](https://github.com/edusouza/inter-pj-cli/pull/127) |
| **Os comandos sugeridos esqueciam a conta.** `Acompanhe com: ...` e `confira antes de tentar de novo: ...` não levavam as opções globais da linha de comando. Depois de um pagamento incerto feito com `-p filial`, a conferência sugerida olharia o perfil padrão, não acharia o pagamento e convidaria a pagar de novo. Achado depois da revisão, quando os guias foram escritos. | Os comandos sugeridos levam as opções que escolheram a conta na linha de comando: `-p`, `--config`, `--ambiente`, as credenciais e `--conta-corrente`. As que vêm de variáveis de ambiente continuam no shell por si. | [#141](https://github.com/edusouza/inter-pj-cli/pull/141) |
| **Os pacotes das releases não eram assinados nem atestados.** O `SHA256SUMS` era gerado no mesmo job que publica, então quem trocasse os pacotes trocaria também as somas. | Cada pacote tem uma atestação de origem (SLSA, assinada pelo Sigstore com a identidade do workflow), conferível com `gh attestation verify`. | [#129](https://github.com/edusouza/inter-pj-cli/pull/129) |

### Baixos

| Achado | Correção | PR |
| --- | --- | --- |
| **O filtro deixava passar os caracteres que invertem ou escondem texto**: marcas e *overrides* bidirecionais, espaços de largura zero, o BOM. Um U+202E numa descrição mostrava o resto da linha invertido, com o valor. | `output::perigoso` passa a cobri-los; os *joiners* das sequências de emoji ficam. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| Os avisos, o progresso (`aguardando: …`) e alguns resumos escreviam status e erros da API em stderr sem filtro. | Toda mensagem em stderr passa por `output::eprint`, ou por `eprint_linha` nas de uma linha. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| Uma quebra de linha numa mensagem da API forjava uma linha `dica:`. | A biblioteca põe os textos da API numa linha, e as linhas seguintes de um erro são recuadas: só as linhas da própria CLI começam na margem. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| O parser repetia um valor recusado com as suas sequências de escape, como um copia e cola colado de uma fatura. | Essa mensagem sai sem cores e filtrada. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| **Fórmulas de planilha**: com `,` como separador, `x;=HYPERLINK(…)` não ia entre aspas, e o Excel em português separa um `.csv` por `;`. | O apóstrofo vai também antes de `=`, `+`, `-` e `@` que venham depois de `,`, `;`, tabulação ou quebra de linha dentro do texto. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| **As gravações seguiam links simbólicos**: `--sobrescrever`, `config init --forcar` e o temporário do cache. Além disso, `config init` conferia `exists()` antes de abrir o arquivo. | Arquivos novos são criados com `O_EXCL`. As substituições são arquivos novos renomeados sobre os antigos, então um link é trocado, nunca seguido. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| O diretório do cache não tinha as permissões conferidas, `INTER_CACHE_DIR` podia ser relativo (e os tokens caíam no diretório atual, até num repositório), e o temporário tinha nome previsível. | O diretório é fechado (`700`) a cada gravação, `INTER_CACHE_DIR` precisa ser absoluto e a gravação é atômica. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| O `client_secret` era um `String` comum no contexto e no perfil (com `Debug` derivado) até ser resolvido, e o `SECURITY.md` exagerava a limpeza da memória. | É um `SecretString` desde a leitura, e o texto do arquivo é zerado. O `SECURITY.md` diz agora exatamente o que é zerado e o que não é. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| O assistente acrescentava o perfil sem corrigir as permissões do arquivo e não avisava sobre a chave. | Ao acrescentar, o assistente regrava o arquivo com `600` e avisa quando a chave pode ser lida por outros usuários. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| A action oficial do cargo-deny baixa o binário sem conferir nada; fixá-la pelo SHA não fixava o binário. | O binário da release é instalado e conferido pelo SHA-256 publicado, como o do gitleaks. | [#129](https://github.com/edusouza/inter-pj-cli/pull/129) |
| Vulnerabilidades novas só apareciam no push seguinte. | `auditoria.yml` confere as dependências toda semana. | [#129](https://github.com/edusouza/inter-pj-cli/pull/129) |
| **Dados com aparência real**: os exemplos da especificação tinham CNPJs válidos e e-mails em provedores reais; os exemplos e os testes usavam e-mails em domínios registráveis (`exemplo.com`), e um CPF válido com formato de celular. O teste de dados pessoais não cobria CNPJs nem o resto do repositório. | A sanitização cobre CNPJs e e-mails, e os exemplos usam `empresa.example`, domínio reservado. Um teste confere CPFs, CNPJs e e-mails em todos os arquivos que o git pode versionar. | [#125](https://github.com/edusouza/inter-pj-cli/pull/125) |
| Pagar um copia e cola dinâmico com outro valor não trazia aviso. | O resumo avisa. | [#127](https://github.com/edusouza/inter-pj-cli/pull/127) |

### Informativos

| Achado | Correção | PR |
| --- | --- | --- |
| O JSON trocava DEL e os controles C1, mudando os dados, e deixava passar os *overrides*. | Esses caracteres saem escapados (`‮`), com os mesmos dados. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| Alguns ids do banco eram formatados fora dos renderizadores. | Passam pelo filtro. | [#126](https://github.com/edusouza/inter-pj-cli/pull/126) |
| A confirmação conferia só a entrada: com stderr num arquivo, a pergunta era respondida às cegas. | A entrada e a saída de erros precisam ser terminais. | [#127](https://github.com/edusouza/inter-pj-cli/pull/127) |
| Um valor que não fosse texto no `client_secret` do arquivo era repetido na mensagem de erro. | Recusado sem ser repetido. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| `auth limpar --todos` deixava os temporários de uma gravação interrompida, que também têm tokens. | Apagados junto. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |
| As páginas de manual eram gravadas seguindo links. | Gravação atômica. | [#128](https://github.com/edusouza/inter-pj-cli/pull/128) |

## Riscos aceitos e limitações

**Dinheiro e automação**

- **O limite por operação vale para cada pagamento**, não para o total de um lote. Ele fica no arquivo de configuração e protege contra erros de digitação, não contra quem pode rodar a CLI: essa pessoa poderia usar outro perfil, outro arquivo, ou a API diretamente.
- **A confirmação exige terminais.** Uma automação que aloca um pseudo-terminal passa pela conferência, como passaria com `--sim`.
- **Um `401` é repetido uma vez**, depois de renovar o token, também num pagamento: um `401` significa que a requisição não foi processada.
- **O destaque `*** PRODUÇÃO`** fica nos resumos que movimentam dinheiro. As alterações, os cancelamentos e os webhooks mostram "PRODUÇÃO (conta real)" na linha do ambiente.
- **Os resumos em stderr trazem os dados do pagamento** (chave, CPF/CNPJ, valor): num CI, eles ficam nos logs.

**Credenciais e arquivos**

- **A memória só é zerada no que a CLI guarda.** Os tipos do `secrecy` zeram o que a CLI mantém; a variável de ambiente do processo, o corpo da requisição de token e os buffers das bibliotecas de HTTP e TLS estão fora do alcance dela.
- **No Windows não há ACLs próprias.** Os arquivos herdam as permissões da pasta (no perfil do usuário, privadas por padrão), e os avisos de permissão só existem no Unix.
- **O cache de tokens não é assinado nem ligado ao segredo:**
  - um token adulterado custa um `401` e uma renovação;
  - um segredo trocado continua "funcionando" até o token em cache vencer;
  - o nome do arquivo, um hash, confirma um `client_id` adivinhado a quem já lê o diretório.

**Rede**

- **`http` só é aceito em endereços locais**, para os testes: outro usuário da máquina poderia ocupar a porta antes.
- **Não há verificação de revogação de certificados no Linux** (o `rustls-platform-verifier` só carrega as raízes), nem *pinning* dos certificados do Inter.
- **Um proxy (`HTTPS_PROXY`) só vê o `CONNECT`.** Interceptar exigiria uma autoridade certificadora confiável no sistema, e mesmo assim o mTLS com o Inter não se completaria.

**Repositório e CI**

- **O toolchain do CI é o `stable`, sem versão fixa.** O `idna_adapter` é fixado de propósito, e o Dependabot o ignora.
- **O histórico do git mantém as versões anteriores da especificação**, com os exemplos públicos do portal que a sanitização agora troca: CNPJs de empresas e do Banco Central, e e-mails fictícios em provedores reais. Não são dados da conta, de clientes nem credenciais, e reescrever o histórico de todas as branches não se justifica para dados publicados pelo próprio Inter. Nenhum dado da conta, credencial ou resposta real da API aparece em nenhum commit.

## Números

- **Dependências**: 113 crates no binário, 111 deles de terceiros; 122 com as de compilação; 273 no `Cargo.lock`, com as dos testes e as de outras plataformas.
- **cargo-deny**: nenhuma vulnerabilidade, as licenças e as fontes em ordem. Três crates aparecem em duas versões (`base64`, `syn` e `winnow`), aceitas como aviso.
- **gitleaks**: 100 commits de todas as branches, nenhum vazamento.
- **Testes**: 998 no total, com testes novos para cada correção.

## Como repetir

```console
$ gitleaks git . --config .gitleaks.toml --redact --log-opts=--all   # todas as branches
$ cargo deny --all-features check
$ cargo test --workspace     # inclui os testes de dados pessoais do repositório e da especificação
```

Os três comandos rodam no CI a cada push, e o cargo-deny também toda semana. Uma nova revisão parte deste relatório: os achados aceitos acima são os primeiros a reavaliar.
