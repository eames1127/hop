//! Connection history in a small SQLite file: host name, start time and session length only.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
	pub host: String,
	/// Unix seconds.
	pub started_at: i64,
	/// Always empty for now: the terminal runs on its own, so Hop cannot see when `ssh` ends.
	pub duration_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
	pub last_used: i64,
	pub count: i64,
}

pub struct History {
	conn: Connection,
}

impl History {
	pub fn open(path: &Path) -> rusqlite::Result<History> {
		History::init(Connection::open(path)?)
	}

	#[cfg(test)]
	pub fn in_memory() -> rusqlite::Result<History> {
		History::init(Connection::open_in_memory()?)
	}

	fn init(conn: Connection) -> rusqlite::Result<History> {
		conn.execute_batch(
			"CREATE TABLE IF NOT EXISTS connections (
				id INTEGER PRIMARY KEY,
				host TEXT NOT NULL,
				started_at INTEGER NOT NULL,
				duration_secs INTEGER
			);
			CREATE INDEX IF NOT EXISTS connections_host ON connections (host, started_at);",
		)?;
		Ok(History { conn })
	}

	pub fn record(&self, host: &str, started_at: i64) -> rusqlite::Result<()> {
		self.conn.execute(
			"INSERT INTO connections (host, started_at) VALUES (?1, ?2)",
			params![host, started_at],
		)?;
		Ok(())
	}

	pub fn usage(&self) -> rusqlite::Result<HashMap<String, Usage>> {
		let mut statement = self
			.conn
			.prepare("SELECT host, MAX(started_at), COUNT(*) FROM connections GROUP BY host")?;
		let rows = statement.query_map([], |row| {
			let usage = Usage {
				last_used: row.get(1)?,
				count: row.get(2)?,
			};
			Ok((row.get::<_, String>(0)?, usage))
		})?;
		rows.collect()
	}

	/// Hosts by most recent connection, newest first.
	pub fn recent_hosts(&self, limit: usize) -> rusqlite::Result<Vec<String>> {
		let mut statement = self.conn.prepare(
			"SELECT host FROM connections GROUP BY host ORDER BY MAX(started_at) DESC LIMIT ?1",
		)?;
		let limit = i64::try_from(limit).unwrap_or(i64::MAX);
		let rows = statement.query_map([limit], |row| row.get(0))?;
		rows.collect()
	}

	pub fn entries(&self, host: Option<&str>, since: i64) -> rusqlite::Result<Vec<HistoryEntry>> {
		let mut statement = self.conn.prepare(
			"SELECT host, started_at, duration_secs FROM connections
			WHERE started_at >= ?1 AND (?2 IS NULL OR host = ?2)
			ORDER BY started_at DESC",
		)?;
		let rows = statement.query_map(params![since, host], |row| {
			Ok(HistoryEntry {
				host: row.get(0)?,
				started_at: row.get(1)?,
				duration_secs: row.get(2)?,
			})
		})?;
		rows.collect()
	}

	pub fn rename_host(&self, old: &str, new: &str) -> rusqlite::Result<()> {
		self.conn.execute(
			"UPDATE connections SET host = ?2 WHERE host = ?1",
			params![old, new],
		)?;
		Ok(())
	}

	/// Deletes connections that started before `cutoff`, returning how many went.
	pub fn prune(&self, cutoff: i64) -> rusqlite::Result<usize> {
		self.conn
			.execute("DELETE FROM connections WHERE started_at < ?1", [cutoff])
	}

	pub fn clear(&self) -> rusqlite::Result<()> {
		self.conn.execute("DELETE FROM connections", [])?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn records_and_summarises() {
		let history = History::in_memory().unwrap();
		history.record("nas", 100).unwrap();
		history.record("pi", 200).unwrap();
		history.record("nas", 300).unwrap();

		let usage = history.usage().unwrap();
		assert_eq!(
			usage["nas"],
			Usage {
				last_used: 300,
				count: 2
			}
		);
		assert_eq!(history.recent_hosts(5).unwrap(), vec!["nas", "pi"]);
		assert_eq!(history.entries(Some("pi"), 0).unwrap().len(), 1);
		assert_eq!(history.entries(None, 250).unwrap().len(), 1);

		history.rename_host("nas", "store").unwrap();
		assert!(history.usage().unwrap().contains_key("store"));
		history.clear().unwrap();
		assert!(history.usage().unwrap().is_empty());
	}

	#[test]
	fn prunes_only_older_connections() {
		let history = History::in_memory().unwrap();
		history.record("nas", 100).unwrap();
		history.record("nas", 200).unwrap();
		history.record("pi", 300).unwrap();

		assert_eq!(history.prune(200).unwrap(), 1);
		assert_eq!(history.entries(None, 0).unwrap().len(), 2);
		assert_eq!(history.prune(200).unwrap(), 0);
	}
}
