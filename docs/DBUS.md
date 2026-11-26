# API D-Bus do Raiza

Contrato estável entre o daemon `raizad` e qualquer cliente (`raiza-ctl`, `raiza-gtk`, KDE futuro).
Mudanças neste documento são do líder técnico.

| | |
|---|---|
| Barramento | sessão do usuário |
| Serviço | `io.github.yurileo01.Raiza.Daemon` |
| Objeto | `/io/github/yurileo01/Raiza` |
| Interface | `io.github.yurileo01.Raiza1` (versão 1 da API) |
| Erros | `io.github.yurileo01.Raiza1.Error.*` |

O serviço do daemon termina em `.Daemon` de propósito: `io.github.yurileo01.Raiza` é o **app id**
da interface GTK, e o GTK registra o app id como nome no barramento de sessão. Se os dois fossem
iguais, a janela e o daemon brigariam pelo nome (encontrado na T13).

Em Rust, use `raiza_core::dbus::RaizaClient` (feature `dbus` do `raiza-core`), que já
converte JSON e erros. Constantes: `DBUS_SERVICE`, `dbus::DBUS_PATH`, `dbus::DBUS_INTERFACE`, `dbus::API_VERSION`.

## Formato no fio

Ids e tipos simples vão como tipos D-Bus nativos. **Dispositivos, capacidades e valores vão
como JSON dentro de uma string (`s`)**, no mesmo formato serde dos tipos do `raiza-core`.

Por que JSON e não `a{sv}`:
- `Capability` é um enum com 8 variantes e dados aninhados (zonas RGB, estágios de DPI). Mapear
  isso para `a{sv}` exige uma conversão manual nos dois sentidos, ou seja, um segundo contrato
  para manter em sincronia com o serde. O formato JSON já é testado com ida e volta no `raiza-core`.
- Clientes em outras linguagens (QML/KDE, scripts) leem JSON com uma linha.
- O `busctl` continua prático: os exemplos abaixo são copiáveis.
- O custo (sem introspecção de tipos detalhada) é pequeno para uma API com quatro métodos.

### Formatos JSON

`DeviceId` (string): `usb:24ae:1423:0`, `bt:AA:BB:CC:DD:EE:FF`, `other:openrgb:0`.

`DeviceInfo`:
```json
{
  "id": "usb:24ae:1423:0",
  "name": "Rapoo VT9",
  "vendor": "Rapoo",
  "kind": "mouse",
  "providers": ["hid-perfis", "udev"],
  "capabilities": [
    {"kind": "dpi", "level": {"current": 1600, "min": 400, "max": 16000, "step": 50},
     "stages": [800, 1600, 3200], "writable": true},
    {"kind": "battery", "percent": 80, "charging": false},
    {"kind": "connection", "medium": "wired", "connected": true, "writable": false}
  ]
}
```

`kind` do dispositivo: `mouse`, `keyboard`, `headset`, `gamepad`, `microphone`,
`rgb_controller`, `other`.

`Capability` (campo `kind` + dados):

| `kind` | Campos |
|---|---|
| `battery` | `percent` (0–100), `charging` |
| `dpi` | `level`, `stages`, `writable` |
| `polling_rate` | `current` (Hz), `supported`, `writable` |
| `rgb` | `zones` (`[{"name", "color": "#rrggbb"}]`), `modes`, `current_mode`, `writable` |
| `volume` | `level`, `writable` |
| `mute` | `muted`, `writable` |
| `rumble` | `max_duration_ms`, `writable` |
| `connection` | `medium` (`wired`, `wireless`, `bluetooth`, `other`), `connected`, `writable` |

`level` = `{"current", "min", "max", "step"}`.

`CapabilityValue` (o que se manda em `SetCapability`):

| JSON | Efeito |
|---|---|
| `{"kind": "dpi", "value": 1600}` | DPI |
| `{"kind": "polling_rate", "value": 1000}` | taxa em Hz |
| `{"kind": "rgb", "value": {"zone": 0, "mode": "static", "color": "#00ff00"}}` | `zone` `null` = todas; `mode` ou `color` podem ser `null`, mas não os dois |
| `{"kind": "volume", "value": 50}` | volume (%) |
| `{"kind": "mute", "value": true}` | mudo |
| `{"kind": "rumble", "value": {"strength": 80, "duration_ms": 500}}` | pulso de vibração |
| `{"kind": "connection", "value": false}` | conectar (`true`) ou desconectar (`false`) |

## Métodos

