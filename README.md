# IKUN PX

Independent Tauri/Rust client compatible with MarsLink accounts and official
nodes. It does not bundle, start, locate, or call `MarsLinkProxy.exe`.

## Capabilities

- Logs in through the live control-plane API and protects credentials with
  Windows DPAPI.
- Decrypts node lists and connects directly to official nodes using an
  independently implemented compatible transport.
- Provides local HTTP (`:11888`) and SOCKS5 (`:11889`) proxies.
- Supports fake-IP DNS, domain rules, node latency, traffic counters and
  in-process node switching.
- Separates traffic capture (local ports, Windows system proxy, or TUN) from
  routing policy (smart rules, global proxy, or direct) and applies changes live.
- Diagnoses the live connection on demand: handshake latency to the address the
  tunnel uses, plus page-open time and download rate measured through the local
  proxy.
- Uses MIT-licensed tun2socks v2.6 and Wintun for optional system-wide TUN mode.
- Restores owned system-proxy, DNS and route state on shutdown/recovery.
- Provides a tray menu for proxy environment variables, proxy console,
  Git/SSH integration, rule editing, network diagnostics/repair, hotkeys,
  automatic connection and startup.

## Development

Prerequisites: Node.js, Go, Rust MSVC toolchain, Visual Studio C++ Build Tools,
and WebView2.

```powershell
cd app
npm install
npm run tauri dev
```

Existing login state can be imported once from the legacy ini. Future writes
use `%LOCALAPPDATA%\IKUN PX`; secrets are kept in `credentials.dpapi`.

Core verification:

```powershell
cd app\src-tauri
cargo test --all-targets
cargo run --example verify
cargo run --example protocol_probe
cargo run --example proxy_probe
cargo build --example tun_probe
```

`protocol_probe` connects to an official node without using a local vendor
process. `proxy_probe` starts independent HTTP/SOCKS5 services on development
ports 21888/21889 and verifies their exit IP plus fake-DNS reverse mapping.
`tun_probe` is an elevated, isolated `/32` route test that verifies Wintun
without replacing an already-running VPN session.

## Packaging

```powershell
cd app
npm run tauri build
```

The NSIS installer contains only IKUN PX, tun2socks, Wintun and their license
notices. The build fails if a vendor engine executable appears in resources.

## Architecture

```text
React UI ──Tauri IPC──> Rust runtime
                           ├─ control API + node-list crypto
                           ├─ DPAPI credentials + independent config
                           ├─ MarsLink-compatible Shadowsocks transport
                           ├─ native HTTP/SOCKS5 + fake DNS
                           └─ tun2socks/Wintun + route recovery
                                      │
                                      ▼
                              Official MarsLink node
```

`captures/`, decrypted payloads and WebView diagnostics are gitignored because
development captures may contain account credentials. No vendor executable is
redistributed.
