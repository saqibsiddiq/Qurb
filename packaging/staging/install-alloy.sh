#!/usr/bin/env bash
# Install Grafana Alloy on the staging host -- the owner's laptop for now --
# sending its traffic, CPU and memory and the rendezvous service's log to
# Grafana Cloud (docs/staging.md). Run once, with sudo:
#
#   sudo packaging/staging/install-alloy.sh
#
# It asks for the Grafana Cloud stack's values (Connections -> Add new
# connection -> Linux server shows them, or the stack's details page) and
# keeps them in /etc/alloy/grafana-cloud.env, readable by root and Alloy
# only. Run it again to change them. Arch-family systems (pacman).
set -euo pipefail
cd "$(dirname "$0")"
[[ $EUID -eq 0 ]] || { echo "run with sudo" >&2; exit 1; }
ALLOY=v1.21.3

echo "== Alloy $ALLOY, Grafana's own release, checked against its published SHA-256"
# Grafana's release rather than a distribution's package: on 2026-11-01 every
# Arch and CachyOS mirror answered 404 for the build the laptop's package
# list named -- the list was behind -- and a whole-system update to get one
# tool is the owner's call, not this script's.
if [[ ! -x /usr/local/bin/alloy ]] || ! /usr/local/bin/alloy --version 2>/dev/null | grep -q "${ALLOY#v}"; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    base=https://github.com/grafana/alloy/releases/download/$ALLOY
    curl -fsSL -o "$tmp/alloy-linux-amd64.zip" "$base/alloy-linux-amd64.zip"
    curl -fsSL -o "$tmp/SHA256SUMS" "$base/SHA256SUMS"
    (cd "$tmp" && grep ' alloy-linux-amd64.zip$' SHA256SUMS | sha256sum -c -)
    bsdtar -xOf "$tmp/alloy-linux-amd64.zip" alloy-linux-amd64 > "$tmp/alloy"
    install -m 0755 "$tmp/alloy" /usr/local/bin/alloy
fi
/usr/local/bin/alloy --version | head -1

# Its own user, with nothing but its data, and the journal to read.
id alloy >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin alloy
usermod -aG systemd-journal alloy
install -d -o alloy -g alloy -m 0750 /var/lib/alloy

echo "== The Grafana Cloud stack's values (nothing is shown as the token is typed)"
read -rp "Prometheus remote-write URL (…/api/prom/push): " PROM_URL
read -rp "Prometheus user (a number): " PROM_USER
read -rp "Loki URL (…/loki/api/v1/push): " LOKI_URL
read -rp "Loki user (a number): " LOKI_USER
read -rsp "Access token (glc_…): " TOKEN; echo
install -d -m 0755 /etc/alloy
umask 027
cat > /etc/alloy/grafana-cloud.env <<EOF
GCLOUD_PROMETHEUS_URL=$PROM_URL
GCLOUD_PROMETHEUS_USER=$PROM_USER
GCLOUD_LOKI_URL=$LOKI_URL
GCLOUD_LOKI_USER=$LOKI_USER
GCLOUD_TOKEN=$TOKEN
EOF
chown root:alloy /etc/alloy/grafana-cloud.env
chmod 0640 /etc/alloy/grafana-cloud.env
umask 022

echo "== The configuration"
install -m 0644 config.alloy /etc/alloy/config.alloy
/usr/local/bin/alloy fmt /etc/alloy/config.alloy >/dev/null

cat > /etc/systemd/system/alloy.service <<'EOF'
[Unit]
Description=Grafana Alloy: the staging host's traffic and the rendezvous log, to Grafana Cloud
Wants=network-online.target
After=network-online.target

[Service]
User=alloy
Group=alloy
EnvironmentFile=/etc/alloy/grafana-cloud.env
ExecStart=/usr/local/bin/alloy run /etc/alloy/config.alloy --storage.path=/var/lib/alloy --server.http.listen-addr=127.0.0.1:12345
Restart=on-failure
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/var/lib/alloy
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
EOF

echo "== Starting it"
systemctl daemon-reload
systemctl enable alloy.service >/dev/null
systemctl restart alloy.service
sleep 5
systemctl --no-pager --lines=8 status alloy.service
