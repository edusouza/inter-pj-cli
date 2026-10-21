# Códigos de saída

Os códigos de saída são um contrato estável: um script pode decidir pelo número, sem ler a
mensagem. Os erros vão para `stderr`, em português, com a explicação da API e dicas.

| Código | Significado |
| --- | --- |
| 0 | sucesso |
| 1 | erro inesperado |
| 2 | uso incorreto (argumentos inválidos) |
| 3 | configuração ausente ou inválida (inclui certificado e chave) |
| 4 | falha de autenticação ou acesso negado (credenciais, escopos, `401` e `403`) |
| 5 | requisição rejeitada pela API (`400`, `404`, `409`, `422`) |
| 6 | serviço indisponível, limite de requisições (`429`), erro `5xx` ou falha de rede, numa operação que certamente não foi feita (uma consulta, ou um envio que a API não recebeu) |
| 7 | operação cancelada na confirmação (nada foi enviado) |
| 8 | `pix consultar --aguardar`: tempo esgotado antes de um status final |
| 9 | resultado incerto: um envio pode ter sido feito (tempo esgotado, erro `5xx` ou resposta ilegível depois do envio); confira com o comando da dica antes de tentar de novo |

Os códigos 4, 5 e 6 vêm da categoria do erro HTTP que a API devolveu; a biblioteca expõe a
mesma categoria em seus erros tipados (veja [erros e códigos de saída](../por-dentro/arquitetura.md#erros-e-codigos-de-saida)).
O 9 vem antes da categoria: um pagamento ou um Pix que pode ter sido feito sai com 9, qualquer que
seja o erro, porque repeti-lo às cegas pode pagar duas vezes.

Um exemplo de uso num script está em [automação e scripts](../guias/automacao.md#reagir-ao-codigo-de-saida),
e o que fazer em cada caso, em [quando algo dá errado](../guias/problemas.md).

O `config verificar` também usa o código 3 quando encontra algum erro no arquivo.
