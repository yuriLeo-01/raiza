# Instalação do Raiza

Este guia descreve onde instalar cada arquivo de sistema do Raiza, como habilitar o acesso aos dispositivos sem root (udev) e como iniciar o daemon `raizad` automaticamente na sessão do usuário (systemd + D-Bus).

## Mapa de arquivos

| Arquivo no repositório | Destino (pacote / sistema) | Destino (instalação manual / usuário) |
|---|---|---|
| Binário `raizad` | `/usr/bin/raizad` | `/usr/bin/raizad` |
| [`packaging/udev/70-raiza.rules`](../packaging/udev/70-raiza.rules) | `/usr/lib/udev/rules.d/70-raiza.rules` | `/etc/udev/rules.d/70-raiza.rules` |
| [`packaging/systemd/raizad.service`](../packaging/systemd/raizad.service) | `/usr/lib/systemd/user/raizad.service` | `~/.config/systemd/user/raizad.service` |
| [`packaging/systemd/io.github.yurileo01.Raiza.service`](../packaging/systemd/io.github.yurileo01.Raiza.service) | `/usr/share/dbus-1/services/io.github.yurileo01.Raiza.service` | `~/.local/share/dbus-1/services/io.github.yurileo01.Raiza.service` |

> **Nota:** tanto a unidade systemd quanto o arquivo de ativação D-Bus apontam para `/usr/bin/raizad`. Se você instalar o binário em outro caminho (como `/usr/local/bin/raizad`), ajuste `ExecStart=` e `Exec=` nesses dois arquivos.

## 1. Compilar e instalar o binário

```sh
cargo build --release --locked -p raizad
sudo install -Dm755 target/release/raizad /usr/bin/raizad
```

## 2. Regra udev (acesso sem root)

A regra [`70-raiza.rules`](../packaging/udev/70-raiza.rules) aplica `TAG+="uaccess"` apenas aos dispositivos HID explicitamente suportados e aos barramentos `/dev/i2c-*` usados por monitores DDC/CI. Isso concede permissão de leitura/escrita apenas ao usuário ativo da sessão local via `systemd-logind`, sem liberar todo `/dev/hidraw*`.

Instale e recarregue as regras:

```sh
sudo install -Dm644 packaging/udev/70-raiza.rules /etc/udev/rules.d/70-raiza.rules
sudo udevadm control --reload && sudo udevadm trigger
```

Se algum dispositivo USB já estiver conectado e a permissão não atualizar imediatamente, desconecte e reconecte o cabo/dongle.

### DDC/CI: módulo `i2c-dev` e alternativa com grupo `i2c`

Para controlar brilho e contraste de monitores via DDC/CI (`/dev/i2c-*`), o módulo de kernel `i2c-dev` precisa estar carregado:

```sh
# Carregar na sessão atual
sudo modprobe i2c-dev

# Carregar automaticamente em todo boot
echo "i2c-dev" | sudo tee /etc/modules-load.d/i2c-dev.conf
```

Por padrão, a regra usa `TAG+="uaccess"` para liberar `/dev/i2c-*` ao usuário da sessão ativa. Caso você esteja em um ambiente sem `systemd-logind` ou acesse a máquina remotamente e prefira controle por grupo:

1. Crie o grupo `i2c` (se ainda não existir) e adicione seu usuário:

   ```sh
   sudo groupadd -f i2c
   sudo usermod -aG i2c "$USER"
   ```

2. Em `/etc/udev/rules.d/70-raiza.rules`, substitua a regra de `i2c-dev` pela alternativa comentada no arquivo:

   ```udev
   SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", GROUP="i2c", MODE="0660"
   ```

3. Recarregue o udev e faça logout/login para aplicar o novo grupo à sua sessão:

   ```sh
   sudo udevadm control --reload && sudo udevadm trigger
   ```

## 3. Unidade systemd do usuário e ativação D-Bus

Instale a unidade de usuário e o arquivo de ativação D-Bus do serviço `io.github.yurileo01.Raiza`:

```sh
install -Dm644 packaging/systemd/raizad.service ~/.config/systemd/user/raizad.service
install -Dm644 packaging/systemd/io.github.yurileo01.Raiza.service ~/.local/share/dbus-1/services/io.github.yurileo01.Raiza.service
systemctl --user daemon-reload
systemctl --user enable --now raizad
```

Com isso, o `raizad` sobe automaticamente com a sessão do usuário e também pode ser ativado sob demanda via D-Bus.

## 4. Verificação

- **Daemon:** `systemctl --user status raizad`
- **Regra udev (sintaxe):** `udevadm verify packaging/udev/70-raiza.rules`
- **Unidade systemd (sintaxe):** `systemd-analyze --user verify packaging/systemd/raizad.service`
- **Permissões efetivas nos dispositivos:** `getfacl /dev/hidraw* /dev/i2c-*` (verifique a presença de `user:<seu-usuario>:rw-` nos dispositivos esperados)
