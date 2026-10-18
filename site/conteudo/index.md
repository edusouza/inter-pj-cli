# inter-pj

CLI em Rust para acessar a sua **conta PJ do Inter Empresas** pela linha de comando, usando as
[APIs oficiais do Inter](https://developers.inter.co/references) (OAuth2 com mTLS). Serve a
quem quer consultar a conta no terminal ou num script, e a quem quer usar a biblioteca Rust por
trás dele ou contribuir com o projeto.

!!! warning "Projeto não oficial"
    Não tem vínculo com o Banco Inter. Use por sua conta e risco e comece pelo ambiente
    **sandbox**, que tem dados fictícios.

```console
$ inter-pj saldo
Saldo disponível          R$ 2.850,55
Bloqueado em cheque         R$ 240,25
Bloqueado judicialmente     R$ 510,35
Bloqueado administrativo      R$ 0,00
Limite                    R$ 1.000,00

$ inter-pj saldo --json | jq .disponivel
2850.55

$ inter-pj extrato --inicio 2026-08-01 --fim 2026-08-31
Extrato de 01/08/2026 a 31/08/2026

Data        Tipo       Descrição                                   Valor
03/08/2026  Pix        Pix recebido · Cliente Exemplo Ltda   R$ 1.500,00
05/08/2026  Pagamento  Pagamento efetuado · Boleto; energia   -R$ 250,10

Entradas              R$ 1.500,00
Saídas                 -R$ 250,10
Resultado do período  R$ 1.249,90
2 transações
```

Os valores e os nomes são fictícios, e as saídas mostram o formato, não o seu extrato.

## O que funciona hoje

- **Saldo**: o atual, com os bloqueios e o limite, ou o de um dia.
- **Extrato**: o de um período, o completo, com os detalhes de cada transação e todas as
  páginas, e o PDF.
- **Saída para pessoas e para programas**: texto alinhado, JSON com os nomes de campo da API
  e CSV, inclusive no formato do Excel em português.
- **Credenciais sob controle**: tokens pedidos só com os escopos de que o comando precisa,
  guardados num cache local, e o `client_secret` nunca numa opção de linha de comando.
- **Configuração com perfis** (sandbox e produção lado a lado) e um diagnóstico do arquivo,
  o `config verificar`.

Pix, pagamentos, cobranças, webhooks e Pix Automático vêm nas próximas versões: o
[roadmap](por-dentro/roadmap.md) mostra a ordem, e o [changelog](changelog.md), o que cada
versão trouxe.

## Por onde começar

### Quero usar o CLI

1. [Instale o `inter-pj`](comecar/instalacao.md).
2. [Crie a integração no Internet Banking PJ](comecar/integracao.md), que dá o certificado, a
   chave e as credenciais.
3. [Configure](comecar/configurar.md) e [rode os primeiros comandos](comecar/primeiros-comandos.md).
4. Depois, os [guias](guias/saldo-e-extrato.md): o extrato em CSV para a contabilidade, o uso
   em scripts e em CI, e o que fazer [quando algo dá errado](guias/problemas.md).

Procurando uma opção? A [referência de comandos](referencia/comandos.md) é gerada da ajuda
do próprio programa.

### Quero usar a biblioteca ou contribuir

- [A biblioteca Rust](por-dentro/biblioteca.md): o cliente HTTP, o OAuth2 com mTLS e os modelos
  das APIs, sem terminal nem arquivos de configuração.
- [Arquitetura](por-dentro/arquitetura.md): como o projeto está dividido e por que cada decisão.
- [Desenvolvimento](por-dentro/desenvolvimento.md) e [Como contribuir](por-dentro/contribuir.md):
  compilar, testar, e o fluxo de trabalho do projeto.

## Segurança

A CLI acessa uma conta bancária empresarial, e por isso as credenciais têm tratamento próprio:
o [que é guardado, onde e com que permissão](referencia/seguranca.md), e o que fazer se algo
vazar. Nenhum dado pessoal ou credencial faz parte do repositório.

O código está no [GitHub](https://github.com/edusouza/inter-pj-cli), sob as licenças MIT ou
Apache-2.0, à sua escolha.
