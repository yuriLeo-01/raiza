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

## Modelo de dados (definido em `crates/raiza-core`, T01)

- **`DeviceId`**: id estável, serializado como string canônica:
  `usb:VID:PID:instância` (serial, ou índice estável), `bt:AA:BB:CC:DD:EE:FF` (só o MAC,
  para BlueZ e PipeWire chegarem ao mesmo id), `display:chave-do-EDID` e `other:provedor:chave`.
- **`DeviceInfo`**: id, nome, fabricante, `DeviceKind` (mouse, keyboard, headset, gamepad,
  microphone, monitor, rgb_controller, other), `providers` (em ordem de prioridade) e capacidades.
- **`Capability`**: algo que o dispositivo sabe fazer, com valor atual, limites e a flag `writable`:
  `Battery`, `Dpi`, `PollingRate`, `Rgb`, `Volume`, `Mute`, `Brightness`, `Contrast`, `Rumble`, `Connection`.
  Bateria e conexão são sempre somente leitura.
- **`CapabilityValue`**: o que se envia num `set`. É validado contra a capacidade
  (`DeviceInfo::validate`) antes de chegar ao provedor. Erros: `Unsupported`, `ReadOnly`, `OutOfRange`.
- **`Provider`** (`async-trait`, usado como `Box<dyn Provider>`): `id`, `discover`, `read`,
  `write` e `events` (fluxo de `ProviderEvent`: `DeviceAdded`, `DeviceRemoved`, `CapabilityChanged`).
- **Fusão** (`fuse`, `capability_owner`): um mesmo dispositivo físico pode ser atendido por mais
  de um provedor (ex.: headset Bluetooth = bluez + pipewire). O daemon funde pelo id. Para cada
  tipo de capacidade **vence o primeiro provedor na ordem de prioridade**, e é ele que recebe
  o `read`/`write` daquela capacidade.
- **Testes**: a feature `mock` do `raiza-core` expõe `mock::MockProvider`.

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
