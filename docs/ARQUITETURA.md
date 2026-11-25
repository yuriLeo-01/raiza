# Arquitetura

```
                          ┌─► raiza-gtk   (GTK4/libadwaita, GNOME)
                          ├─► raiza-ctl   (linha de comando)
raizad ◄──── D-Bus ───────┤
  │   (sessão do usuário) └─► raiza-kde   (Kirigami, futuro)
  │
  ├── registro de provedores
  │     ├── udev        → descoberta e hotplug de dispositivos
  │     ├── bluez       → conexão Bluetooth, bateria de headsets/controles
  │     ├── upower      → bateria de mouses/teclados sem fio
  │     ├── pipewire    → volume e mudo de microfones/headsets
  │     ├── ddc         → brilho/contraste de monitores (DDC/CI)
  │     ├── evdev       → controles: bateria, teste de vibração
  │     ├── openrgb     → RGB via servidor SDK do OpenRGB, se instalado
  │     └── hid-perfis  → motor de perfis JSON via /dev/hidraw*
  │
  └── armazenamento de configurações (~/.config/raiza/) e reaplicação ao plugar
```

## Crates

| Crate | Papel |
|---|---|
| `raiza-core` | Tipos e contratos: `DeviceInfo`, `Capability`, trait `Provider`, eventos, erros. Sem I/O. |
| `raizad` | Daemon: registra provedores, expõe a API D-Bus e persiste configurações. |
| `raiza-ctl` | CLI cliente da API D-Bus (`raiza-ctl list`, `get`, `set`). |
| `raiza-gtk` | Interface GTK4 + libadwaita (relm4). |
| `raiza-provider-*` | Um crate por provedor (criados nas tarefas). |
| `raiza-profiles` | Formato, validação e execução de perfis HID declarativos. |

## Modelo de dados (resumo, detalhado em T01)

- **Dispositivo**: id estável (barramento + vendor:product + serial/caminho), nome, tipo
  (mouse, teclado, headset, controle, microfone, monitor, rgb, outro), provedor de origem e capacidades.
- **Capacidade**: algo que o dispositivo sabe fazer, com tipo, valor atual e limites.
  Exemplos: `Battery { percent, charging }`, `Dpi { stages, current, min, max, step }`,
  `Rgb { zones, modes }`, `Volume`, `Mute`, `Brightness`, `Rumble`, `Connection`.
- **Provedor**: trait assíncrona que descobre dispositivos, lê e escreve capacidades e emite eventos.
- Um mesmo dispositivo físico pode ser atendido por mais de um provedor (ex.: headset
  Bluetooth = bluez + pipewire). O daemon **funde** as capacidades pelo id do dispositivo.

## API D-Bus (resumo, especificada em T02)

- Serviço `io.github.yurileo01.Raiza` no barramento de sessão, objeto `/io/github/yurileo01/Raiza`.
- Métodos: `ListDevices`, `GetDevice`, `SetCapability`.
- Sinais: `DeviceAdded`, `DeviceRemoved`, `CapabilityChanged`.

## Stack

- Rust (edição 2024), `tokio`, `zbus`, `udev`, `hidapi`, `evdev`, `serde`/`serde_json`, `thiserror`, `tracing`.
- Interface: `gtk4`, `libadwaita`, `relm4`.
- Testes: provedores têm uma camada de I/O injetável para testar sem hardware real.

## Segurança

- O daemon só aceita chamadas D-Bus da sessão do próprio usuário.
- O acesso a `/dev/hidraw*` vem de uma regra udev com `TAG+="uaccess"`, restrita aos
  vendor/product IDs dos perfis conhecidos.
- Perfis HID novos começam como `"verificado": false` e ficam em somente leitura (pilar 1).
