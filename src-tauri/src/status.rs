//! Online checks: a plain TCP connect, in Rust, to the address `ssh -G` reports.

use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

const TIMEOUT: Duration = Duration::from_secs(3);
const PARALLEL_CHECKS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reachability {
	Online,
	Offline,
	/// Behind a jump host, so a direct check would say nothing useful.
	Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStatus {
	pub name: String,
	pub state: Reachability,
	pub detail: Option<String>,
	pub latency_ms: Option<u64>,
}

pub fn check_many(ssh: &Path, names: &[String]) -> Vec<HostStatus> {
	let mut results = Vec::with_capacity(names.len());
	for chunk in names.chunks(PARALLEL_CHECKS) {
		thread::scope(|scope| {
			let handles: Vec<_> = chunk
				.iter()
				.map(|name| scope.spawn(move || check(ssh, name)))
				.collect();
			for (handle, name) in handles.into_iter().zip(chunk) {
				results.push(handle.join().unwrap_or_else(|_| HostStatus {
					name: name.clone(),
					state: Reachability::Unknown,
					detail: Some("The check stopped unexpectedly".to_string()),
					latency_ms: None,
				}));
			}
		});
	}

	results
}

pub fn check(ssh: &Path, name: &str) -> HostStatus {
	let status = |state, detail: String| HostStatus {
		name: name.to_string(),
		state,
		detail: Some(detail),
		latency_ms: None,
	};

	let resolved = match sshconf::resolve(ssh, name) {
		Ok(resolved) => resolved,
		Err(error) => return status(Reachability::Unknown, error.to_string()),
	};

	if let Some(jump) = resolved.proxy_jump {
		return status(Reachability::Unknown, format!("Reached through {jump}"));
	}

	let addresses = match (resolved.host_name.as_str(), resolved.port).to_socket_addrs() {
		Ok(addresses) => addresses,
		Err(error) => return status(Reachability::Offline, format!("Address not found: {error}")),
	};

	let mut last_error = "No address to try".to_string();
	for address in addresses {
		let started = Instant::now();
		match TcpStream::connect_timeout(&address, TIMEOUT) {
			Ok(_) => {
				let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
				return HostStatus {
					name: name.to_string(),
					state: Reachability::Online,
					detail: None,
					latency_ms: Some(elapsed),
				};
			}
			Err(error) => last_error = error.to_string(),
		}
	}

	status(Reachability::Offline, last_error)
}
