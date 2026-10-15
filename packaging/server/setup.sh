#!/usr/bin/env bash
# Set up the rendezvous service and the relay on a server of your own.
#
# Run as root on the server, from a directory holding this file and the unit
# files beside it -- which is what deploy.sh arranges from your computer:
#
#   sudo bash setup.sh --binary ./qurb            # a qurb built elsewhere
#   sudo bash setup.sh --source ./qurb-src.tar    # build it here from source
#   ... [--address 203.0.113.5]                   # if it cannot be found out
#
# For a server with an address and no domain name: the rendezvous service
# presents its own certificate and devices pin its fingerprint (decision 0035).
# Written for Ubuntu, as Oracle's and most small servers' images are; it says
# so and stops on anything without apt and systemd.
#
# What it does not do: open the cloud provider's own firewall -- a firewall
# rule on Google Cloud, the subnet's security list on Oracle Cloud, both in the
# web console. This prints how.
set -euo pipefail
cd "$(dirname "$0")"

BINARY="" SOURCE="" ADDRESS=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --binary)  BINARY=$2; shift 2 ;;
        --source)  SOURCE=$2; shift 2 ;;
        --address) ADDRESS=$2; shift 2 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
done

say()  { printf '\n== %s\n' "$*"; }
fail() { echo "setup: $*" >&2; exit 1; }

[[ $EUID -eq 0 ]] || fail "run as root: sudo bash $0 ..."
command -v apt-get >/dev/null && command -v systemctl >/dev/null \
    || fail "this expects Ubuntu or Debian with systemd"
[[ -n $BINARY || -n $SOURCE || -x /usr/local/bin/qurb ]] \
    || fail "give --binary or --source; there is no qurb installed yet"

# -- qurb itself ----------------------------------------------------------------

if [[ -n $BINARY ]]; then
    say "installing the qurb that was sent"
    install -m755 "$BINARY" /usr/local/bin/qurb
elif [[ -n $SOURCE ]]; then
    say "building qurb here (a few minutes on two cores; much longer on less)"
    apt-get update -qq
    apt-get install -y -qq build-essential pkg-config curl ca-certificates >/dev/null

    # Rust needs more memory to build than the smallest free servers have.
    # A temporary swap file, removed afterwards.
    swap=""
    if [[ $(awk '/MemTotal/ {print $2}' /proc/meminfo) -lt 3000000 ]] && ! swapon --show | grep -q .; then
        swap=/qurb-build.swap
        fallocate -l 3G "$swap" && chmod 600 "$swap" && mkswap -q "$swap" && swapon "$swap"
    fi

    work=$(mktemp -d)
    tar -xf "$SOURCE" -C "$work"
    if ! command -v cargo >/dev/null; then
        curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal >/dev/null
    fi
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
    (cd "$work" && cargo build --release -p qurb-cli)
    install -m755 "$work/target/release/qurb" /usr/local/bin/qurb
    strip /usr/local/bin/qurb || true
    rm -rf "$work"
    if [[ -n $swap ]]; then swapoff "$swap" && rm -f "$swap"; fi
else
    say "keeping the qurb already installed"
fi
# With no arguments it prints its usage. Nothing at all means it cannot run
# here: built for another kind of CPU, or against a newer C library.
/usr/local/bin/qurb 2>&1 | head -1 | grep -q "qurb" \
    || fail "the installed qurb does not run on this machine; try --source"

# -- the address devices will use -------------------------------------------------

if [[ -z $ADDRESS ]]; then
    ADDRESS=$(curl -fsS --max-time 5 https://api.ipify.org || curl -fsS --max-time 5 https://ifconfig.me || true)
fi
[[ $ADDRESS =~ ^[0-9.]+$|: ]] || fail "could not find this server's public address; give --address"
say "devices will reach this server at $ADDRESS"

# -- the services -------------------------------------------------------------------

say "installing the services"
id qurb >/dev/null 2>&1 || useradd --system --no-create-home --shell /usr/sbin/nologin qurb
sed "s/--host 203.0.113.5/--host $ADDRESS/" qurb-signal-tls.service >/etc/systemd/system/qurb-signal-tls.service
install -m644 qurb-relay.service /etc/systemd/system/qurb-relay.service
systemctl daemon-reload
systemctl disable --now qurb-signal 2>/dev/null || true
systemctl enable --now qurb-signal-tls qurb-relay
systemctl restart qurb-signal-tls qurb-relay

# -- this machine's own firewall ----------------------------------------------------
#
# The relay is TCP, like the rendezvous service. Oracle's Ubuntu images reject
# everything but SSH in iptables, below any ufw; both are handled.

say "opening TCP 9000 and 9001 on this machine"
if command -v ufw >/dev/null && ufw status | grep -q "Status: active"; then
    ufw allow 9000/tcp >/dev/null
    ufw allow 9001/tcp >/dev/null
fi
for tool in iptables ip6tables; do
    command -v "$tool" >/dev/null || continue
    for port in 9000 9001; do
        "$tool" -C INPUT -p tcp --dport "$port" -j ACCEPT 2>/dev/null && continue
        # Above the first REJECT, which is where Oracle's images end the chain.
        at=$("$tool" -L INPUT --line-numbers -n | awk '/REJECT/ {print $1; exit}')
        if [[ -n $at ]]; then
            "$tool" -I INPUT "$at" -p tcp --dport "$port" -j ACCEPT
        else
            "$tool" -A INPUT -p tcp --dport "$port" -j ACCEPT
        fi
    done
done
command -v netfilter-persistent >/dev/null && netfilter-persistent save >/dev/null 2>&1 || true

# -- what to tell the devices -------------------------------------------------------

say "waiting for the rendezvous service to print its address"
url=""
for _ in $(seq 1 30); do
    url=$(journalctl -u qurb-signal-tls --no-pager -n 100 2>/dev/null | grep -o "wss://[^ ]*#[0-9a-f]\{64\}" | tail -1 || true)
    [[ -n $url ]] && break
    sleep 1
done
[[ -n $url ]] || fail "the rendezvous service did not start; see: journalctl -u qurb-signal-tls"

ss -ltn | grep -qE ':9000\b' || fail "nothing is listening on 9000"
ss -ltn | grep -qE ':9001\b' || fail "nothing is listening on 9001"

cat <<DONE

== ready

On your computer:

  qurb config 'signal=$url' relay=$ADDRESS:9001

  then quit qurb from its Settings and open it again.

On the phone, in qurb's Settings:

  Rendezvous service   $url
  Relay                $ADDRESS:9001

The cloud's own firewall must also let TCP 9000 and 9001 in, from 0.0.0.0/0 --
once, in its web console:

  Google Cloud   VPC network > Firewall > Create a firewall rule, ingress,
                 targets with the tag qurb, TCP 9000,9001 -- and the tag qurb
                 on this machine.
  Oracle Cloud   Networking > Virtual cloud networks > your network > Security
                 > the Default security list > Add ingress rules, for each port.
DONE
