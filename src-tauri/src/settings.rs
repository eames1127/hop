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
	/// Connections older than this many days are deleted; 0 keeps them forever.
	pub history_retention_days: u32,
	/// Off stops Hop contacting hosts until asked, for networks where unexpected connection
	/// attempts are unwelcome.
	pub auto_status_checks: bool,
}

/// A fixed list, so the window cannot choose an arbitrary period.
pub const RETENTION_CHOICES: &[u32] = &[0, 30, 90, 365];
const SECONDS_PER_DAY: i64 = 86_400;

impl Default for Settings {
	fn default() -> Settings {
		Settings {
			terminal: "auto".to_string(),
			theme: "system".to_string(),
			start_with_computer: false,
			stale_days: 90,
			history_retention_days: 90,
			auto_status_checks: true,
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

		if !RETENTION_CHOICES.contains(&self.history_retention_days) {
			return Err("Keep history must be forever, 30, 90 or 365 days".to_string());
		}

		Ok(())
	}

	/// The start time before which history is pruned, or none when it is kept forever.
	pub fn retention_cutoff(&self, now: i64) -> Option<i64> {
		if self.history_retention_days == 0 {
			None
		}
		else {
			Some(now - i64::from(self.history_retention_days) * SECONDS_PER_DAY)
		}
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

		let odd_retention = Settings {
			history_retention_days: 7,
			..Settings::default()
		};
		assert!(odd_retention.validate().is_err());
	}

	#[test]
	fn loads_files_written_before_the_privacy_settings() {
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("settings.json");
		let old = r#"{"terminal":"auto","theme":"dark","startWithComputer":true,"staleDays":30}"#;
		fs::write(&path, old).unwrap();

		let settings = Settings::load(&path).unwrap();
		assert_eq!(settings.theme, "dark");
		assert_eq!(settings.stale_days, 30);
		assert_eq!(settings.history_retention_days, 90);
		assert!(settings.auto_status_checks);
		assert!(settings.validate().is_ok());
	}

	#[test]
	fn retention_cutoff_follows_the_setting() {
		let forever = Settings {
			history_retention_days: 0,
			..Settings::default()
		};
		assert_eq!(forever.retention_cutoff(1_000_000), None);

		let month = Settings {
			history_retention_days: 30,
			..Settings::default()
		};
		assert_eq!(
			month.retention_cutoff(3_000_000),
			Some(3_000_000 - 30 * 86_400)
		);
	}
}
