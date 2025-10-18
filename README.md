# Sonar

Tiny monitoring tool written in Rust for monitoring CPU temperature, CPU usage, disk usage, disk health/stats, and memory usage. When an alert fires, a Discord webhook will be fired.

## Roadmap

- [ ] Configuration file
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
- [ ] Network (just to log)
- [ ] Systemd service (primarily for backups)
- [ ] UPS battery level
- [ ] Nixpkgs last update

## Usage

```bash
cargo run -- --discord-webhook-url <DISCORD_WEBHOOK_URL>
```

For development, you may provide a `.env` file (see `.env.sample`).
