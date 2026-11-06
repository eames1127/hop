use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::platform::is_known_terminal;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
	/// One of the ids in `platform::TERMINALS`.
	pub terminal: String,
	/// `system`, `light` or `dark`.
	pub theme: String,
	/// Off by default: Hop should only start with the computer when asked to.
	pub start_with_computer: bool,
	/// A host unused for this many days is flagged as stale.
	pub stale_days: u32,
}

impl Default for Settings {
	fn default() -> Settings {
		Settings {
			terminal: "auto".to_string(),
			theme: "system".to_string(),
			start_with_computer: false,
			stale_days: 90,
		}
	}
}

impl Settings {
	pub fn validate(&self) -> Result<(), String> {
		if !is_known_terminal(&self.terminal) {
			return Err(format!("Unknown terminal \"{}\"", self.terminal));
		}

		if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
			return Err(format!("Unknown theme \"{}\"", self.theme));
		}

		if !(1..=3650).contains(&self.stale_days) {
			return Err("Stale after must be between 1 and 3650 days".to_string());
		}

		Ok(())
	}

	/// A missing file gives the defaults; a damaged one is reported rather than overwritten.
	pub fn load(path: &Path) -> io::Result<Settings> {
		match fs::read_to_string(path) {
			Ok(text) => serde_json::from_str(&text)
				.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
			Err(error) => Err(error),
		}
	}

	pub fn save(&self, path: &Path) -> io::Result<()> {
		if let Some(dir) = path.parent() {
			fs::create_dir_all(dir)?;
		}

		let text = serde_json::to_string_pretty(self)
			.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
		fs::write(path, text)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn round_trips_and_validates() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("settings.json");
		assert_eq!(Settings::load(&path).unwrap(), Settings::default());

		let settings = Settings {
			theme: "dark".to_string(),
			..Settings::default()
		};
		settings.save(&path).unwrap();
		assert_eq!(Settings::load(&path).unwrap(), settings);

		let bad = Settings {
			terminal: "sh -c x".to_string(),
			..Settings::default()
		};
		assert!(bad.validate().is_err());
	}
}