| Método | Assinatura | Descrição |
|---|---|---|
| `ListDevices` | `() → s` | JSON de `[DeviceInfo]`: todos os dispositivos, **já fundidos** entre provedores |
| `GetDevice` | `(s id) → s` | JSON de `DeviceInfo` |
| `SetCapability` | `(s id, s value) → ()` | Valida `value` contra a capacidade e manda ao provedor dono dela. Retorna depois que o provedor aplicou |
| `Version` | `() → (u api, s daemon)` | Versão da API (`1`) e do daemon |

O daemon **sempre valida** o valor antes de chegar ao provedor (pilar 1). Uma capacidade com
`"writable": false` (incluindo as de perfis não verificados) recusa a escrita com `ReadOnly`.

## Sinais

| Sinal | Assinatura | Quando |
|---|---|---|
| `DeviceAdded` | `(s device)` | Um dispositivo apareceu. JSON de `DeviceInfo` fundido |
| `DeviceRemoved` | `(s id)` | O último provedor que atendia o dispositivo o perdeu |
| `CapabilityChanged` | `(s id, s capability)` | Uma capacidade mudou (por escrita ou por fora). JSON de `Capability` |

Se um segundo provedor passa a atender um dispositivo que já existe, o daemon emite
`CapabilityChanged` para cada capacidade nova, e não um novo `DeviceAdded`.

O daemon pode emitir `DeviceRemoved` seguido de `DeviceAdded` com o mesmo id quando um
dispositivo muda de nome, tipo ou conjunto de capacidades. Clientes devem tratar o par como
atualização, por exemplo mantendo a página do dispositivo aberta.

## Erros

| Nome | Quando | Mensagem (lida pelo cliente) |
|---|---|---|
| `io.github.yurileo01.Raiza1.Error.NotFound` | Dispositivo inexistente | o id |
| `…Error.Unsupported` | O dispositivo não tem a capacidade | o tipo (`dpi`) |
| `…Error.ReadOnly` | Capacidade somente leitura | o tipo |
| `…Error.OutOfRange` | Valor fora dos limites | `tipo: explicação` |
| `…Error.Io` | Falha ao falar com o dispositivo ou serviço | texto livre |
| `…Error.Permission` | Sem acesso (ex.: falta a regra udev) | texto livre |
| `…Error.InvalidArgument` | Id ou JSON ilegível | texto livre |

`raiza_core::dbus::DbusError` implementa esses nomes. O `RaizaClient` os converte de volta em
`RaizaError`.

## Exemplos com `busctl`

```sh
S="io.github.yurileo01.Raiza.Daemon"; O="/io/github/yurileo01/Raiza"; I="io.github.yurileo01.Raiza1"

busctl --user introspect $S $O
busctl --user call $S $O $I Version
busctl --user call $S $O $I ListDevices
busctl --user call $S $O $I GetDevice s "usb:24ae:1423:0"
busctl --user call $S $O $I SetCapability ss "usb:24ae:1423:0" '{"kind":"dpi","value":1600}'
busctl --user call $S $O $I SetCapability ss "bt:AA:BB:CC:DD:EE:FF" '{"kind":"volume","value":40}'

# Acompanhar os sinais
busctl --user monitor $S
```

Saída de um erro:
```
Call failed: dpi: 1625 não é múltiplo do passo 50 a partir de 400
```
(nome do erro: `io.github.yurileo01.Raiza1.Error.OutOfRange`).

## Implementação no daemon (T04)

O `raizad` implementa a interface com `#[zbus::interface(name = "io.github.yurileo01.Raiza1")]`
e devolve `Result<_, raiza_core::dbus::DbusError>`. Use `dbus::parse_id`, `dbus::from_wire` e
`dbus::to_wire` para ler e gerar o JSON: assim um id ou JSON inválido vira `InvalidArgument`.
O teste `dbus::tests::cliente_e_servidor_pelo_fio` do `raiza-core` mostra um servidor mínimo.

## Versionamento

- Acrescentar um campo opcional ao JSON, um método novo ou um sinal novo **não** quebra a versão 1
  (os clientes ignoram campos desconhecidos).
- Um **tipo novo de capacidade** (`kind` novo) quebra clientes que não o conhecem. Clientes de
  fora do repositório devem ignorar capacidades de `kind` desconhecido. Os clientes em Rust
  deste repositório são atualizados junto com o `raiza-core`.
- Remover ou mudar o significado de algo exige `io.github.yurileo01.Raiza2`. O daemon pode servir
  as duas interfaces durante a transição.
