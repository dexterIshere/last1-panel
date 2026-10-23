//! Portables Razer Blade : leur contrôleur (EC) garde sous Linux son mode par
//! défaut, alors que Razer Synapse le passe sous Windows en mode Gaming et
//! pousse les ventilateurs. Last1 Panel fait de même le temps des jeux, sur
//! secteur, par le port USB interne du clavier (rapports HID « feature »,
//! protocole de razer-laptop-control, Razer-Linux/razer-laptop-control-no-dkms,
//! GPL-2.0 ou ultérieure, et d'openrazer), puis rend le mode d'avant.
//!
//! /dev/hidraw* est à root : le bouton du diagnostic (Fix::RazerCooling) pose
//! une règle udev « uaccess » pour ces seuls périphériques.

use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

pub const VENDOR: &str = "1532";
pub const UDEV_RULE: &str = "/etc/udev/rules.d/70-last1-panel-razer.rules";

/// Portables connus de razer-laptop-control : identifiant produit et vitesse
/// la plus haute de leurs ventilateurs (tr/min).
pub const LAPTOPS: [(u16, u16); 37] = [
    (0x0224, 5000),
    (0x0233, 5000),
    (0x023B, 5000),
    (0x0240, 5000),
    (0x0246, 5000),
    (0x023A, 5300),
    (0x0245, 5300),
    (0x0255, 5000),
    (0x0268, 5200),
    (0x0253, 5300),
    (0x022D, 5000),
    (0x0232, 5000),
    (0x0239, 5300),
    (0x024A, 5000),
    (0x0252, 5000),
    (0x0256, 5300),
    (0x0234, 5300),
    (0x022F, 5000),
    (0x0225, 5000),
    (0x0210, 5000),
    (0x020F, 5000),
    (0x026A, 5000),
    (0x026F, 5000),
    (0x0270, 5000),
    (0x0276, 5000),
    (0x026D, 5000),
    (0x027A, 5000),
    (0x028A, 5000),
    (0x028B, 5000),
    (0x028C, 5000),
    (0x0259, 5000),
    (0x029F, 5000),
    (0x029D, 5000),
    (0x026E, 4300),
    (0x02B6, 5000),
    (0x0279, 4300),
    (0x02A0, 5000),
];

/// Modes du contrôleur.
pub const BALANCED: u8 = 0;
pub const GAMING: u8 = 1;

/// Le portable Razer de ce PC : son nœud hidraw (interface 0) et ses
/// ventilateurs.
#[derive(Debug, Clone, PartialEq)]
pub struct Laptop {
    pub product: u16,
    pub hidraw: PathBuf,
    pub fan_max: u16,
}

/// Cherche l'interface 0 d'un portable Razer connu (/sys/class/hidraw).
pub fn find(sys: &Path, dev: &Path) -> Option<Laptop> {
    fs::read_dir(sys.join("class/hidraw")).ok()?.flatten().find_map(|entry| {
        let device = fs::canonicalize(entry.path().join("device")).ok()?;
        // « 0003:1532:0279.0004 »
        let name = device.file_name()?.to_string_lossy().into_owned();
        let mut parts = name.split([':', '.']);
        let (_, vendor, product) = (parts.next()?, parts.next()?, parts.next()?);
        if !vendor.eq_ignore_ascii_case(VENDOR) {
            return None;
        }
        let product = u16::from_str_radix(product, 16).ok()?;
        let fan_max = LAPTOPS.iter().find(|(id, _)| *id == product)?.1;
        let interface = fs::read_to_string(device.parent()?.join("bInterfaceNumber")).ok()?;
        (interface.trim() == "00").then(|| Laptop { product, hidraw: dev.join(entry.file_name()), fan_max })
    })
}

/// Ce compte peut-il lui parler (règle udev posée) ?
pub fn writable(laptop: &Laptop) -> bool {
    fs::OpenOptions::new().read(true).write(true).open(&laptop.hidraw).is_ok()
}

/// Un rapport de 91 octets : numéro de rapport 0, puis le paquet Razer
/// (état, transaction 0x1f, paquets restants, protocole, taille, classe,
/// commande, 80 octets d'arguments, somme de contrôle, réservé).
pub fn packet(class: u8, command: u8, args: &[u8]) -> [u8; 91] {
    let mut buf = [0u8; 91];
    buf[2] = 0x1f;
    buf[6] = args.len() as u8;
    buf[7] = class;
    buf[8] = command;
    buf[9..9 + args.len()].copy_from_slice(args);
    // Comme openrazer : XOR des octets 2 à 87 du paquet (le numéro de
    // rapport en plus devant).
    buf[89] = buf[3..89].iter().fold(0, |crc, b| crc ^ b);
    buf
}

/// _IOC(_IOC_READ | _IOC_WRITE, 'H', nr, len) de linux/hidraw.h.
const fn hid_ioctl(nr: u64, len: u64) -> u64 {
    (3 << 30) | (len << 16) | ((b'H' as u64) << 8) | nr
}

