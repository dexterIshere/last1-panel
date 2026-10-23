// Notes de mise à jour (tools/release-notes.sh) embarquées dans les
// exécutables : release-notes.tsv à la racine du dépôt, posé par la
// construction des versions ; vide sinon (construction locale).
use std::path::Path;

fn main() {
    let notes = Path::new(env!("CARGO_MANIFEST_DIR")).join("../release-notes.tsv");
    println!("cargo:rerun-if-changed={}", notes.display());
    let text = std::fs::read_to_string(&notes).unwrap_or_default();
    let out = Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("release-notes.tsv");
    std::fs::write(out, text).expect("notes écrites");
}
