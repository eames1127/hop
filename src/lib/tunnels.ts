// Helpers for showing background tunnels next to the forwards they came from.
import type { Forward, TunnelView } from "./types";

export function findTunnel(
	tunnels: TunnelView[],
	host: string,
	forward: number,
): TunnelView | null {
	return tunnels.find((tunnel) => tunnel.host === host && tunnel.forward === forward) ?? null;
}

// A failed tunnel is started again, since the usual fix (freeing the port, loading a key)
// happens outside Hop.
export function tunnelAction(tunnel: TunnelView | null): "start" | "stop" {
	return tunnel?.state === "running" ? "stop" : "start";
}

export function forwardName(forward: Forward): string {
	return forward.label || `Forward ${forward.localPort}`;
}

export function tunnelTitle(forward: Forward, tunnel: TunnelView | null): string {
	const route = `localhost:${forward.localPort} → ${forward.remoteHost}:${forward.remotePort}`;
	if (tunnel?.state === "running") {
		return `Tunnel open: ${route}. Click to stop.`;
	}
	else if (tunnel?.state === "failed") {
		return `Tunnel failed: ${tunnel.error ?? "unknown error"} Click to try again.`;
	}

	return `Open a background tunnel: ${route}`;
}
