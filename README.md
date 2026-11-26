# Raiza

**English** · [Português](README.pt-BR.md)

**One hub to manage your peripherals on Linux, any brand, even without vendor support.**

Mice, keyboards, headsets and microphones (and gamepads, at a lower priority) in one place:
battery, DPI, polling rate, volume, mute and connection, with a native GNOME app
(GTK4/libadwaita) and a command-line tool.

> 🧪 **Test version (0.0.1).** It already works day to day with the devices in the table below, but
> it may still change a lot. Follow the roadmap in [`docs/tarefas/README.md`](docs/tarefas/README.md)
> (in Portuguese).

## Why?

Peripheral makers almost never ship software for Linux. Linux users end up juggling a different
tool for each brand, when one exists at all. Raiza brings everything into one hub and, for devices
nobody supports, lets you describe the protocol in **JSON profiles**: anyone can contribute a
profile without writing code.

## What works

### With any brand

Raiza builds on what Linux already knows about the device:

| Feature | Source | Examples |
|---|---|---|
| Battery | UPower (kernel drivers) | Logitech wireless, PS4/PS5 controllers, many wireless mice |
| Battery, connect and disconnect | BlueZ (Bluetooth) | headphones and mice with the standard battery service |
| Volume and mute | PipeWire | any headset or microphone |
| Rumble | evdev | gamepads |
| List of connected devices | udev | everything plugged in |

Devices that only report "connected" (hubs, controllers with nothing attached) are hidden by
default. The eye button in the app and `raiza-ctl list --all` show everything.

### With a profile (per model)

DPI, polling rate and battery over vendor protocols need a profile for the model, in
[`profiles/`](profiles/). New profiles start **read-only** and only start writing after being tested
on the real device (see [safety](#safety)).

| Device | Connection | Reads | Writes |
|---|---|---|---|
| Rapoo VT2 Max / VT2 Pro Max | dongle `24ae:1423`, cable `24ae:4623` | battery, charging, DPI, polling rate | DPI, polling rate |
| Akko Verge S9 Ultra (headset) | dongle `36eb:1087` | battery | — |
| Akko 5087 V3 HE (keyboard) | dongle `3151:5038` | battery | — |

Have a device that isn't listed? [Request support](https://github.com/yuriLeo-01/raiza/issues/new?template=aparelho.yml)
or [contribute a profile](CONTRIBUTING.md#2-contributing-a-profile).

## Install

### Arch and derivatives (CachyOS, EndeavourOS, Manjaro…)

```sh
git clone https://github.com/yuriLeo-01/raiza.git
cd raiza/packaging/aur
makepkg -si
systemctl --user enable --now raizad
```

Then **unplug and replug** your devices so the udev rule applies, and open "Raiza" from the
application menu.

For other distributions, see the manual installation in [`docs/INSTALACAO.md`](docs/INSTALACAO.md)
(in Portuguese; the commands are self-explanatory).

## Command line

```sh
raiza-ctl list                                  # devices and capabilities
raiza-ctl get usb:24ae:1423:2607-54L            # details of one device
raiza-ctl set usb:24ae:1423:2607-54L dpi 1600
raiza-ctl set usb:24ae:1423:2607-54L polling-rate 2000
raiza-ctl watch                                 # follow changes live
```

`--json` on `list`, `get` and `watch` gives script-friendly output. `raiza-ctl completions bash|zsh|fish`
generates shell completions (the package already installs them).

The interface is in Brazilian Portuguese for now; translations are welcome.

## Safety

Sending the wrong command to a peripheral can brick it. That's why Raiza follows strict rules
([pillar 1](docs/PILARES.md)):

- **No command is sent unless it's described and validated** in a profile. Every write is checked
  (limits, size, checksum) right before it goes to the device.
- **New profiles are read-only** (`"verified": false`) until someone tests them on the real device
  and compares the results with the official software.
- **Commands known to erase firmware or enter flashing mode are blocked** by validation, even if a
  profile asks for them.
- When the same USB id is shared by different devices, the profile requires the device to
  **identify** itself before any write.
- Device access comes from a udev rule that grants access **only to supported models**, for the
  logged-in user, without running anything as root.

## How it works

```
raizad (daemon, user session) ── D-Bus ──► raiza-gtk (app) · raiza-ctl (CLI)
   ├── HID profiles → /dev/hidraw  (DPI, polling rate, battery over vendor protocols)
   ├── BlueZ        → Bluetooth and battery
   ├── UPower       → battery
   ├── PipeWire     → volume and mute
   ├── evdev        → gamepad rumble
   └── udev         → device list and hotplug
```

When more than one source knows the same device, the daemon merges them, and the most specific
one (the profile) wins.

Design documents (in Portuguese):
- [Project pillars](docs/PILARES.md)
- [Architecture](docs/ARQUITETURA.md)
- [D-Bus API](docs/DBUS.md), for anyone building another interface
- [Profile format](docs/PERFIS.md)
- [Documented protocols](docs/protocolos/)
- [How the project is developed (a lead Claude agent plus subagents)](docs/ORQUESTRACAO.md)

## Development

Dependencies (Arch): `rust gtk4 libadwaita systemd-libs dbus`. HID access uses the native `hidapi`
backend, without the C library.

```sh
cargo build
systemctl --user stop raizad      # if Raiza is installed: frees the D-Bus name
cargo run -p raizad -- --mock     # daemon with fake devices, no hardware touched
cargo run -p raiza-gtk            # the app, connected to that daemon
```

Before opening a pull request:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[GPL-3.0-or-later](LICENSE).
