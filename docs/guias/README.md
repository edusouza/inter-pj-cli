# Guias

Como usar a `inter-pj` no dia a dia, um assunto por guia, com exemplos de terminal.

| Guia | O que tem |
| --- | --- |
| [Saldo e extrato](saldo-e-extrato.md) | o saldo, o extrato de um período, o extrato completo com os detalhes de cada transação, planilhas e PDF |

A instalação e a configuração estão no [README](../../README.md#instalação).

## Como ler os exemplos

- Uma linha que começa com `$ ` é um comando. As linhas seguintes, até o próximo comando, são o que o terminal mostra: a saída do comando e as mensagens de erro e avisos, na ordem em que aparecem.
- Um comando que pede confirmação aparece com a resposta: `Enviar o Pix? [s/N] s`.
- Os exemplos rodam no perfil `padrao`, de produção, de uma empresa fictícia, a Empresa Exemplo Ltda. Nomes, documentos, chaves e valores são todos fictícios.
- Nos exemplos, hoje é 24/09/2026, e a pasta pessoal aparece como `/home/voce`.

## Como os exemplos são conferidos

Os testes do projeto executam cada exemplo destes guias contra uma simulação da API do Inter, que tem os dados da empresa fictícia, e conferem que o comando imprime exatamente o que o guia mostra. Um guia desatualizado quebra o CI.

Os testes fixam o dia de hoje, para que as datas dos exemplos não envelheçam. Há duas exceções, e o texto do guia avisa quando um exemplo é uma delas:

- os exemplos cuja saída depende do relógio, como os dias que faltam para o certificado vencer, são executados, mas a saída mostrada é só uma ilustração;
- os que usam outro programa, como o `jq`, não são executados.

## Para quem escreve um guia

Cada bloco `console` de `docs/guias`, `docs/receitas.md` e `docs/faq.md` é uma sessão de terminal, e os testes a executam na ordem do arquivo: um arquivo gravado por um comando fica disponível para os seguintes. Os dados da simulação, e as regras de cada exemplo, estão em [`crates/inter-pj-cli/tests/guias`](../../crates/inter-pj-cli/tests/guias).

- Escreva o comando e deixe a saída em branco: `ATUALIZAR_GUIAS=1 cargo test -p inter-pj-cli --test guias` grava no guia o que cada comando imprime. Revise o diff antes do commit.
- Um comentário `<!-- guia: saída ilustrativa -->` antes do bloco faz o teste só conferir que o comando funciona (ou falha, se o guia mostra um `erro:`); `<!-- guia: não executar -->`, que o bloco não rode.
- Os comandos aceitos são `inter-pj` e `cat`, com `>` e `<`; um exemplo com `|` ou com variáveis do shell precisa de `<!-- guia: não executar -->`.
