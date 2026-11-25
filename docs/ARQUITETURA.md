# Arquitetura

```
                          ┌─► raiza-gtk   (GTK4/libadwaita, GNOME)
                          ├─► raiza-ctl   (linha de comando)
raizad ◄──── D-Bus ───────┤
  │   (sessão do usuário) └─► raiza-kde   (Kirigami, futuro)
  │
  ├── registro de provedores
  │     ├── udev        → descoberta e hotplug de dispositivos
  │     ├── bluez       → conexão Bluetooth, bateria de headsets
  │     ├── upower      → bateria de mouses/teclados sem fio
  │     ├── pipewire    → volume e mudo de microfones/headsets
  │     ├── evdev       → controles: bateria, teste de vibração (prioridade menor)
  │     ├── openrgb     → RGB via OpenRGB (adiado)
  │     └── hid-perfis  → motor de perfis JSON via /dev/hidraw*

  Monitores estão fora do escopo (docs/PILARES.md): não há provedor DDC/CI, e o udev
  ignora portas USB de controle de monitor.
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
  para BlueZ e PipeWire chegarem ao mesmo id) e `other:provedor:chave`.
- **`DeviceInfo`**: id, nome, fabricante, `DeviceKind` (mouse, keyboard, headset, gamepad,
  microphone, rgb_controller, other), `providers` (em ordem de prioridade) e capacidades.
- **`Capability`**: algo que o dispositivo sabe fazer, com valor atual, limites e a flag `writable`:
  `Battery`, `Dpi`, `PollingRate`, `Rgb`, `Volume`, `Mute`, `Rumble`, `Connection`. `Sidetone` e
  `Equalizer` entram com o headset Akko S9 Ultra (T20/T21).
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

### Identidade: como cada provedor gera o `DeviceId`

A fusão só funciona se todo provedor chegar ao **mesmo** id para o mesmo aparelho. Regras:

| Aparelho | Id | Como obter |
|---|---|---|
| USB (com fio ou dongle) | `DeviceId::usb(vid, pid, DeviceId::usb_instance(serial, porta, portas_do_mesmo_modelo_sem_serial))` | Suba no sysfs até o dispositivo USB pai (`/sys/bus/usb/devices/<porta>`, com `idVendor`, `idProduct`, `serial`) |
| Bluetooth | `DeviceId::bluetooth(mac)` | BlueZ: `Address`; PipeWire: `api.bluez5.address`; evdev/HID: atributo `uniq` |
| Sem nada disso | `DeviceId::Other { provider, key }` | Chave estável do próprio provedor (não funde) |

Nunca invente outro formato de instância USB: use `DeviceId::usb_instance`.

**Um aparelho, duas conexões** (ex.: teclado com cabo e com dongle próprio): cada conexão tem
o próprio `DeviceId`, o que mantém a fusão com o `udev` em cada uma. Se o protocolo é o mesmo,
um único perfil HID lista as duas em `match`, e a primeira é a preferida. Com as duas presentes
ao mesmo tempo, o `hid-perfis` anuncia as capacidades só na preferida, para a interface não
mostrar dois controles do mesmo aparelho. Limitação conhecida: as configurações salvas são por
`DeviceId`, então não acompanham o aparelho de uma conexão para a outra.

### Atualização de um dispositivo

Um provedor pode anunciar que um dispositivo mudou (nome, tipo ou capacidades) como
`DeviceRemoved` seguido de `DeviceAdded` com o mesmo id. O daemon trata o par como
atualização do `DeviceInfo` daquele provedor, sem apagar as configurações salvas.

### Prioridade dos provedores

O daemon registra os provedores nesta ordem. Na fusão, para cada capacidade, vence o primeiro:

1. `hid-perfis` 2. `openrgb` (adiado) 3. `bluez` 4. `upower` 5. `pipewire` 6. `evdev` 7. `udev`

Os específicos vêm antes dos genéricos. O `udev` vem por último: ele só garante que todo
aparelho apareça, com `Connection` somente leitura.

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
- Perfis HID novos começam como `"verified": false` e ficam em somente leitura (pilar 1). Formato em `docs/PERFIS.md`.
