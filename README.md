# Sonar

Lightweight monitoring tool for monitoring system health of the super fish NAS. When an alert fires, a Discord message will be sent, notifying the admin when Severity == Critical.

## Roadmap

- [x] Configuration file (`monitors.toml`)
- [x] Nix Package/NixOS module
- [x] CPU
  - [x] Temperature
  - [x] Usage
- [x] Disk
  - [x] Usage
  - [x] Stats/Health
  - [x] Scrub interval
- [ ] Memory
  - [x] Usage
  - [ ] OOM kills
- [ ] Network
  - [ ] Connect to self via domain
  - [ ] Connect to third party via domain
- [x] Systemd service status
- [ ] UPS battery level
- [x] Nixpkgs last update

## Usage

```bash
cargo run -- --discord-webhook-url <DISCORD_WEBHOOK_URL>
```

For development, you may provide a `.env` file (see `.env.sample`).
