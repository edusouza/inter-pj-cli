# inter-pj

Cliente Rust **não oficial** para as APIs do Inter Empresas (conta PJ): OAuth2 *client credentials* com mTLS, cache de token com escopos mínimos e modelos das APIs.

É a biblioteca por trás da CLI [`inter-pj`](https://github.com/edusouza/inter-pj-cli).

```rust,no_run
use inter_pj::{ClientIdentity, Credentials, Environment, InterClient};

# async fn exemplo() -> Result<(), Box<dyn std::error::Error>> {
let client = InterClient::builder()
    .environment(Environment::Sandbox)
    .credentials(Credentials::new("client-id", "client-secret"))
    .identity(ClientIdentity::from_pem_files("certificado.crt", "chave.key")?)
    .build()?;

let saldo = client.banking().saldo(None).await?;
println!("{:?}", saldo.disponivel);

// Extrato de agosto, com o sinal de cada operação (saídas negativas).
use chrono::NaiveDate;
use inter_pj::banking::Periodo;
let agosto = Periodo::new(
    NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
    NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
)?;
for transacao in client.banking().extrato(agosto).await? {
    println!("{:?} {:?}", transacao.data(), transacao.valor_com_sinal());
}

// Pix de R$ 150,00 por chave. Repetir com a mesma chave de idempotência não
// paga duas vezes (útil quando a resposta se perde).
use inter_pj::banking::{Destinatario, IdIdempotente, PagamentoPix};
use rust_decimal::Decimal;
let pagamento = PagamentoPix::new(
    Decimal::new(15_000, 2),
    Destinatario::Chave { chave: "fornecedor@exemplo.com".parse()? },
);
let id = IdIdempotente::novo();
let solicitacao = client.banking().enviar_pix(&pagamento, &id).await?;
println!("{:?} {:?}", solicitacao.tipo_retorno, solicitacao.codigo_solicitacao);

// Boleto pelo valor e vencimento do próprio código, cujos dígitos
// verificadores são conferidos antes de qualquer requisição.
use inter_pj::banking::PagamentoBoleto;
use inter_pj::boleto::CodigoBarras;
let codigo: CodigoBarras = "07797777051167847115990071126347192950000003010".parse()?;
let hoje = chrono::Local::now().date_naive();
let (Some(valor), Some(vencimento)) = (codigo.valor(), codigo.vencimento(hoje)) else {
    return Err("o código não traz valor e vencimento".into());
};
let boleto = PagamentoBoleto::new(codigo, valor, vencimento);
let resposta = client.banking().pagar_boleto(&boleto).await?;
println!("{:?} {:?}", resposta.status_pagamento, resposta.codigo_transacao);

// Cobrança (boleto com Pix) para um cliente: o tipo de pessoa vem do CPF/CNPJ,
// e a emissão termina depois; a consulta traz o boleto e o copia e cola.
use inter_pj::cobranca::{EmissaoCobranca, Pagador, Uf};
let pagador = Pagador::new(
    "12.345.678/0001-95".parse()?,
    "Cliente Exemplo Ltda",
    "Avenida Brasil",
    "Belo Horizonte",
    Uf::Mg,
    "30110000",
);
let vencimento = NaiveDate::from_ymd_opt(2026, 10, 20).unwrap();
let cobranca = EmissaoCobranca::new("NF-123", Decimal::new(15_000, 2), vencimento, pagador);
let solicitacao = client.cobranca().emitir(&cobranca).await?;
if let Some(codigo) = solicitacao.codigo_solicitacao {
    let emitida = client.cobranca().consultar(&codigo).await?;
    println!("{:?} {:?}", emitida.cobranca.situacao, emitida.pix.and_then(|p| p.pix_copia_e_cola));
}
# Ok(())
# }
```

Também há DARF sem código de barras (`pagar_darf`, `darfs`) e lotes de 2 a 150 boletos e DARFs (`enviar_lote`, `consultar_lote`).

Na API de Cobrança, além de emitir e consultar: `listar` (uma página) e `listar_todas`, `sumario` por situação, `pdf`, `cancelar` (o motivo é conferido por `motivo_cancelamento`), `editar` (vencimento e valor) com `consultar_edicao`, e `pagar_no_sandbox`, recusado fora do sandbox antes de qualquer requisição.

Consultas são repetidas automaticamente em falhas temporárias (`429`, `5xx`, conexão), conforme a `RetryPolicy` do cliente. Operações com efeitos só são repetidas quando certamente não foram processadas (`429`, conexão recusada): o Pix com a mesma chave de idempotência; boletos, DARFs e lotes, que não têm essa chave, devem ser consultados antes de uma nova tentativa quando o resultado for incerto.

Sem vínculo com o Banco Inter. Licença MIT OR Apache-2.0.
