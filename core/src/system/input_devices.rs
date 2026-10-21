//! Souris, claviers et manettes branchés (/proc/bus/input/devices) : leur
//! modèle, leur identifiant USB et leur liaison, pour le diagnostic ; et ce
//! qui est branché ou débranché pendant une partie (le jeu a pu perdre la
//! souris à ce moment-là).

use crate::i18n::t;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Mouse,
    Keyboard,
    Gamepad,
}

/// Un périphérique de jeu.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Device {
    pub kind: Kind,
    pub name: String,
    /// « 046d:c547 ».
    pub id: String,
    /// Liaison : « usb », « usb-wireless » (récepteur), « bluetooth », « built-in ».
    pub link: &'static str,
}

impl Device {
    pub fn kind_name(&self) -> String {
        match self.kind {
            Kind::Mouse => t("Souris", "Mouse"),
            Kind::Keyboard => t("Clavier", "Keyboard"),
            Kind::Gamepad => t("Manette", "Gamepad"),
        }
    }

    fn link_name(&self) -> String {
        match self.link {
            "usb-wireless" => t("récepteur USB", "USB receiver"),
            "bluetooth" => "Bluetooth".to_string(),
            "built-in" => t("intégré", "built-in"),
            _ => t("câble USB", "USB cable"),
        }
    }

    /// « Logitech G Pro X Superlight (récepteur USB 046d:c547) ».
    pub fn describe(&self) -> String {
        format!("{} ({} {})", self.name, self.link_name(), self.id)
    }
}

/// Lit /proc/bus/input/devices : un bloc par périphérique.
pub fn parse(text: &str) -> Vec<Device> {
    let mut devices = BTreeSet::new();
    for block in text.split("\n\n") {
        let field = |prefix: &str| block.lines().find_map(|line| line.strip_prefix(prefix)).map(str::trim);
        let Some(info) = field("I:") else { continue };
        let value =
            |key: &str| info.split_whitespace().find_map(|part| part.strip_prefix(key)).map(|v| v.to_ascii_lowercase());
        let (Some(bus), Some(vendor), Some(product)) = (value("Bus="), value("Vendor="), value("Product=")) else {
            continue;
        };
        let name = field("N:").and_then(|n| n.strip_prefix("Name=")).unwrap_or("").trim_matches('"').to_string();
        let handlers = field("H:").and_then(|h| h.strip_prefix("Handlers=")).unwrap_or("");
        let lower = name.to_lowercase();
        // Ni pavé tactile, ni périphérique virtuel (ydotool, Steam Input…).
        if name.is_empty()
            || bus == "0006"
            || ["touchpad", "trackpad", "virtual", "ydotool", "consumer control", "system control"]
                .iter()
                .any(|word| lower.contains(word))
        {
            continue;
        }
        let words: Vec<&str> = handlers.split_whitespace().collect();
        let kind = if words.iter().any(|h| h.starts_with("js")) {
            Kind::Gamepad
        } else if words.iter().any(|h| h.starts_with("mouse")) {
            Kind::Mouse
        } else if words.contains(&"kbd") && block.lines().any(|line| line.starts_with("B: LED=")) {
            Kind::Keyboard
        } else {
            continue;
        };
        let link = match bus.as_str() {
            "0005" => "bluetooth",
            "0011" | "0018" | "0019" => "built-in",
            _ if ["receiver", "wireless", "dongle", "lightspeed", "2.4g"].iter().any(|w| lower.contains(w)) => {
                "usb-wireless"
            }
            _ => "usb",
        };
        devices.insert(Device { kind, name, id: format!("{vendor}:{product}"), link });
    }
    // Une souris en plusieurs interfaces (« … Mouse », « … Keyboard » du même
    // récepteur) : une ligne par nom et par sorte.
    devices.into_iter().collect()
}

/// Ceux de ce PC.
pub fn detect(proc_dir: &Path) -> Vec<Device> {
    parse(&fs::read_to_string(proc_dir.join("bus/input/devices")).unwrap_or_default())
}

