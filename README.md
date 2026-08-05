# Aka

A local development DNS and reverse proxy manager for Docker. Aka lets you access your Docker containers via custom `.docker` domains (or any domain you configure) instead of `localhost:PORT` — no manual `/etc/hosts` editing required.

Aka is a Rust reimplementation of [Dory](https://github.com/FreedomBen/dory). If you are migrating from Dory, rename your `.dory.yml` to `.aka.yml` — the config format is otherwise identical.

**Not supported: [Dinghy](https://github.com/codekitchen/dinghy).** Dory let you set `nameserver`/`address` config values to the literal string `dinghy`, which it would substitute at runtime with the IP of a running Dinghy VM (`dinghy ip`). Dinghy itself has been unmaintained since Docker Desktop for Mac made its VirtualBox-based Docker host unnecessary, so aka does not implement this substitution — a literal `dinghy` value in `nameserver`/`address` is used as-is rather than resolved to a VM IP.

---

## How it works

Aka runs two Docker containers on your machine:

- **dnsmasq** — a lightweight DNS server that resolves your custom domains (e.g. `myapp.docker`) to `127.0.0.1`
- **nginx-proxy** — an Nginx reverse proxy that routes incoming HTTP/HTTPS requests to the correct container based on the `VIRTUAL_HOST` environment variable

It also configures your OS resolver so that DNS queries for your custom domains are sent to the dnsmasq container:

- **macOS**: writes files to `/etc/resolver/[domain]`
- **Linux**: prepends a nameserver entry to `/etc/resolv.conf` (or uses `resolvconf` on Ubuntu)

---

## Prerequisites

- [Rust](https://rustup.rs) (to build from source)
- [Docker](https://docs.docker.com/get-docker/)

---

## Installation

```bash
git clone <repo>
cd aka
cargo install --path .
```

---

## Quick start

**1. Write a default config file:**

```bash
aka config-file
```

This creates `~/.aka.yml` with sensible defaults. Edit it to customize domains, ports, or image names.

**2. Start all services:**

```bash
aka up
```

This starts the dnsmasq and nginx-proxy containers and configures your system resolver. Some steps require `sudo` (lsof port checks and writing resolver files).

**3. Launch your Docker container with a `VIRTUAL_HOST`:**

```bash
docker run -e VIRTUAL_HOST=myapp.docker myimage
```

Your app is now accessible at `http://myapp.docker`.

**4. Stop everything:**

```bash
aka down
```

---

## Commands

| Command | Description |
|---|---|
| `aka up [proxy\|dns\|resolv]` | Start all services, or specific ones |
| `aka down [proxy\|dns\|resolv]` | Stop and remove all services, or specific ones |
| `aka restart` | Stop then start all services |
| `aka status` | Show running state of each container and DNS resolver configuration |
| `aka config-file` | Write default `~/.aka.yml` |
| `aka config-file --upgrade` | Migrate existing config to the latest format |
| `aka config-file --force` | Overwrite existing config with defaults |
| `aka pull [proxy\|dns]` | Pull the latest Docker images |
| `aka attach [proxy\|dns]` | Attach to a container's output stream |
| `aka logs [proxy\|dns]` | Print a container's logs |
| `aka ip [proxy\|dns]` | Print the IPv4 address of a container |
| `aka upgrade` | Check for a newer version of aka |

Pass `--verbose` to any command to see debug-level output:

```bash
aka --verbose up
```

`aka status --verbose` additionally prints the raw contents of the resolver file(s) being managed (`/etc/resolv.conf` on Linux, each `/etc/resolver/<domain>` file on macOS).

---

## Configuration

Aka loads config from three sources in order, with each layer overriding the previous:

1. Built-in defaults
2. `~/.aka.yml` (user-level settings)
3. `.aka.yml` in the current directory or any parent (project-level overrides)

### Example `~/.aka.yml`

```yaml
aka:
  dnsmasq:
    enabled: true
    domains:
      - domain: docker       # resolves *.docker → 127.0.0.1
        address: 127.0.0.1
    container_name: aka_dnsmasq
    port: 53
    kill_others: ask         # ask | yes | no
    service_start_delay: 5

  nginx_proxy:
    enabled: true
    container_name: aka_http_proxy
    https_enabled: true
    ssl_certs_dir: ''        # leave empty to use built-in certs
    port: 80
    tls_port: 443

  resolv:
    enabled: true
    nameserver: 127.0.0.1
    port: 53

  debug: false  # set to true for the same effect as always passing --verbose
```

### Multiple domains

You can resolve multiple TLDs to different addresses:

```yaml
aka:
  dnsmasq:
    domains:
      - domain: docker
        address: 127.0.0.1
      - domain: local
        address: 127.0.0.1
```

### `kill_others`

Controls what happens when another process is already using the dnsmasq port:

- `ask` — prompt before killing (default)
- `yes` / `true` — kill automatically
- `no` / `false` — fail without killing

### `service_start_delay`

When aka stops or restarts a conflicting systemd service (see below), it polls the service's actual state rather than sleeping blindly. `service_start_delay` is the maximum number of seconds to wait for that state change to be confirmed — aka returns as soon as the service is actually stopped/running, and only waits the full duration if the service is slow to respond.

### `debug`

Setting `debug: true` forces debug-level logging on every run — the same effect as always passing `--verbose` (see [Commands](#commands) above), without having to type the flag each time. Defaults to `false`.

### macOS port note

On macOS, if `dnsmasq.port` is set to `53` (the Linux default), aka automatically uses port `19323` instead to avoid conflicting with the macOS system resolver. The `resolv.port` value is still written to the `/etc/resolver/` files so macOS knows where to send DNS queries.

---

## How nginx-proxy routing works

nginx-proxy watches the Docker socket for new containers. When a container starts with a `VIRTUAL_HOST` environment variable, nginx-proxy automatically creates a routing rule for that hostname.

```bash
docker run -d \
  -e VIRTUAL_HOST=myapp.docker \
  -e VIRTUAL_PORT=3000 \
  myimage
```

For HTTPS, also set:

```bash
  -e VIRTUAL_HOST=myapp.docker \
  -e CERT_NAME=myapp.docker \
```

Place your certs in `ssl_certs_dir` (or leave it empty to use the built-in self-signed cert).

---

## Troubleshooting

**Containers won't start — port conflict**

If port 53 is in use (common on Ubuntu where `systemd-resolved` owns it), aka will detect and temporarily stop `systemd-resolved` or `NetworkManager` while dnsmasq starts, then restart them, waiting for each state change to be confirmed (see `service_start_delay` above) rather than a fixed delay. Set `kill_others: yes` to do this automatically.

**DNS not resolving after `aka up`**

- Run `aka status` to confirm the dnsmasq container is running.
- On macOS, check that `/etc/resolver/docker` exists and contains `nameserver 127.0.0.1`.
- On Linux, check that `/etc/resolv.conf` has a `nameserver 127.0.0.1` line near the top.
- Try `aka down` then `aka up` to reconfigure the resolver.

**`aka up` requires sudo**

Some operations require elevated privileges:
- `lsof` port conflict detection
- Writing to `/etc/resolver/` (macOS) or `/etc/resolv.conf` (Linux)
- Stopping/restarting conflicting systemd services (`systemd-resolved`, `NetworkManager`) on Linux

You will be prompted for your password when needed.

---

## Building from source

```bash
cargo build --release
./target/release/aka --help
```

Run the test suite:

```bash
cargo test
```
