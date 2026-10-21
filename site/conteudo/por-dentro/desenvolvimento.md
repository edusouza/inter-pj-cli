# Desenvolvimento

O projeto é um workspace Cargo com três crates:

| Crate | Conteúdo |
| --- | --- |
| [`crates/inter-pj`](https://github.com/edusouza/inter-pj-cli/tree/main/crates/inter-pj) | a biblioteca `inter_pj`: cliente HTTP, OAuth2 com mTLS, cache de token e os modelos das APIs |
| [`crates/inter-pj-cli`](https://github.com/edusouza/inter-pj-cli/tree/main/crates/inter-pj-cli) | o binário `inter-pj`: comandos, configuração e formatação |
| [`crates/sitegen`](https://github.com/edusouza/inter-pj-cli/tree/main/crates/sitegen) | o gerador deste site; não entra nos pacotes de release |

A [arquitetura](arquitetura.md) explica a divisão: a biblioteca não conhece terminal nem arquivos
de configuração, e a CLI não conhece HTTP.

## Compilar e rodar

```console
$ cargo build -p inter-pj-cli
$ cargo run -p inter-pj-cli -- saldo --json
```

Requer Rust 1.88 ou superior (a edição é a 2024). Os comandos que o CI roda, e que valem antes
de abrir um pull request, estão em [como contribuir](contribuir.md#verificacoes-locais).

## Os testes rodam offline

```console
$ cargo test --workspace
```

Não precisam de credenciais: um servidor mock simula a API do Inter, os certificados são gerados
a cada execução e os contratos são conferidos contra a especificação OpenAPI versionada em
[`spec/`](https://github.com/edusouza/inter-pj-cli/tree/main/spec). As camadas de teste, e a
receita para acrescentar um endpoint, estão em [como contribuir](contribuir.md#testes).

Para rodar a CLI contra o sandbox de verdade, configure um perfil seu, **fora do repositório**, e
use-a normalmente. Nunca cole saídas reais numa issue ou num pull request sem tirar os dados
identificáveis.

## Este site

O site é construído com o [Zensical](https://zensical.org), a partir de `site/`, e publicado no
GitHub Pages pelo workflow `pages.yml` a cada mudança na `main`. Nos pull requests ele só é
construído, em modo estrito: um link quebrado reprova a mudança, em vez de chegar ao site.

### O que é escrito à mão e o que é gerado

As páginas escritas à mão estão em `site/conteudo/`. As demais são geradas pelo `sitegen`,
antes de cada construção, a partir de arquivos que já existem no repositório, e **não são
commitadas**: uma cópia que não existe não tem como ficar desatualizada.

| Página | De onde vem |
| --- | --- |
| [Comandos](../referencia/comandos.md) | a ajuda do binário `inter-pj` compilado (`--help` de cada comando) |
| [Segurança e privacidade](../referencia/seguranca.md) | `SECURITY.md` |
| [Arquitetura](arquitetura.md) | `docs/arquitetura.md` |
| [Roadmap](roadmap.md) | `docs/roadmap.md` |
| [Como contribuir](contribuir.md) | `CONTRIBUTING.md` |
| [A biblioteca Rust](biblioteca.md) | `crates/inter-pj/README.md` |
| [Changelog](../changelog.md) | `CHANGELOG.md` |

Nas cópias, os links relativos são reescritos: o que é página do site vira link entre páginas, e
o resto aponta para o GitHub.

Por que a referência de comandos vem da ajuda e não de uma tabela escrita à mão: uma opção
nova, ou renomeada, no CLI aparece na página sem que ninguém precise lembrar.

### Os comandos citados nas páginas são conferidos

Depois de gerar as páginas, o `sitegen` lê as páginas escritas à mão e confere cada comando
`inter-pj ...` que aparece num bloco de código (uma linha com `$ ` ou `PS> `, num bloco
`console`, ou uma linha num bloco `bash`) contra a árvore de comandos do binário: o comando e
as opções precisam existir, e as opções que pedem um valor precisam recebê-lo. Se um comando
for renomeado ou removido, a construção do site falha e aponta o arquivo e a linha.

Só os **comandos** são conferidos. As saídas mostradas nas páginas são exemplos, com dados
fictícios, e não são comparadas com a saída real.

### Construir localmente

Da raiz do repositório:

```console
$ cargo build -p inter-pj-cli
$ cargo run -p sitegen
$ python -m pip install -r site/requirements.txt
$ cd site
$ zensical serve
$ zensical build --strict
```

O `cargo run -p sitegen` gera as páginas e confere os comandos; o `zensical serve` abre uma
prévia em `http://localhost:8000`; e o `zensical build --strict` é o que a CI roda. O Python
só é preciso para o site, na CI e na máquina de quem mexe nele: nada dele entra no binário.

### Ao mudar o projeto

- **Uma página nova** vai em `site/conteudo/` e na lista `nav` de `site/mkdocs.yml`.
- **Um comando ou uma opção nova**: a referência de comandos já a mostra. Se o comando já era
  citado numa página, o `sitegen` avisa; atualize o texto.
- **Uma versão nova**: o `CHANGELOG.md` e o `docs/roadmap.md` já viram páginas. O que fala do
  que a CLI faz (a [página inicial](../index.md), os guias) é escrito à mão e precisa ser
  atualizado junto.

### Por que o Zensical, e o que isso exige do `mkdocs.yml`

O gerador é o Zensical, que lê um `mkdocs.yml`. O Material for MkDocs, no qual ele se baseia,
está entrando em modo de manutenção, e o Zensical é o sucessor, feito pela mesma equipe, que lê o mesmo
arquivo de configuração. Mas ele está na versão 0.0.x, e por isso valem duas regras:

- o `mkdocs.yml` **só usa o que o Material também entende**. Se o Zensical quebrar, voltar ao
  Material é trocar uma linha do `site/requirements.txt`;
- a versão do Zensical fica **fixada** em `site/requirements.txt`, e atualizá-la é uma decisão,
  não um efeito colateral.

### Publicação

O workflow `.github/workflows/pages.yml` constrói o site e o publica com `actions/deploy-pages`.
Para isso, o repositório precisa de **Settings → Pages → Build and deployment → Source: GitHub
Actions**.

O endereço é `edusouza.github.io/inter-pj-cli/`, o de um site de projeto. Isso só vale enquanto
o repositório `edusouza.github.io` não tiver um domínio personalizado: se tiver, o GitHub
redireciona todos os sites de projeto para esse domínio.
