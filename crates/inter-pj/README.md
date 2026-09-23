# inter-pj

Cliente Rust **não oficial** para as APIs do Inter Empresas (conta PJ): OAuth2 *client credentials* com mTLS, cache de token com escopos mínimos e modelos das APIs.

É a biblioteca por trás da CLI [`inter-pj`](https://github.com/edusouza/inter-pj-cli).

```rust,no_run
use inter_pj::{ClientIdentity, Credentials, Environment, InterClient};

# async fn exemplo() -> Result<(), inter_pj::Error> {
let client = InterClient::builder()
    .environment(Environment::Sandbox)
    .credentials(Credentials::new("client-id", "client-secret"))
    .identity(ClientIdentity::from_pem_files("certificado.crt", "chave.key")?)
    .build()?;

let saldo = client.banking().saldo(None).await?;
println!("{:?}", saldo.disponivel);
# Ok(())
# }
```

Sem vínculo com o Banco Inter. Licença MIT OR Apache-2.0.
