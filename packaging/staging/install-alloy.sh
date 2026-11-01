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

echo "== Alloy, from Arch's own repository"
# Arch's build rather than a derivative's: on 2026-11-01 CachyOS's mirrors
# had moved past the build its package list named, and two answered 404 --
# which pacman then rightly refused as a corrupt package.
if ! pacman -Q grafana-alloy >/dev/null 2>&1; then
    pacman -S --noconfirm extra/grafana-alloy
fi

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
alloy fmt /etc/alloy/config.alloy >/dev/null

# The token file, whatever the packaged unit reads; and the journal, which
# Alloy reads only as a member of systemd-journal.
install -d /etc/systemd/system/alloy.service.d
cat > /etc/systemd/system/alloy.service.d/qurb.conf <<'EOF'
[Service]
EnvironmentFile=/etc/alloy/grafana-cloud.env
EOF
usermod -aG systemd-journal alloy

echo "== Starting it"
systemctl daemon-reload
systemctl enable alloy.service >/dev/null
systemctl restart alloy.service
sleep 5
systemctl --no-pager --lines=8 status alloy.service
