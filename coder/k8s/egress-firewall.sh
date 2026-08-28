#!/usr/bin/env bash
# Deny kind pods from reaching THIS host and the LAN, while keeping pod-to-pod and
# internet egress working. This is what actually contains the coder's network:
# kindnet does NOT enforce Kubernetes NetworkPolicy, so we enforce at the host with
# iptables.
#
# Two separate chains are needed, not one:
#   - DOCKER-USER (hooked into FORWARD) sees genuinely-forwarded traffic: pod -> pod,
#     pod -> LAN, pod -> internet.
#   - Traffic from a pod to an address THIS HOST OWNS (e.g. the kind bridge gateway,
#     or any other host-owned IP) is locally delivered by the kernel's routing
#     decision straight to INPUT — it never reaches FORWARD/DOCKER-USER at all. A
#     DOCKER-USER-only DROP rule for the host address is therefore a silent no-op;
#     containing "no localhost services, no daemon" requires its own INPUT rule.
#
# Effect for every kind pod (incl. the coder):
#   pod -> other kind pods (same subnet)  : ALLOWED  (cluster networking keeps working)
#   pod -> this host (any host-owned IP)  : DROPPED  (no localhost services, no daemon) [INPUT]
#   pod -> LAN (10/8, 172.16/12, 192.168) : DROPPED  (no other machines)                [DOCKER-USER]
#   pod -> internet (e.g. ollama.com)     : ALLOWED  (nothing sensitive to exfiltrate)
#
# Requires sudo (iptables). Idempotent. Reverse with:  egress-firewall.sh --remove
#
#   sudo coder/k8s/egress-firewall.sh            # install
#   sudo coder/k8s/egress-firewall.sh --remove   # uninstall
#   sudo coder/k8s/egress-firewall.sh --status   # show the rules
set -euo pipefail

FWD_CHAIN="DOCKER-USER"
HOST_CHAIN="INPUT"
# Discover the kind network's subnet + gateway (the host address pods would use).
SUBNET="$(docker network inspect kind -f '{{(index .IPAM.Config 0).Subnet}}' 2>/dev/null || echo 172.18.0.0/16)"
GW="$(docker network inspect kind -f '{{(index .IPAM.Config 0).Gateway}}' 2>/dev/null || echo 172.18.0.1)"
LAN_RANGES=(10.0.0.0/8 172.16.0.0/12 192.168.0.0/16)
TAG="cto-coder-egress"

# Forwarded traffic: pod -> pod (allow), pod -> LAN (deny). Pod -> internet falls
# through DOCKER-USER = allowed (no rule needed).
fwd_rules() {
  echo "-s $SUBNET -d $SUBNET -j RETURN -m comment --comment $TAG"      # pod -> pod (allow)
  for lan in "${LAN_RANGES[@]}"; do
    echo "-s $SUBNET -d $lan -j DROP -m comment --comment $TAG"         # pod -> LAN
  done
}

# Locally-delivered traffic: pod -> this host. Allow replies to connections the
# host itself initiated toward a pod (e.g. a kubelet health probe) before denying
# new pod-initiated connections to the host, so we don't break host->pod flows.
host_rules() {
  echo "-s $SUBNET -m conntrack --ctstate ESTABLISHED,RELATED -j RETURN -m comment --comment $TAG"
  echo "-s $SUBNET -j DROP -m comment --comment $TAG"                   # pod -> this host
}

do_status() {
  echo "-- $FWD_CHAIN --"
  iptables -S "$FWD_CHAIN" | grep "$TAG" || echo "(no $TAG rules installed)"
  echo "-- $HOST_CHAIN --"
  iptables -S "$HOST_CHAIN" | grep "$TAG" || echo "(no $TAG rules installed)"
}

# Exit non-zero if the containment rules are absent — for a health check. These
# rules do NOT survive a Docker restart or host reboot, so containment can lapse
# silently while the pod still looks deployed; a caller (deploy.sh, a systemd unit)
# can gate on this.
do_verify() {
  local ok=1
  iptables -S "$FWD_CHAIN" 2>/dev/null | grep -q "$TAG" || ok=0
  iptables -S "$HOST_CHAIN" 2>/dev/null | grep -q "$TAG" || ok=0
  if [ "$ok" = 1 ]; then
    echo "✓ $TAG egress rules present ($FWD_CHAIN + $HOST_CHAIN)"
  else
    echo "✗ $TAG egress rules ABSENT — the coder is NOT contained (re-run without --verify)" >&2
    exit 1
  fi
}

# Delete any rule carrying our tag from one chain (loop until none remain).
remove_from_chain() {
  local chain="$1"
  while iptables -S "$chain" | grep -q "$TAG"; do
    line="$(iptables -S "$chain" | grep "$TAG" | head -1)"
    # shellcheck disable=SC2086
    iptables -D "$chain" ${line#-A $chain }
  done
}

do_remove() {
  remove_from_chain "$FWD_CHAIN"
  remove_from_chain "$HOST_CHAIN"
  echo "removed $TAG rules"
}

install_into_chain() {
  local chain="$1"
  shift
  # Insert at the TOP of the chain in reverse, so final order matches the list.
  local -a r=("$@")
  local i
  for ((i=${#r[@]}-1; i>=0; i--)); do
    # shellcheck disable=SC2086
    iptables -I "$chain" 1 ${r[$i]}
  done
}

do_install() {
  do_remove >/dev/null 2>&1 || true       # clean slate so ordering is deterministic
  mapfile -t FR < <(fwd_rules)
  mapfile -t HR < <(host_rules)
  install_into_chain "$FWD_CHAIN" "${FR[@]}"
  install_into_chain "$HOST_CHAIN" "${HR[@]}"
  echo "installed $TAG egress rules for kind subnet $SUBNET (host $GW + LAN denied):"
  do_status
}

case "${1:-}" in
  --remove) do_remove ;;
  --status) do_status ;;
  --verify) do_verify ;;
  *)        do_install ;;
esac
