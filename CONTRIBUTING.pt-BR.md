# Como contribuir com o Raiza

[English](CONTRIBUTING.md) · **Português**

Obrigado pelo interesse! Há três jeitos principais de ajudar, do mais simples ao mais técnico.

## 1. Pedir suporte a um aparelho

Abra uma issue com o modelo **"Suporte a aparelho"**. As informações que mais ajudam:

- o modelo exato (como aparece na caixa ou no site do fabricante);
- a saída de `lsusb` com o aparelho plugado (a linha dele basta);
- como ele se conecta: cabo, dongle 2.4 GHz, Bluetooth (alguns usam os três);
- o que você gostaria de controlar (bateria, DPI, RGB…);
- se existe software oficial (Windows, web) e se você sabe de algum projeto aberto que já
  entenda o protocolo (OpenRGB, configuradores no GitHub etc.).

## 2. Contribuir um perfil

Um perfil é um JSON em `profiles/<marca>/` que descreve como ler e escrever as capacidades de um
modelo pelo `/dev/hidraw`. Não precisa escrever código Rust. O formato completo está em
[`docs/PERFIS.md`](docs/PERFIS.md), e o editor valida o arquivo pelo [`profiles/schema.json`](profiles/schema.json).

### De onde vêm os bytes

1. **Documentação aberta:** muitos protocolos já foram decifrados por outros projetos. Os perfis
   atuais vieram de projetos assim (veja [`docs/protocolos/`](docs/protocolos/)). Cite sempre a
   fonte e confira se a licença é compatível com a GPL-3.0.
2. **Relatórios que o aparelho manda sozinho:** muitos mouses mandam um status periódico com
   bateria e DPI. Dá para descobri-lo só escutando, sem enviar nada.
3. **Capturas do software oficial:** usando `usbmon`/Wireshark com o software numa máquina virtual
   Windows, com o USB repassado. É o caminho mais trabalhoso.

### O caminho de um perfil até gravar

1. Escreva o perfil com **`"verified": false`**. Assim ele é **somente leitura**: o Raiza só envia
   consultas fixas descritas no perfil, e nunca escritas.
2. Instale o perfil em `~/.local/share/raiza/profiles/` (o daemon recarrega sozinho) e confira os
   valores com `raiza-ctl list`, **comparando com o software oficial**.
3. Para liberar uma escrita, teste o comando no aparelho **uma vez, à mão**: grave um valor,
   releia, confira no software oficial e volte ao valor original. Anote os bytes e o resultado em
   `docs/protocolos/<aparelho>.md`.
4. Só então o perfil passa a `"verified": true`. Na revisão, os mantenedores conferem o registro do
   teste.

### Regras de segurança (pilar 1)

- **Nunca** inclua comandos de atualização de firmware, bootloader ou apagamento de memória. A
  validação bloqueia os padrões conhecidos, mas a responsabilidade é de quem escreve o perfil.
- Grave o **mínimo** de bytes necessário. Se o software oficial grava um bloco inteiro, descubra o
  que cada byte faz antes de copiar. No Rapoo, o bloco do polling também guardava a taxa de
  varredura do sensor.
- Quando o mesmo VID:PID é usado por aparelhos diferentes (comum em teclados), o perfil **precisa**
  de `identify`.
- Na dúvida, deixe somente leitura.

## 3. Contribuir código

O Raiza é escrito em Rust (edição 2024). Leia antes:

- [`docs/PILARES.md`](docs/PILARES.md): as decisões que guiam o projeto;
- [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md): crates, provedores e fusão;
- [`AGENTS.md`](AGENTS.md), seção "Regras de código": sem `unwrap()` em caminho de hardware, I/O de
  hardware atrás de uma trait injetável (para testar sem o aparelho), logs com `tracing`,
  comentários e documentação em português e nomes de código em inglês.

Para testar sem hardware, use o daemon falso:

```sh
systemctl --user stop raizad      # se o Raiza estiver instalado
cargo run -p raizad -- --mock
cargo run -p raiza-gtk            # ou: cargo run -p raiza-ctl -- list
```

Antes de abrir o pull request, tudo isto precisa passar:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

Os contratos públicos (`crates/raiza-core` e [`docs/DBUS.md`](docs/DBUS.md)) mudam com cuidado:
abra uma issue para discutir antes.

## Relatar um bug

Use o modelo **"Bug"** e inclua:

- `raiza-ctl list --json` (mostra o que o daemon vê);
- `journalctl --user -u raizad --since "-10min"` (os logs do daemon);
- a versão: `pacman -Q raiza-git` ou o commit, se compilou à mão;
- o que você fez, o que esperava e o que aconteceu.

O `raiza-ctl list --json` traz os números de série dos aparelhos. Se preferir, apague-os antes de
colar.
