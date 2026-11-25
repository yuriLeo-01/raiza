//! Da placa ALSA (`api.alsa.card`) ao id USB do aparelho, pelo sysfs.

use std::fs;
use std::path::{Path, PathBuf};

use raiza_core::DeviceId;
use tracing::debug;

/// Descobre o [`DeviceId`] USB do aparelho dono de uma placa de som ALSA.
pub trait CardResolver: Send + Sync + 'static {
    /// `card` é o número da placa (`api.alsa.card`, o `N` de `/sys/class/sound/cardN`).
    /// Devolve `None` se a placa não pertence a um dispositivo USB (onboard, PCI, HDMI).
    fn usb_id(&self, card: u32) -> Option<DeviceId>;
}

/// [`CardResolver`] que lê o sysfs.
///
/// Sobe de `/sys/class/sound/cardN/device` até o primeiro diretório com `idVendor` e
/// `idProduct` (o dispositivo USB pai) e monta o id com [`DeviceId::usb_instance`],
/// como os outros provedores.
#[derive(Debug, Clone)]
pub struct SysfsCardResolver {
    root: PathBuf,
}

impl Default for SysfsCardResolver {
    fn default() -> Self {
        Self::with_root("/sys")
    }
}

impl SysfsCardResolver {
    /// Usa o `/sys` real.
    pub fn new() -> Self {
        Self::default()
    }

    /// Usa outra raiz no lugar de `/sys` (testes).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        SysfsCardResolver { root: root.into() }
    }

    fn usb_parent(&self, card: u32) -> Option<PathBuf> {
        let root = fs::canonicalize(&self.root).ok()?;
        let device = self
            .root
            .join("class/sound")
            .join(format!("card{card}"))
            .join("device");
        let mut current = fs::canonicalize(device).ok()?;
        loop {
            if is_usb_device(&current) {
                return Some(current);
            }
            if !current.pop() || !current.starts_with(&root) || current == root {
                return None;
            }
        }
    }

    /// Portas USB dos aparelhos `vid:pid` conectados sem serial.
    fn ports_without_serial(&self, vid: u16, pid: u16) -> Vec<String> {
        let Ok(entries) = fs::read_dir(self.root.join("bus/usb/devices")) else {
            return Vec::new();
        };
        entries
            .flatten()
            .filter(|e| {
                let dir = e.path();
                is_usb_device(&dir)
                    && read_hex(&dir, "idVendor") == Some(vid)
                    && read_hex(&dir, "idProduct") == Some(pid)
                    && read_trimmed(&dir, "serial").is_none()
            })
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    }
}

impl CardResolver for SysfsCardResolver {
    fn usb_id(&self, card: u32) -> Option<DeviceId> {
        let Some(dir) = self.usb_parent(card) else {
            debug!(card, "placa de som sem dispositivo USB pai");
            return None;
        };
        let vid = read_hex(&dir, "idVendor")?;
        let pid = read_hex(&dir, "idProduct")?;
        let port = dir.file_name()?.to_str()?.to_owned();
        let serial = read_trimmed(&dir, "serial");
        let ports = self.ports_without_serial(vid, pid);
        let ports: Vec<&str> = ports.iter().map(String::as_str).collect();
        let instance = DeviceId::usb_instance(serial.as_deref(), &port, &ports);
        debug!(card, port, "placa de som resolvida para dispositivo USB");
        Some(DeviceId::usb(vid, pid, instance))
    }
}

fn is_usb_device(dir: &Path) -> bool {
    dir.join("idVendor").is_file() && dir.join("idProduct").is_file()
}

fn read_trimmed(dir: &Path, file: &str) -> Option<String> {
    let text = fs::read_to_string(dir.join(file)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn read_hex(dir: &Path, file: &str) -> Option<u16> {
    u16::from_str_radix(&read_trimmed(dir, file)?, 16).ok()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;

    /// Sysfs falso: o fifine (USB `1-2`, placa 4) e a placa onboard (PCI, placa 3).
    fn sysfs(serial: Option<&str>, gemeo_sem_serial: bool) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let usb = root.join("devices/pci0000:00/0000:0b:00.3/usb1/1-2");
        let card4 = usb.join("1-2:1.0/sound/card4");
        fs::create_dir_all(&card4).unwrap();
        fs::write(usb.join("idVendor"), "3142\n").unwrap();
        fs::write(usb.join("idProduct"), "a010\n").unwrap();
        if let Some(s) = serial {
            fs::write(usb.join("serial"), s).unwrap();
        }
        symlink("../..", card4.join("device")).unwrap();
        fs::create_dir_all(root.join("class/sound")).unwrap();
        symlink(&card4, root.join("class/sound/card4")).unwrap();

        let pci = root.join("devices/pci0000:00/0000:0b:00.6");
        let card3 = pci.join("sound/card3");
        fs::create_dir_all(&card3).unwrap();
        symlink("../..", card3.join("device")).unwrap();
        symlink(&card3, root.join("class/sound/card3")).unwrap();

        let devices = root.join("bus/usb/devices");
        fs::create_dir_all(&devices).unwrap();
        symlink(&usb, devices.join("1-2")).unwrap();
        symlink(usb.join("1-2:1.0"), devices.join("1-2:1.0")).unwrap();
        if gemeo_sem_serial {
            let outro = root.join("devices/pci0000:00/0000:0b:00.3/usb1/1-1");
            fs::create_dir_all(&outro).unwrap();
            fs::write(outro.join("idVendor"), "3142").unwrap();
            fs::write(outro.join("idProduct"), "a010").unwrap();
            symlink(&outro, devices.join("1-1")).unwrap();
        }
        tmp
    }

    #[test]
    fn sobe_ate_o_usb_pai() {
        let tmp = sysfs(None, false);
        let r = SysfsCardResolver::with_root(tmp.path());
        assert_eq!(r.usb_id(4), Some(DeviceId::usb(0x3142, 0xa010, "0")));
    }

    #[test]
    fn usa_serial_ou_indice_entre_iguais() {
        let tmp = sysfs(Some(" ABC123\n"), true);
        let r = SysfsCardResolver::with_root(tmp.path());
        assert_eq!(r.usb_id(4), Some(DeviceId::usb(0x3142, 0xa010, "ABC123")));

        let tmp = sysfs(None, true);
        let r = SysfsCardResolver::with_root(tmp.path());
        // Portas sem serial: 1-1 e 1-2 → a nossa (1-2) é o índice 1.
        assert_eq!(r.usb_id(4), Some(DeviceId::usb(0x3142, 0xa010, "1")));
    }

    #[test]
    fn onboard_ou_inexistente() {
        let tmp = sysfs(None, false);
        let r = SysfsCardResolver::with_root(tmp.path());
        assert_eq!(r.usb_id(3), None);
        assert_eq!(r.usb_id(9), None);
    }
}
