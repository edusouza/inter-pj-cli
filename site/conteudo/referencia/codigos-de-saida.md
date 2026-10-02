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
| 6 | serviço indisponível, limite de requisições (`429`), erro `5xx` ou falha de rede |

Os códigos 4, 5 e 6 vêm da categoria do erro HTTP que a API devolveu; a biblioteca expõe a
mesma categoria em seus erros tipados (veja [erros e códigos de saída](../por-dentro/arquitetura.md#erros-e-codigos-de-saida)).

Um exemplo de uso num script está em [automação e scripts](../guias/automacao.md#reagir-ao-codigo-de-saida),
e o que fazer em cada caso, em [quando algo dá errado](../guias/problemas.md).

O `config verificar` também usa o código 3 quando encontra algum erro no arquivo.
