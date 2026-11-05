//! Vanilla's RSA decryption (`tests/fixtures/AuthDecrypt.java`, run through `scripts/vanilla-java.sh`) accepts our ciphertext.

use std::path::PathBuf;
use std::process::Command;

use mc_protocol::auth::encrypt_with_public_key;

const GOLDEN: &str = include_str!("../fixtures/auth_golden.txt");

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|b| format!("{b:02x}")).collect()
}

fn golden(key: &str) -> Vec<u8> {
    let line = GOLDEN.lines().find_map(|l| l.strip_prefix(&format!("{key}="))).expect("fixture key");
    hex(line)
}

#[test]
#[ignore = "oracle: runs scripts/vanilla-java.sh"]
fn vanilla_decrypts_rust_ciphertext() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/vanilla-java.sh");
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/AuthDecrypt.java");
    let key = golden("public_der");
    for plain in [&golden("secret")[..], &[7u8; 117], &[]] {
        let ct = encrypt_with_public_key(&key, plain).unwrap();
        let out = Command::new(&script)
            .arg(&fixture)
            .arg(to_hex(&golden("private_pkcs8")))
            .arg(to_hex(&ct))
            .output()
            .expect("run scripts/vanilla-java.sh (needs ref/ from scripts/setup-ref.sh and java)");
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), to_hex(plain));
    }
    // Too long for PKCS#1 v1.5 with a 1024-bit key, like Java's "Data must not be longer than 117 bytes".
    assert!(encrypt_with_public_key(&key, &[0; 118]).is_err());
}
