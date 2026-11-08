//! Opens WebView2's remote debugging port for the end-to-end smoke test (`pnpm test:e2e`).
//!
//! Some WebView2 setups (GitHub's Windows runners among them) ignore the
//! `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` override, so the test asks the app itself through
//! `HOP_E2E_DEBUG_PORT`. The module is only compiled into debug builds: a release build has no
//! way to open a debugging port, whatever its environment says.

use tauri::{Context, Runtime};

const PORT_VARIABLE: &str = "HOP_E2E_DEBUG_PORT";
/// Wry's defaults, which it drops once a window sets its own arguments.
const DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Adds `--remote-debugging-port` to every configured window when `HOP_E2E_DEBUG_PORT` holds a
/// valid port. Anything else is reported and ignored, so a typo cannot open an unexpected port.
pub fn apply<R: Runtime>(mut context: Context<R>) -> Context<R> {
	let Some(value) = std::env::var_os(PORT_VARIABLE)
	else {
		return context;
	};
	let Some(port) = value.to_str().and_then(parse_port)
	else {
		eprintln!("hop: ignoring {PORT_VARIABLE}={value:?}; expected a port from 1 to 65535");
		return context;
	};
	let args = format!("{DEFAULT_ARGS} --remote-debugging-port={port}");
	for window in &mut context.config_mut().app.windows {
		window.additional_browser_args = Some(args.clone());
	}
	context
}

fn parse_port(text: &str) -> Option<u16> {
	// u16's parser accepts a leading "+", which a port number should not have.
	if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
		return None;
	}
	text.parse::<u16>().ok().filter(|port| *port != 0)
}

#[cfg(test)]
mod tests {
	use super::parse_port;

	#[test]
	fn accepts_plain_ports() {
		assert_eq!(parse_port("9333"), Some(9333));
		assert_eq!(parse_port("65535"), Some(65535));
	}

	#[test]
	fn rejects_anything_else() {
		for text in [
			"", "0", "65536", "+9333", "-1", " 9333", "9333 ", "93a3", "9333 --x",
		] {
			assert_eq!(parse_port(text), None, "{text:?}");
		}
	}
}
