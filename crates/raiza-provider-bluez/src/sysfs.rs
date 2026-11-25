//! Resolução do id USB de uma fonte de energia do kernel (`/sys/class/power_supply`).

use std::fs;
use std::path::{Path, PathBuf};

use raiza_core::DeviceId;
use tracing::debug;

/// Descobre o [`DeviceId`] USB do aparelho dono de uma bateria do kernel.
pub trait UsbResolver: Send + Sync + 'static {
    /// `native_path` é o nome em `/sys/class/power_supply/` (o `NativePath` do UPower).
    /// Devolve `None` se a bateria não pertence a um dispositivo USB.
    fn usb_id(&self, native_path: &str) -> Option<DeviceId>;
}

/// [`UsbResolver`] que lê o sysfs.
///
/// Sobe de `/sys/class/power_supply/<nome>/device` até o primeiro diretório com
/// `idVendor` e `idProduct` (o dispositivo USB pai) e monta o id com
/// [`DeviceId::usb_instance`], como os outros provedores.
#[derive(Debug, Clone)]
pub struct SysfsUsbResolver {
    root: PathBuf,
}

impl Default for SysfsUsbResolver {
    fn default() -> Self {
        Self::with_root("/sys")
    }
}

impl SysfsUsbResolver {
    /// Usa o `/sys` real.
    pub fn new() -> Self {
        Self::default()
    }

    /// Usa outra raiz no lugar de `/sys` (testes).
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        SysfsUsbResolver { root: root.into() }
    }

    fn usb_parent(&self, native_path: &str) -> Option<PathBuf> {
        let valid = !native_path.is_empty() && !native_path.contains('/') && native_path != "..";
        if !valid {
            return None;
        }
        let root = fs::canonicalize(&self.root).ok()?;
        let device = self
            .root
            .join("class/power_supply")
            .join(native_path)
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

impl UsbResolver for SysfsUsbResolver {
    fn usb_id(&self, native_path: &str) -> Option<DeviceId> {
        let dir = self.usb_parent(native_path)?;
        let vid = read_hex(&dir, "idVendor")?;
        let pid = read_hex(&dir, "idProduct")?;
        let port = dir.file_name()?.to_str()?.to_owned();
        let serial = read_trimmed(&dir, "serial");
        let ports = self.ports_without_serial(vid, pid);
        let ports: Vec<&str> = ports.iter().map(String::as_str).collect();
        let instance = DeviceId::usb_instance(serial.as_deref(), &port, &ports);
        debug!(native_path, port, "bateria resolvida para dispositivo USB");
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

    /// Monta um sysfs falso com um receptor USB (porta `1-3`) e a bateria do mouse.
    fn sysfs(serial: Option<&str>, gemeo_sem_serial: bool) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let usb = root.join("devices/pci0000:00/usb1/1-3");
        let hid = usb.join("1-3:1.2/0003:046D:C52B.0003/0003:046D:4082.0004");
        let bat = hid.join("power_supply/hidpp_battery_0");
        fs::create_dir_all(&bat).unwrap();
        fs::write(usb.join("idVendor"), "046d\n").unwrap();
        fs::write(usb.join("idProduct"), "c52b\n").unwrap();
        if let Some(s) = serial {
            fs::write(usb.join("serial"), s).unwrap();
        }
        symlink("../..", bat.join("device")).unwrap();
        fs::create_dir_all(root.join("class/power_supply")).unwrap();
        symlink(&bat, root.join("class/power_supply/hidpp_battery_0")).unwrap();
        // Bateria do notebook: sem USB no caminho.
        let bat0 = root.join("devices/LNXSYSTM:00/PNP0C0A:00/power_supply/BAT0");
        fs::create_dir_all(&bat0).unwrap();
        symlink("../..", bat0.join("device")).unwrap();
        symlink(&bat0, root.join("class/power_supply/BAT0")).unwrap();

        let devices = root.join("bus/usb/devices");
        fs::create_dir_all(&devices).unwrap();
        symlink(&usb, devices.join("1-3")).unwrap();
        symlink(usb.join("1-3:1.2"), devices.join("1-3:1.2")).unwrap();
        if gemeo_sem_serial {
            let outro = root.join("devices/pci0000:00/usb1/1-1");
            fs::create_dir_all(&outro).unwrap();
            fs::write(outro.join("idVendor"), "046d").unwrap();
            fs::write(outro.join("idProduct"), "c52b").unwrap();
            symlink(&outro, devices.join("1-1")).unwrap();
        }
        tmp
    }

    #[test]
    fn sobe_ate_o_usb_pai() {
        let tmp = sysfs(None, false);
        let r = SysfsUsbResolver::with_root(tmp.path());
        assert_eq!(
            r.usb_id("hidpp_battery_0"),
            Some(DeviceId::usb(0x046d, 0xc52b, "0"))
        );
    }

    #[test]
    fn usa_serial_ou_indice_entre_iguais() {
        let tmp = sysfs(Some(" ABC123\n"), true);
        let r = SysfsUsbResolver::with_root(tmp.path());
        assert_eq!(
            r.usb_id("hidpp_battery_0"),
            Some(DeviceId::usb(0x046d, 0xc52b, "ABC123"))
        );

        let tmp = sysfs(None, true);
        let r = SysfsUsbResolver::with_root(tmp.path());
        // Portas sem serial: 1-1 e 1-3 → o nosso (1-3) é o índice 1.
        assert_eq!(
            r.usb_id("hidpp_battery_0"),
            Some(DeviceId::usb(0x046d, 0xc52b, "1"))
        );
    }

    #[test]
    fn sem_usb_ou_nome_invalido() {
        let tmp = sysfs(None, false);
        let r = SysfsUsbResolver::with_root(tmp.path());
        assert_eq!(r.usb_id("BAT0"), None);
        assert_eq!(r.usb_id("nao-existe"), None);
        assert_eq!(r.usb_id("../BAT0"), None);
        assert_eq!(r.usb_id(""), None);
    }
}
