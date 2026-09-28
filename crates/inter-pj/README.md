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
# Ok(())
# }
```

Consultas são repetidas automaticamente em falhas temporárias (`429`, `5xx`, conexão), conforme a `RetryPolicy` do cliente; operações com efeitos nunca são repetidas.

Sem vínculo com o Banco Inter. Licença MIT OR Apache-2.0.
