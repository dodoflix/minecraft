//! `net.minecraft.server.Eula` (`eula.txt`).

use std::fs;
use std::path::{Path, PathBuf};

use super::java_properties::Properties;
use super::settings::java_boolean;

/// `CommonLinks.EULA`.
pub const EULA_LINK: &str = "https://aka.ms/MinecraftEULA";

pub struct Eula {
    file: PathBuf,
    agreed: bool,
}

impl Eula {
    /// `new Eula(file)`: reads the file; when it cannot be read it is (re)created with `eula=false`.
    pub fn new(file: impl Into<PathBuf>) -> Eula {
        let file = file.into();
        let agreed = Self::read_file(&file);
        Eula { file, agreed }
    }

    fn read_file(file: &Path) -> bool {
        let loaded = fs::read(file).ok().and_then(|bytes| Properties::load_latin1(&bytes).ok());
        match loaded {
            Some(properties) => java_boolean(properties.get_property("eula", "false")),
            None => {
                log::warn!("Failed to load {}", file.display());
                Self::save_defaults(file);
                false
            }
        }
    }

    pub fn has_agreed_to_eula(&self) -> bool {
        self.agreed
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    fn save_defaults(file: &Path) {
        let mut properties = Properties::new();
        properties.put("eula", "false");
        let comments = format!("By changing the setting below to TRUE you are indicating your agreement to our EULA ({EULA_LINK}).");
        if let Err(e) = fs::write(file, properties.store_latin1(Some(&comments))) {
            log::warn!("Failed to save {}\n{e}", file.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_default_and_reads_agreement() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("eula.txt");
        assert!(!Eula::new(&file).has_agreed_to_eula());
        let text = fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "#By changing the setting below to TRUE you are indicating your agreement to our EULA (https://aka.ms/MinecraftEULA).");
        assert!(lines[1].starts_with('#'));
        assert_eq!(lines[2], "eula=false");
        assert_eq!(lines.len(), 3);
        fs::write(&file, "#x\neula=TRUE\n").unwrap();
        assert!(Eula::new(&file).has_agreed_to_eula());
        fs::write(&file, "eula=yes\n").unwrap();
        assert!(!Eula::new(&file).has_agreed_to_eula());
    }
}
