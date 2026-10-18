#!/usr/bin/env bash
#
# Remove the default route that DHCP adds on the boards-only Ethernet link
# (eth0 → STM32H563), so internet traffic keeps using wlan0.
#
# Idempotent: succeeds when the route is already absent.

set -u

BOARDS_IF="${BOARDS_IF:-eth0}"

# dhcpcd may add the route a few seconds after network-online.
sleep 10

while ip route show default dev "$BOARDS_IF" | grep -q .; do
    ip route del default dev "$BOARDS_IF" || break
done

if ip route show default dev "$BOARDS_IF" | grep -q .; then
    echo "default route on $BOARDS_IF still present" >&2
    exit 1
fi
exit 0