/// Envoie une commande et rend la réponse (ses arguments).
fn send(laptop: &Laptop, class: u8, command: u8, args: &[u8]) -> io::Result<[u8; 80]> {
    let file = fs::OpenOptions::new().read(true).write(true).open(&laptop.hidraw)?;
    let mut request = packet(class, command, args);
    let mut response = [0u8; 91];
    for _ in 0..3 {
        // SAFETY : ioctl HIDIOCSFEATURE / HIDIOCGFEATURE sur un tampon de la
        // taille annoncée.
        let sent = unsafe { libc::ioctl(file.as_raw_fd(), hid_ioctl(0x06, 91) as _, request.as_mut_ptr()) };
        if sent < 0 {
            return Err(io::Error::last_os_error());
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
        response = [0u8; 91];
        let got = unsafe { libc::ioctl(file.as_raw_fd(), hid_ioctl(0x07, 91) as _, response.as_mut_ptr()) };
        // 0x02 : commande faite.
        if got >= 0 && response[1] == 0x02 && response[7] == class && response[8] == command {
            let mut out = [0u8; 80];
            out.copy_from_slice(&response[9..89]);
            return Ok(out);
        }
    }
    Err(io::Error::other(format!("le contrôleur Razer a répondu {:#04x}", response[1])))
}

/// Mode actuel (Balanced 0, Gaming 1, Creator 2, Custom 4).
pub fn mode(laptop: &Laptop) -> io::Result<u8> {
    send(laptop, 0x0d, 0x82, &[0x00, 0x01, 0x00, 0x00]).map(|args| args[2])
}

/// Mode Gaming et ventilateurs à fond, sur les deux zones.
pub fn gaming(laptop: &Laptop) -> io::Result<()> {
    let rpm = (laptop.fan_max / 100) as u8;
    for zone in [0x01, 0x02] {
        send(laptop, 0x0d, 0x02, &[0x00, zone, GAMING, 0x01])?;
        send(laptop, 0x0d, 0x01, &[0x00, zone, rpm])?;
    }
    Ok(())
}

/// Rend un mode, ventilateurs en automatique.
pub fn restore(laptop: &Laptop, mode: u8) -> io::Result<()> {
    for zone in [0x01, 0x02] {
        send(laptop, 0x0d, 0x02, &[0x00, zone, mode, 0x00])?;
    }
    Ok(())
}

/// Règle udev : les portables connus, au compte connecté.
pub fn udev_rule() -> String {
    let mut text =
        String::from("# Last1 Panel : contrôleur des portables Razer (mode, ventilateurs) pendant les jeux\n");
    for (product, _) in LAPTOPS {
        text.push_str(&format!(
            "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{VENDOR}\", ATTRS{{idProduct}}==\"{product:04x}\", TAG+=\"uaccess\"\n"
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packet_is_laid_out_like_openrazer() {
        let p = packet(0x0d, 0x02, &[0x00, 0x01, GAMING, 0x01]);
        assert_eq!(&p[..10], &[0x00, 0x00, 0x1f, 0x00, 0x00, 0x00, 0x04, 0x0d, 0x02, 0x00]);
        assert_eq!(&p[10..13], &[0x01, GAMING, 0x01]);
        // XOR de la taille, de la classe, de la commande et des arguments.
        assert_eq!(p[89], 0x04 ^ 0x0d ^ 0x02 ^ 0x01 ^ GAMING ^ 0x01);
        assert_eq!(hid_ioctl(0x06, 91), 0xC05B_4806);
    }

    #[test]
    fn blade_17_2021_is_found_on_its_interface_0() {
        let dir = tempfile::tempdir().unwrap();
        let sys = dir.path().join("sys");
        let usb = dir.path().join("devices/usb1/1-8");
        let hid = |interface: &str, n: u8| {
            let node = usb.join(format!("1-8:1.{interface}"));
            let device = node.join(format!("0003:1532:0279.000{n}"));
            fs::create_dir_all(&device).unwrap();
            fs::write(node.join("bInterfaceNumber"), format!("0{interface}\n")).unwrap();
            let class = sys.join(format!("class/hidraw/hidraw{n}"));
            fs::create_dir_all(&class).unwrap();
            std::os::unix::fs::symlink(&device, class.join("device")).unwrap();
        };
        hid("1", 3);
        assert_eq!(find(&sys, Path::new("/dev")), None, "seulement l'interface 0");
        hid("0", 2);
        assert_eq!(
            find(&sys, Path::new("/dev")),
            Some(Laptop { product: 0x0279, hidraw: PathBuf::from("/dev/hidraw2"), fan_max: 4300 })
        );
        assert!(udev_rule().contains("ATTRS{idProduct}==\"0279\", TAG+=\"uaccess\""));
    }
}
