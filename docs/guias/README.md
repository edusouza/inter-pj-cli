# Guias

Como usar a `inter-pj` no dia a dia, um assunto por guia, com exemplos de terminal.

| Guia | O que tem |
| --- | --- |
| [Saldo e extrato](saldo-e-extrato.md) | o saldo, o extrato de um período, o extrato completo com os detalhes de cada transação, planilhas e PDF |
| [Pagamentos](pagamentos.md) | pagar boletos, contas e tributos pelo código, agendar, conferir o beneficiário, listar os pagamentos e cancelar um agendamento; pagar DARFs, pelas opções ou por um arquivo; e pagar em lote, a partir de uma planilha ou de um JSON |
| [Cobranças](cobrancas.md) | emitir cobranças (boletos com Pix) pelas opções ou por um arquivo, esperar a emissão, o boleto, o Pix e o QR Code, o PDF, o prazo depois do vencimento, as cobranças de um período e o resumo por situação, alterar o valor ou o vencimento, cancelar, conferir uma emissão de resultado incerto e testar no sandbox |
| [Cobranças Pix](cobrancas-pix.md) | criar uma cobrança imediata (`pix cob`) com o txid que torna segura a repetição, alterar e remover, uma cobrança paga com os seus Pix, conferir uma criação de resultado incerto e as cobranças de um período; criar uma cobrança com vencimento (`pix cobv`) pelas opções ou por um arquivo, com a validade, a multa, os juros e o desconto, e alterá-la; as locations, para um QR Code impresso que serve a uma cobrança depois da outra; os lotes de cobranças com vencimento, a partir de uma planilha, com a conferência antes do envio, o processamento e as alterações; e o pagamento de teste no sandbox |
| [Pix](pix.md) | enviar um Pix por chave, copia e cola ou dados bancários, agendar, os trilhos de segurança (resumo, confirmação, limite, idempotência), acompanhar o Pix enviado, os Pix recebidos e as devoluções |
| [Pix Automático](pix-automatico.md) | criar uma recorrência, a autorização das cobranças de um contrato, pelas opções ou por um arquivo, conferir uma criação de resultado incerto, as recorrências de um período, uma recorrência aprovada, alterar o primeiro pagamento e cancelar; pedir a aprovação ao banco do pagador, com uma solicitação de confirmação, acompanhá-la e cancelá-la |
| [Webhooks](webhooks.md) | cadastrar, consultar e excluir os webhooks do Banking, das cobranças, das chaves Pix e do Pix Automático, com o antes e o depois de uma troca de URL; o histórico das tentativas de entrega das notificações e o reenvio das que não chegaram |

A instalação e a configuração estão no [README](../../README.md#instalação).

## Como ler os exemplos

- Uma linha que começa com `$ ` é um comando. As linhas seguintes, até o próximo comando, são o que o terminal mostra: a saída do comando e as mensagens de erro e avisos, na ordem em que aparecem.
- Um comando que pede confirmação aparece com a resposta: `Enviar o Pix? [s/N] s`.
- Os arquivos que um comando lê, como uma planilha de pagamentos, aparecem antes dele, com o nome do arquivo no texto.
- Os exemplos rodam no perfil `padrao`, de produção, de uma empresa fictícia, a Empresa Exemplo Ltda. Nomes, documentos, chaves e valores são todos fictícios.
- Nos exemplos, hoje é 24/09/2026, e a pasta pessoal aparece como `/home/voce`.

## Como os exemplos são conferidos

Os testes do projeto executam cada exemplo destes guias contra uma simulação da API do Inter, que tem os dados da empresa fictícia, e conferem que o comando imprime exatamente o que o guia mostra. Um guia desatualizado quebra o CI.

Os testes fixam o dia de hoje, para que as datas dos exemplos não envelheçam. Há duas exceções, e o texto do guia avisa quando um exemplo é uma delas:

- os exemplos cuja saída depende do relógio, como os dias que faltam para o certificado vencer, são executados, mas a saída mostrada é só uma ilustração;
- os que usam outro programa, como o `jq`, não são executados.

## Para quem escreve um guia

Cada bloco `console` de `docs/guias`, `docs/receitas.md` e `docs/faq.md` é uma sessão de terminal, e os testes a executam na ordem do arquivo: um arquivo gravado por um comando fica disponível para os seguintes. Os dados da simulação, e as regras de cada exemplo, estão em [`crates/inter-pj-cli/tests/guias`](../../crates/inter-pj-cli/tests/guias).

- Escreva o comando e deixe a saída em branco: `ATUALIZAR_GUIAS=1 cargo test -p inter-pj-cli --test guias` grava no guia o que cada comando imprime. Num comando com confirmação, escreva só a linha da resposta (`Enviar o Pix? [s/N] s`), que fica depois do resumo. Revise o diff antes do commit.
- Um comentário `<!-- guia: saída ilustrativa -->` antes do bloco faz o teste só conferir que o comando funciona (ou falha, se o guia mostra um `erro:`); `<!-- guia: não executar -->`, que o bloco não rode.
- Um comentário `<!-- guia: arquivo lote.csv -->` antes de um bloco de qualquer linguagem grava o conteúdo do bloco em `lote.csv`, na pasta pessoal da sessão, para os comandos seguintes. O comentário não aparece no guia: diga o nome do arquivo no texto.
- Os comandos aceitos são `inter-pj` e `cat`, com `>` e `<`; um exemplo com `|` ou com variáveis do shell precisa de `<!-- guia: não executar -->`.
