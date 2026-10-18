# ferrite-agent

Reports an embedded Linux device to ferrite-server: image identity, system metrics, reboot reasons and the health of chosen systemd units (for example the CODESYS Control runtime). It speaks the same chunk format as the MCU SDK, so Linux devices appear in the device list, metrics, OTA campaigns and update-reach reports.

```bash
cargo build -p ferrite-agent --release
sudo install -m 0755 target/release/ferrite-agent /usr/bin/
sudo install -D -m 0644 ferrite-agent/dist/agent.conf.example /etc/ferrite/agent.conf   # then edit
sudo install -m 0644 ferrite-agent/dist/ferrite-agent.service /etc/systemd/system/
sudo systemctl enable --now ferrite-agent
ferrite-agent identity     # what the server will see
```

See [docs/guide/linux-agent.md](../docs/guide/linux-agent.md).
