use std::fs;
use std::path::Path;

#[test]
fn gui_source_does_not_reference_osc_sending_apis() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let forbidden = ["sender::", "OscSender", "UdpSocket", "rosc"];
    let mut violations = Vec::new();
    scan(&src, &forbidden, &mut violations);
    assert!(
        violations.is_empty(),
        "GUI source contains direct OSC sender references: {violations:?}"
    );
}

fn scan(path: &Path, forbidden: &[&str], violations: &mut Vec<String>) {
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            scan(&path, forbidden, violations);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = fs::read_to_string(&path).unwrap();
            // This pure address formatter lives beside the sender today; it does not send.
            let source = source.replace("qtable_engine::sender::resolve_address", "");
            for needle in forbidden {
                if source.contains(needle) {
                    violations.push(format!("{} contains {needle:?}", path.display()));
                }
            }
        }
    }
}