/// Faits du diagnostic : une ligne par sorte (« Souris », « Clavier »,
/// « Manette »), les modèles séparés par « ; ».
pub fn facts(devices: &[Device]) -> Vec<(String, String)> {
    let mut facts: Vec<(String, String)> = Vec::new();
    for kind in [Kind::Mouse, Kind::Keyboard, Kind::Gamepad] {
        let names: Vec<String> = devices.iter().filter(|d| d.kind == kind).map(Device::describe).collect();
        if let Some(first) = devices.iter().find(|d| d.kind == kind) {
            facts.push((first.kind_name(), names.join(" ; ")));
        }
    }
    facts
}

/// Ce qui a changé entre deux relevés : « souris Logitech … (câble USB
/// 046d:c08b) branchée ».
pub fn changes(before: &[Device], after: &[Device]) -> Vec<String> {
    let mut out = Vec::new();
    for device in before.iter().filter(|d| !after.contains(d)) {
        out.push(t(
            &format!("{} {} débranché(e)", device.kind_name().to_lowercase(), device.describe()),
            &format!("{} {} unplugged", device.kind_name().to_lowercase(), device.describe()),
        ));
    }
    for device in after.iter().filter(|d| !before.contains(d)) {
        out.push(t(
            &format!("{} {} branché(e)", device.kind_name().to_lowercase(), device.describe()),
            &format!("{} {} plugged in", device.kind_name().to_lowercase(), device.describe()),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICES: &str = "I: Bus=0003 Vendor=046d Product=c547 Version=0111
N: Name=\"Logitech USB Receiver\"
P: Phys=usb-0000:00:14.0-2/input0
H: Handlers=mouse0 event4
B: EV=17

I: Bus=0003 Vendor=046d Product=c547 Version=0111
N: Name=\"Logitech USB Receiver Keyboard\"
H: Handlers=sysrq kbd leds event5
B: EV=12001f
B: LED=1f

I: Bus=0003 Vendor=046d Product=c547 Version=0111
N: Name=\"Logitech USB Receiver Consumer Control\"
H: Handlers=kbd event6

I: Bus=0003 Vendor=1532 Product=0084 Version=0111
N: Name=\"Razer Razer DeathAdder V2\"
H: Handlers=mouse1 event7

I: Bus=0005 Vendor=045e Product=0b13 Version=0509
N: Name=\"Xbox Wireless Controller\"
H: Handlers=event8 js0

I: Bus=0018 Vendor=06cb Product=ce26 Version=0100
N: Name=\"SYNA2393:00 06CB:CE26 Touchpad\"
H: Handlers=mouse2 event9

I: Bus=0006 Vendor=0000 Product=0000 Version=0000
N: Name=\"ydotoold virtual device\"
H: Handlers=mouse3 kbd event10
B: LED=0
";

    #[test]
    fn game_devices_with_model_id_and_link() {
        let devices = parse(DEVICES);
        assert_eq!(
            facts(&devices),
            [
                (
                    "Souris".to_string(),
                    "Logitech USB Receiver (récepteur USB 046d:c547) ; Razer Razer DeathAdder V2 (câble USB 1532:0084)"
                        .to_string()
                ),
                ("Clavier".to_string(), "Logitech USB Receiver Keyboard (récepteur USB 046d:c547)".to_string()),
                ("Manette".to_string(), "Xbox Wireless Controller (Bluetooth 045e:0b13)".to_string()),
            ]
        );
    }

    #[test]
    fn plugging_during_a_game_is_told() {
        let before = parse(DEVICES);
        // La souris sans fil vidée, rebranchée par son câble.
        let wired = DEVICES.replace("N: Name=\"Logitech USB Receiver\"", "N: Name=\"Logitech G Pro X Superlight\"");
        let after = parse(&wired.replace(
            "Product=c547 Version=0111\nN: Name=\"Logitech G Pro",
            "Product=c094 Version=0111\nN: Name=\"Logitech G Pro",
        ));
        assert_eq!(
            changes(&before, &after),
            [
                "souris Logitech USB Receiver (récepteur USB 046d:c547) débranché(e)",
                "souris Logitech G Pro X Superlight (câble USB 046d:c094) branché(e)",
            ]
        );
        assert!(changes(&after, &after).is_empty());
    }
}
