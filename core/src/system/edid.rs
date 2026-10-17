//! Nom des écrans, lu dans leur EDID (/sys/class/drm/cardN-<sortie>/edid).
//!
//! kscreen-doctor ne donne pas le modèle de l'écran : sans l'EDID, on
//! n'afficherait que « DP-2 ».

use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Monitor {
    /// Code du fabricant sur trois lettres (« DEL », « SAM »).
    pub manufacturer: String,
    /// Nom donné par l'écran (« AW2723DF »), s'il en donne un.
    pub name: Option<String>,
    pub serial: Option<String>,
    /// L'écran annonce le HDR (PQ, SMPTE ST 2084) dans son extension CTA-861.
    pub hdr: bool,
}

impl Monitor {
    /// « AW2723DF », sinon le code du fabricant.
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.manufacturer.clone())
    }
}

/// Lit le bloc EDID de base (128 octets).
pub fn parse(edid: &[u8]) -> Option<Monitor> {
    if edid.len() < 128 || edid[0..8] != [0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00] {
        return None;
    }
    // Fabricant : trois lettres de 5 bits (1 = A), sur les octets 8 et 9.
    let id = u16::from_be_bytes([edid[8], edid[9]]);
    let letter = |shift: u16| char::from(b'A' - 1 + ((id >> shift) & 0x1f) as u8);
    let manufacturer: String = [letter(10), letter(5), letter(0)].iter().collect();
    let mut monitor = Monitor { manufacturer, ..Default::default() };
    // Quatre descripteurs de 18 octets : 0xFC nom de l'écran, 0xFF numéro de série.
    for block in edid[54..126].chunks(18) {
        if block[0..3] != [0, 0, 0] {
            continue;
        }
        let text = || {
            let raw = &block[5..18];
            let end = raw.iter().position(|&b| b == 0x0a).unwrap_or(raw.len());
            let text = String::from_utf8_lossy(&raw[..end]).trim().to_string();
            (!text.is_empty()).then_some(text)
        };
        match block[3] {
            0xfc => monitor.name = text(),
            0xff => monitor.serial = text(),
            _ => {}
        }
    }
    monitor.hdr = edid[128..].chunks(128).any(cta_supports_pq);
    Some(monitor)
}

/// Bloc d'extension CTA-861 : bloc de données « HDR Static Metadata »
/// (étiquette étendue 6), dont le premier octet liste les fonctions de
/// transfert reconnues (bit 2 : PQ).
fn cta_supports_pq(block: &[u8]) -> bool {
    if block.len() < 4 || block[0] != 0x02 {
        return false;
    }
    let end = usize::from(block[2]).min(block.len());
    let mut offset = 4;
    while offset < end {
        let header = block[offset];
        let (tag, length) = (header >> 5, usize::from(header & 0x1f));
        let payload = &block[(offset + 1).min(end)..(offset + 1 + length).min(end)];
        if tag == 7 && payload.len() >= 2 && payload[0] == 0x06 && payload[1] & 0b100 != 0 {
            return true;
        }
        offset += 1 + length;
    }
    false
}

/// L'écran branché sur `connector` (« DP-2 »), quelle que soit sa carte.
pub fn for_connector(sys: &Path, connector: &str) -> Option<Monitor> {
    let drm = sys.join("class/drm");
    let entries = fs::read_dir(&drm).ok()?;
    entries.flatten().find_map(|entry| {
        let name = entry.file_name().into_string().ok()?;
        let (card, rest) = name.split_once('-')?;
        if !card.starts_with("card") || rest != connector {
            return None;
        }
        parse(&fs::read(entry.path().join("edid")).ok()?)
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// EDID de base valide pour les tests : fabricant, nom et numéro de série.
    pub(crate) fn sample(manufacturer: &str, name: &str, serial: &str) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[0..8].copy_from_slice(&[0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00]);
        let code: Vec<u16> = manufacturer.bytes().map(|b| (b - b'A' + 1) as u16).collect();
        let id = (code[0] << 10) | (code[1] << 5) | code[2];
        edid[8..10].copy_from_slice(&id.to_be_bytes());
        let put = |edid: &mut Vec<u8>, offset: usize, tag: u8, text: &str| {
            edid[offset + 3] = tag;
            let bytes = text.as_bytes();
            edid[offset + 5..offset + 5 + bytes.len()].copy_from_slice(bytes);
            if bytes.len() < 13 {
                edid[offset + 5 + bytes.len()] = 0x0a;
                for b in &mut edid[offset + 6 + bytes.len()..offset + 18] {
                    *b = 0x20;
                }
            }
        };
        // Premier descripteur : timing détaillé (non nul), ignoré.
        edid[54] = 0x02;
        put(&mut edid, 72, 0xff, serial);
        put(&mut edid, 90, 0xfc, name);
        edid
    }

    #[test]
    fn reads_name_manufacturer_and_serial() {
        let monitor = parse(&sample("DEL", "AW2723DF", "8X1Y2Z3")).unwrap();
        assert_eq!(monitor.manufacturer, "DEL");
        assert_eq!(monitor.name.as_deref(), Some("AW2723DF"));
        assert_eq!(monitor.serial.as_deref(), Some("8X1Y2Z3"));
        assert_eq!(monitor.label(), "AW2723DF");
    }

    #[test]
    fn hdr_comes_from_the_cta_extension() {
        let mut edid = sample("SAM", "Odyssey G7", "H4ZR");
        assert!(!parse(&edid).unwrap().hdr);
        // Extension CTA : un bloc vidéo (étiquette 2), puis HDR Static Metadata
        // (étiquette 7, étendue 6) avec SDR, HDR et PQ (0b0111).
        let mut cta = vec![0u8; 128];
        cta[0] = 0x02;
        cta[1] = 0x03;
        let blocks = [0x42, 0x10, 0x04, 0xe3, 0x06, 0x07, 0x01];
        cta[4..4 + blocks.len()].copy_from_slice(&blocks);
        cta[2] = (4 + blocks.len()) as u8;
        edid.extend_from_slice(&cta);
        assert!(parse(&edid).unwrap().hdr);
        // HLG seul (bit 3) : pas de HDR10.
        let last = edid.len() - 128 + 4 + blocks.len() - 2;
        edid[last] = 0b1000;
        assert!(!parse(&edid).unwrap().hdr);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse(&[0u8; 128]).is_none());
        assert!(parse(&[0u8; 12]).is_none());
    }

    #[test]
    fn finds_the_screen_of_a_connector() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("class/drm/card1-DP-2");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("edid"), sample("SAM", "Odyssey G7", "H4ZR")).unwrap();
        fs::create_dir_all(root.path().join("class/drm/card1-HDMI-A-1")).unwrap();
        assert_eq!(for_connector(root.path(), "DP-2").unwrap().label(), "Odyssey G7");
        assert!(for_connector(root.path(), "HDMI-A-1").is_none());
    }
}
