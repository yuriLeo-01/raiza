//! Registro dos provedores, em **um lugar só** e em ordem de prioridade
//! (`docs/ARQUITETURA.md`, "Prioridade dos provedores"). Na fusão, vence o primeiro.

use raiza_core::mock::MockProvider;
use raiza_core::{
    Capability, ConnectionKind, DeviceId, DeviceInfo, DeviceKind, Level, Provider, RgbZone,
};
use raiza_provider_bluez::{BluezProvider, UpowerProvider};
use raiza_provider_evdev::EvdevProvider;
use raiza_provider_hid::{DirProfiles, HidProfilesProvider, SystemHid, Timeouts};
use raiza_provider_pipewire::PipeWireProvider;
use raiza_provider_udev::UdevProvider;

/// Provedores reais, do mais específico ao mais genérico. Para integrar um provedor
/// novo, acrescente-o aqui na posição da sua prioridade:
///
/// 1. `hid-perfis` 2. `openrgb` (adiado) 3. `bluez` 4. `upower` 5. `pipewire` 6. `evdev` 7. `udev`
pub fn system_providers() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(hid_profiles()),
        // 2. openrgb (adiado)
        Box::new(BluezProvider::system()),
        Box::new(UpowerProvider::system()),
        Box::new(PipeWireProvider::new()),
        Box::new(EvdevProvider::new()),
        Box::new(UdevProvider::new()),
    ]
}

/// Executor de perfis HID. `RAIZA_PROFILES` (diretórios separados por `:`) vem antes dos
/// diretórios padrão; útil para desenvolver com o `profiles/` do repositório:
/// `RAIZA_PROFILES=$PWD/profiles cargo run -p raizad`.
fn hid_profiles() -> HidProfilesProvider {
    let mut dirs: Vec<std::path::PathBuf> = std::env::var_os("RAIZA_PROFILES")
        .map(|v| std::env::split_paths(&v).collect())
        .unwrap_or_default();
    dirs.extend(DirProfiles::default_dirs());
    HidProfilesProvider::with_parts(
        SystemHid::new(),
        DirProfiles::new(dirs),
        Timeouts::default(),
    )
}

/// Só o provedor falso, com dispositivos de demonstração (`raizad --mock`).
pub fn mock_providers() -> Vec<Box<dyn Provider>> {
    let mut mock = MockProvider::new("mock");
    for device in mock_devices() {
        mock = mock.with_device(device);
    }
    vec![Box::new(mock)]
}

/// Dispositivos fictícios do modo `--mock`.
pub fn mock_devices() -> Vec<DeviceInfo> {
    let cor = |r, g, b| raiza_core::Color { r, g, b };
    vec![
        DeviceInfo {
            id: DeviceId::Other {
                provider: "mock".into(),
                key: "mouse".into(),
            },
            name: "Mouse de teste".into(),
            vendor: Some("Raiza".into()),
            kind: DeviceKind::Mouse,
            providers: vec!["mock".into()],
            capabilities: vec![
                Capability::Dpi {
                    level: Level {
                        current: 1600,
                        min: 400,
                        max: 16000,
                        step: 50,
                    },
                    stages: vec![800, 1600, 3200],
                    active_stage: None,
                    writable: true,
                },
                Capability::PollingRate {
                    current: 1000,
                    supported: vec![125, 250, 500, 1000],
                    writable: true,
                },
                Capability::Rgb {
                    zones: vec![
                        RgbZone {
                            name: "logo".into(),
                            color: cor(0, 0xff, 0),
                        },
                        RgbZone {
                            name: "roda".into(),
                            color: cor(0, 0, 0xff),
                        },
                    ],
                    modes: vec!["static".into(), "breathing".into(), "off".into()],
                    current_mode: "static".into(),
                    writable: true,
                },
                Capability::Battery {
                    percent: 80,
                    charging: false,
                },
                Capability::Connection {
                    medium: ConnectionKind::Wireless,
                    connected: true,
                    writable: false,
                },
            ],
        },
        DeviceInfo {
            id: DeviceId::Other {
                provider: "mock".into(),
                key: "headset".into(),
            },
            name: "Headset de teste".into(),
            vendor: None,
            kind: DeviceKind::Headset,
            providers: vec!["mock".into()],
            capabilities: vec![
                Capability::Volume {
                    level: Level {
                        current: 50,
                        min: 0,
                        max: 100,
                        step: 1,
                    },
                    writable: true,
                },
                Capability::Mute {
                    muted: false,
                    writable: true,
                },
                Capability::Battery {
                    percent: 45,
                    charging: true,
                },
            ],
        },
    ]
}
