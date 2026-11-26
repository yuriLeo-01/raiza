# Contributing to Raiza

**English** · [Português](CONTRIBUTING.pt-BR.md)

Thanks for your interest! There are three main ways to help, from the simplest to the most
technical. Issues and pull requests are welcome in English or Portuguese.

## 1. Requesting support for a device

Open an issue with the **"Suporte a aparelho"** (device support) template. The most useful
information:

- the exact model (as printed on the box or the maker's website);
- the output of `lsusb` with the device plugged in (its line is enough);
- how it connects: cable, 2.4 GHz dongle, Bluetooth (some use all three);
- what you'd like to control (battery, DPI, RGB…);
- whether there's official software (Windows, web) and whether you know of an open project that
  already understands the protocol (OpenRGB, configurators on GitHub, etc.).

## 2. Contributing a profile

A profile is a JSON file in `profiles/<brand>/` that describes how to read and write a model's
capabilities through `/dev/hidraw`. No Rust code needed. The full format is in
[`docs/PERFIS.md`](docs/PERFIS.md) (in Portuguese), and editors validate the file against
[`profiles/schema.json`](profiles/schema.json).

### Where the bytes come from

1. **Open documentation:** many protocols have already been decoded by other projects. The current
   profiles came from projects like these (see [`docs/protocolos/`](docs/protocolos/)). Always cite
   the source and check that its license is compatible with GPL-3.0.
2. **Reports the device sends on its own:** many mice send a periodic status with battery and DPI.
   You can find it just by listening, without sending anything.
3. **Captures of the official software:** with `usbmon`/Wireshark and the software running in a
   Windows virtual machine with USB passthrough. This is the most laborious path.

### From a profile to writes

1. Write the profile with **`"verified": false`**. This makes it **read-only**: Raiza only sends
   the fixed queries described in the profile, never writes.
2. Install the profile in `~/.local/share/raiza/profiles/` (the daemon reloads it on its own) and
   check the values with `raiza-ctl list`, **comparing them with the official software**.
3. To enable a write, test the command on the device **once, by hand**: write a value, read it
   back, check it in the official software and restore the original value. Record the bytes and the
   result in `docs/protocolos/<device>.md`.
4. Only then does the profile become `"verified": true`. During review, the maintainers check the
   test record.

### Safety rules (pillar 1)

- **Never** include firmware update, bootloader or memory erase commands. Validation blocks the
  known patterns, but the profile author is responsible.
- Write the **minimum** number of bytes. If the official software writes a whole block, find out
  what each byte does before copying it. On the Rapoo, the polling-rate block also held the sensor
  scan rate.
- When the same VID:PID is used by different devices (common with keyboards), the profile **must**
  have `identify`.
- When in doubt, keep it read-only.

## 3. Contributing code

Raiza is written in Rust (2024 edition). Read first (in Portuguese):

- [`docs/PILARES.md`](docs/PILARES.md): the principles that guide the project;
- [`docs/ARQUITETURA.md`](docs/ARQUITETURA.md): crates, providers and merging;
- [`AGENTS.md`](AGENTS.md), "Regras de código" section: no `unwrap()` on hardware paths, hardware
  I/O behind an injectable trait (so it can be tested without the device), logging with `tracing`,
  comments and documentation in Portuguese, code identifiers in English.

To test without hardware, use the fake daemon:

```sh
systemctl --user stop raizad      # if Raiza is installed
cargo run -p raizad -- --mock
cargo run -p raiza-gtk            # or: cargo run -p raiza-ctl -- list
```

Before opening a pull request, all of this must pass:

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

The public contracts (`crates/raiza-core` and [`docs/DBUS.md`](docs/DBUS.md)) change carefully:
open an issue to discuss first.

## Reporting a bug

Use the **"Bug"** template and include:

- `raiza-ctl list --json` (shows what the daemon sees);
- `journalctl --user -u raizad --since "-10min"` (the daemon logs);
- the version: `pacman -Q raiza-git`, or the commit if you built it yourself;
- what you did, what you expected and what happened.

`raiza-ctl list --json` includes device serial numbers. Feel free to remove them before pasting.
