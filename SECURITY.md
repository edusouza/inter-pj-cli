# Segurança

Esta CLI acessa uma conta bancária empresarial. Segurança e privacidade são requisitos, não detalhes.

## Como as credenciais são tratadas

| Dado | Tratamento |
| --- | --- |
| `client_secret` | lido de `INTER_CLIENT_SECRET` ou do arquivo de configuração — **nunca** de flags (histórico do shell, `ps`); mantido em tipo que não aparece em `Debug`/logs |
| Certificado e chave (mTLS) | lidos do disco apenas para montar a identidade TLS; chaves protegidas por senha são recusadas com instrução clara; as cópias mantidas pela CLI são zeradas da memória ao descartar |
| Token de acesso | pedido com os escopos mínimos de cada comando; cache em arquivo com permissão `600` (diretório `700`) e gravação atômica; só é exibido com `auth token --exibir` |
| Conta corrente | enviada no cabeçalho `x-conta-corrente` (marcado como sensível); mascarada em `config mostrar`; nunca ecoada em mensagens de erro |
| Arquivo de configuração | criado com permissão `600`; a CLI avisa se ele (ou a chave privada) puder ser lido por outros usuários |

Outras proteções:

- TLS com rustls, verificação de certificados pelo repositório do sistema, `https` obrigatório (exceto `localhost`, para testes) e sem seguir redirecionamentos.
- Logs (`-v`/`-vv`) restritos aos crates do projeto: bibliotecas de HTTP/TLS não registram nada, então cabeçalhos e corpos não vazam.
- Erros de parse do arquivo de configuração indicam apenas a linha, sem reproduzir o conteúdo (que pode conter o segredo).
- Operações que movimentam dinheiro (`pix enviar`, `pagamento boleto pagar`, `pagamento darf pagar` e `pagamento lote enviar`) validam tudo localmente e mostram um resumo antes de enviar. Só enviam com confirmação explícita: `s`/`sim` digitado em um terminal (respostas vindas de um *pipe* não valem) ou `--sim`. O cancelamento de um agendamento também pede confirmação.
- Essas operações têm `--simular` e um `limite_por_operacao` por perfil, que vale mesmo com `--sim` e, num lote, para cada pagamento. O Pix tem idempotência (`x-id-idempotente`, com repetição segura por `--id-idempotente`); os pagamentos não a têm na API, então um resultado incerto vem com o comando que confere se o pagamento foi feito antes de uma nova tentativa.
- Arquivos de pagamento (DARF, lotes) são conferidos por inteiro antes de qualquer envio: campos desconhecidos são recusados, e um único problema impede o envio do lote todo.
- Um envio nunca é repetido automaticamente depois de um erro após o qual ele pode ter sido processado (tempo esgotado, `5xx`).
- Na saída em texto e nas mensagens, textos de terceiros (nomes, descrições e mensagens de erro da API; o nome e a cidade de um copia e cola) passam por um filtro de caracteres de controle, para que sequências de escape e quebras de linha não alterem o que o terminal mostra. JSON e CSV mantêm o texto original.

## Política do repositório: nenhum dado pessoal

Credenciais, certificados, tokens, números de conta, CPF/CNPJ ou respostas reais da API **não podem** entrar no repositório — nem como exemplo, nem em testes. O `.gitignore` bloqueia os formatos de credencial e o job `segredos` do CI roda o gitleaks, com regras específicas para `client_id`/`client_secret`/conta corrente do Inter (`.gitleaks.toml`), sobre **todo o histórico**.

### Se algo vazar

Trate como incidente grave:

1. **Revogue imediatamente** a credencial no Internet Banking PJ (exclua ou renove a integração, gerando novos certificado e segredo). Considere o dado comprometido mesmo que o commit seja removido depois.
2. **Remova do histórico** com [`git filter-repo`](https://github.com/newren/git-filter-repo) (ex.: `git filter-repo --invert-paths --path caminho/do/arquivo` ou `--replace-text`), confirme com `gitleaks git .` e faça `push --force` de todas as branches e tags afetadas.
3. Peça ao suporte do GitHub a remoção de caches/visualizações de commits antigos, se necessário, e avise quem tiver clonado o repositório.

## Reportando vulnerabilidades

Não abra issue pública para vulnerabilidades. Use o [reporte privado de vulnerabilidades do GitHub](https://github.com/edusouza/inter-pj-cli/security/advisories/new) com a descrição, o impacto e os passos para reproduzir — sem incluir credenciais reais.
