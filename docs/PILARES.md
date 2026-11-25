# Pilares do Raiza

## O problema

Quem usa Linux sofre para gerenciar periféricos. Os fabricantes de mouse, teclado,
headset, controle e RGB lançam software só para Windows e macOS. No Linux sobra
juntar meia dúzia de ferramentas diferentes, uma por marca, quando ela existe. Muitas
vezes não existe nenhuma, e funções como DPI, RGB, bateria ou equalização ficam inacessíveis.

## A proposta

O **Raiza** é um hub único onde o usuário de Linux gerencia todos os seus periféricos,
de qualquer marca, mesmo sem suporte oficial.

## Os pilares

Toda decisão de produto e de código deve respeitar estes pilares. Quando dois deles
entrarem em conflito, vale a ordem da lista.

### 1. Nunca estragar um dispositivo
- Nada de escrever em firmware. O Raiza só envia comandos de configuração já documentados.
- Todo comando enviado a um dispositivo é validado contra o perfil dele antes de sair.
- Um perfil novo ou não verificado roda em **modo somente leitura** até ser marcado como seguro.
- Falha em um dispositivo nunca derruba o daemon nem afeta os outros dispositivos.

### 2. Linux em primeiro lugar, leve e sem depender do fabricante
- O daemon (`raizad`) é escrito em Rust e roda em user space, na sessão do usuário. Não usa driver de kernel.
- Não precisa de root no dia a dia: uma regra udev dá acesso aos dispositivos uma única vez.
- O daemon fica leve: pouca RAM, sem polling agressivo e orientado a eventos (udev, D-Bus).

### 3. Um hub para tudo
- Uma interface só para mouse, teclado, headset, controle, microfone, monitor e RGB.
- Cada dispositivo expõe **capacidades padronizadas** (bateria, DPI, RGB, volume,
  mudo, brilho, vibração, conexão…). A interface desenha as telas a partir das
  capacidades, não da marca.
- As interfaces são **nativas**: primeiro GTK4/libadwaita (GNOME), depois Kirigami (KDE).
  Todas são clientes do mesmo daemon via D-Bus.

### 4. Extensível por plugins e perfis
- Suporte a um dispositivo novo não exige mexer no núcleo.
- Sempre que possível, o suporte é um **perfil declarativo** (JSON com os comandos HID),
  para que usuários contribuam sem saber Rust.
- Integrações mais complexas viram **provedores** em Rust que implementam um contrato do `raiza-core`.

### 5. Aproveitar o que já existe
- Integrar com BlueZ, UPower, PipeWire, DDC/CI, evdev e OpenRGB em vez de reinventar.
- Engenharia reversa só para dispositivos que nenhum projeto suporta.

### 6. Aberto e documentado
- Código aberto (GPL-3.0-or-later).
- Cada protocolo descoberto é documentado em `docs/protocolos/`, para servir à
  comunidade Linux mesmo fora do Raiza.

### 7. Feito por agentes, com um líder
- O Claude é o **líder técnico**: define contratos, divide o trabalho em tarefas
  isoladas, despacha subagentes (Claude ou Gemini 4 Hydrogen pelo Antigravity) em
  worktrees do Orca, revisa e integra.
- Cada tarefa é pequena, tem escopo de arquivos definido e critérios de aceite
  verificáveis (`cargo test`, `cargo clippy`).
- Veja `docs/ORQUESTRACAO.md`.
