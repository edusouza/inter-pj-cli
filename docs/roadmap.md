# Roadmap

Planejado a partir das especificações de todas as APIs do [Portal do Desenvolvedor Inter Empresas](https://developers.inter.co/references) (Autenticação, Banking, Cobrança, Pix e Pix Automático — 92 operações; ver [`spec/`](../spec/)).

## Princípios

- **Cada versão é completa**: instalar → configurar → usar os comandos da versão, sem depender das próximas.
- **Leitura antes de escrita**: primeiro consultas; funcionalidades que movimentam dinheiro só chegam com trilhos de segurança (confirmação, simulação, idempotência, limites).
- **Testes são a garantia**: toda funcionalidade vem com testes unitários, de integração contra servidor mock, de contrato contra a especificação OpenAPI e E2E do binário.
- **Nenhum dado pessoal no repositório.**

## Versões

| Versão | Tema | Épico | Operações |
| --- | --- | --- | --- |
| **0.1.0** | Fundação: autenticação mTLS e saldo | [#1](https://github.com/edusouza/inter-pj-cli/issues/1) | `POST /oauth/v2/token`, `GET /banking/v2/saldo` |
| **0.1.1** | Diagnóstico do arquivo de configuração (`config verificar`) | [#164](https://github.com/edusouza/inter-pj-cli/issues/164) | — |
| 0.2.0 | Extrato: consulta, enriquecido, PDF e exportação | [#12](https://github.com/edusouza/inter-pj-cli/issues/12) | `GET /banking/v2/extrato`, `/extrato/completo`, `/extrato/exportar` |
| 0.3.0 | Pix: envio e consulta | [#18](https://github.com/edusouza/inter-pj-cli/issues/18) | `POST /banking/v2/pix`, `GET /banking/v2/pix/{codigoSolicitacao}` |
| 0.4.0 | Pagamentos: boletos, tributos, DARF e lotes | [#22](https://github.com/edusouza/inter-pj-cli/issues/22) | `/banking/v2/pagamento*` (7 operações) |
| 0.5.0 | Cobrança: boleto com Pix | [#27](https://github.com/edusouza/inter-pj-cli/issues/27) | `/cobranca/v3/cobrancas*` (9 operações) |
| 0.6.0 | Pix Cobrança: cob, cobv, recebidos, devoluções, locations e lotes | [#33](https://github.com/edusouza/inter-pj-cli/issues/33) | `/pix/v2/cob*`, `/cobv*`, `/pix*`, `/loc*`, `/lotecobv*` (27 operações) |
| 0.7.0 | Webhooks e callbacks | [#40](https://github.com/edusouza/inter-pj-cli/issues/40) | webhooks de Banking, Cobrança e Pix (15 operações) |
| 0.8.0 | Pix Automático | [#45](https://github.com/edusouza/inter-pj-cli/issues/45) | `/pix/v2/rec*`, `/solicrec*`, `/cobr*`, `/locrec*`, `/webhookrec`, `/webhookcobr` (27 operações) |
| 0.9.0 | Segurança e experiência de uso | [#51](https://github.com/edusouza/inter-pj-cli/issues/51) | keyring do SO, assistente de configuração, validade do certificado, completions |
| 1.0.0 | Estabilização | [#55](https://github.com/edusouza/inter-pj-cli/issues/55) | cobertura total verificada por contrato, documentação, revisão de segurança |

Fora do escopo: a API Fórum (publicações na comunidade de desenvolvedores), que não é funcionalidade da conta PJ — ver [#60](https://github.com/edusouza/inter-pj-cli/issues/60).

## Comandos planejados

```text
inter-pj saldo                                   0.1.0
inter-pj auth token|limpar                       0.1.0
inter-pj config init|caminho|mostrar             0.1.0
inter-pj config verificar [--corrigir]           0.1.1
inter-pj extrato [completo|pdf]                  0.2.0
inter-pj pix enviar|consultar                    0.3.0
inter-pj pagamento boleto|darf|lote ...          0.4.0
inter-pj cobranca emitir|listar|consultar|...    0.5.0
inter-pj pix cob|cobv|recebidos|devolucao|...    0.6.0
inter-pj webhook banking|cobranca|pix ...        0.7.0
inter-pj pix-automatico rec|solicitacao|cobr|... 0.8.0
```
