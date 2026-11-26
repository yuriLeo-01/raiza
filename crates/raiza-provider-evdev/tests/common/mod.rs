//! Sysfs falso em diretório temporário, mais fontes e backends falsos.
#![allow(dead_code)]

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use raiza_core::RaizaError;
use raiza_provider_evdev::{ChangeStream, InputNode, PadSource, Pulse, RumbleBackend, Sysfs};
use tempfile::TempDir;
use tokio::sync::mpsc;

pub struct Fake {
    pub dir: TempDir,
}

pub fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

impl Fake {
    pub fn new() -> Self {
        Fake {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.dir.path().to_path_buf()
    }

    pub fn sysfs(&self) -> Sysfs {
        Sysfs::new(self.root())
    }

    /// Dispositivo USB em `devices/.../<porta>`, listado em `bus/usb/devices`.
    pub fn usb(&self, port: &str, vid: &str, pid: &str, serial: Option<&str>) -> PathBuf {
        let dir = self.root().join("devices/pci0000:00/usb1").join(port);
        write(&dir.join("idVendor"), &format!("{vid}\n"));
        write(&dir.join("idProduct"), &format!("{pid}\n"));
        write(&dir.join("manufacturer"), "Sony\n");
        if let Some(s) = serial {
            write(&dir.join("serial"), &format!("{s}\n"));
        }
        let bus = self.root().join("bus/usb/devices");
        fs::create_dir_all(&bus).unwrap();
        symlink(&dir, bus.join(port)).unwrap();
        dir
    }

    /// Controle (HID + input) sob `parent`. `ff` é o conteúdo de `capabilities/ff`.
    pub fn pad(
        &self,
        parent: &Path,
        hid: &str,
        n: u32,
        bustype: &str,
        uniq: &str,
        ff: Option<&str>,
    ) -> (InputNode, PathBuf) {
        let hid_dir = parent.join(hid);
        let input = hid_dir.join(format!("input/input{n}"));
        write(&input.join("name"), "Wireless Controller\n");
        write(&input.join("uniq"), &format!("{uniq}\n"));
        write(&input.join("id/bustype"), &format!("{bustype}\n"));
        write(&input.join("id/vendor"), "054c\n");
        write(&input.join("id/product"), "0ce6\n");
        if let Some(ff) = ff {
            write(&input.join("capabilities/ff"), &format!("{ff}\n"));
        }
        let node = InputNode {
            devnode: PathBuf::from(format!("/dev/input/event{n}")),
            input_syspath: input,
        };
        (node, hid_dir)
    }

    /// `power_supply` ligado ao dispositivo `owner`.
    pub fn battery(&self, name: &str, owner: &Path, attrs: &[(&str, &str)]) {
        let dir = self.root().join("class/power_supply").join(name);
        fs::create_dir_all(&dir).unwrap();
        symlink(owner, dir.join("device")).unwrap();
        for (k, v) in attrs {
            write(&dir.join(k), &format!("{v}\n"));
        }
    }
}

/// `capabilities/ff` (palavras de 64 bits, a mais significativa primeiro) com o bit
/// `FF_RUMBLE` (0x50 = palavra 1, bit 16).
pub const FF_RUMBLE_CAPS: &str = "10000 0";
/// FF sem vibração: só o bit 0x4f (palavra 1, bit 15).
pub const FF_SEM_RUMBLE: &str = "8000 0";

/// Fonte falsa: a lista muda quando o teste quer, e `avisar` simula um evento udev.
#[derive(Clone)]
pub struct Falsa {
    pub nos: Arc<Mutex<Vec<InputNode>>>,
    pub eventos: Arc<Mutex<Option<mpsc::UnboundedReceiver<()>>>>,
}

impl Falsa {
    pub fn nova(nos: Vec<InputNode>) -> (Self, mpsc::UnboundedSender<()>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Falsa {
                nos: Arc::new(Mutex::new(nos)),
                eventos: Arc::new(Mutex::new(Some(rx))),
            },
            tx,
        )
    }
}

impl PadSource for Falsa {
    fn list(&self) -> Result<Vec<InputNode>, RaizaError> {
        Ok(self.nos.lock().unwrap().clone())
    }

    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        let rx = self
            .eventos
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| RaizaError::Io("monitor ausente".into()))?;
        Ok(Box::pin(futures_util::stream::unfold(rx, |mut rx| async {
            rx.recv().await.map(|()| ((), rx))
        })))
    }
}

/// Fonte que sempre falha: udev ausente.
pub struct Quebrada;

impl PadSource for Quebrada {
    fn list(&self) -> Result<Vec<InputNode>, RaizaError> {
        Err(RaizaError::Io("udev ausente".into()))
    }
    fn watch(&self) -> Result<ChangeStream, RaizaError> {
        Err(RaizaError::Io("udev ausente".into()))
    }
}

/// Backend simulado: grava o que recebeu.
#[derive(Clone, Default)]
pub struct Gravador {
    pub pulsos: Arc<Mutex<Vec<(PathBuf, Pulse)>>>,
}

impl RumbleBackend for Gravador {
    fn play(&self, devnode: &Path, pulse: Pulse) -> Result<(), RaizaError> {
        self.pulsos
            .lock()
            .unwrap()
            .push((devnode.to_path_buf(), pulse));
        Ok(())
    }
}
