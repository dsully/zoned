#!/usr/bin/env python3

import socket

from dataclasses import dataclass, field

import CloudFlare
import netifaces
import requests

from getmac import get_mac_address
from netaddr.ip import IPAddress, IPNetwork
from netaddr.eui import EUI

IF = "eno1"


@dataclass
class Host:
    name: str
    ip: str = field(default="")
    type: str = field(default="")


def local_addresses() -> list[Host]:
    prefix = 0

    # # Get the IPv6 prefix of the global address.
    for addresses in netifaces.ifaddresses(IF)[netifaces.AF_INET6]:
        addr = IPAddress(addresses["addr"])  # type: ignore
        network = IPNetwork(addresses["netmask"]).prefixlen  # type: ignore

        if addr.is_link_local():
            print("Skipping link-local address.")
            continue

        prefix = IPAddress(IPNetwork(f"{addr}/{network}").network)

        break

    print("Getting public IP addresses...")

    hosts = [
        Host("gateway.sully.org", type="A", ip=requests.get("https://ipinfo.io/json").json()["ip"]),
        Host("gateway.sully.org", type="AAAA", ip=requests.get("https://v6.ipinfo.io/json").json()["ip"]),
        Host("gpu.sully.org", type="A", ip=socket.gethostbyname("gpu")),
        Host("server.sully.org", type="A", ip=socket.gethostbyname("server.sully.org")),
        Host("gpu.sully.org", type="AAAA", ip=str(EUI(get_mac_address(hostname="gpu") or "").ipv6(prefix))),
        Host("server.sully.org", type="AAAA", ip=str(EUI(get_mac_address(interface=IF) or "").ipv6(prefix))),
    ]

    return hosts


def main():
    cf = CloudFlare.CloudFlare()

    print("Updating CloudFlare DNS records...")

    zone_info: dict[str, str] = cf.zones.get(params={"name": "sully.org"})[0]
    zone_id: str = zone_info["id"]

    # Update the record - unless it's already correct.
    records: list[dict[str, str]] = cf.zones.dns_records.get(zone_id)

    for host in local_addresses():
        match: dict[str, str] = {}

        for record in records:
            if host.name == record["name"] and record["type"] == host.type:
                match = record
                break

        if match:
            old_ip: str = match["content"]

            if host.ip == old_ip:
                print("UNCHANGED: %s -> %s" % (host.name, host.ip))
                continue

            try:
                cf.zones.dns_records.put(
                    zone_id,
                    match["id"],
                    data={"name": host.name, "type": host.type, "content": host.ip, "proxied": match["proxied"]},
                )

                print(f"UPDATED: {host.name} {old_ip} -> {host.ip}")
            except Exception as e:
                print(f"/zones.dns_records.put {host.name} - {e} - API call failed")

        else:
            try:
                cf.zones.dns_records.post(zone_id, data={"name": host.name, "type": host.type, "content": host.ip})
                print(f"ADDED: {host.name} -> {host.ip}")
            except Exception as e:
                print("/zones.dns_records.post %s - %d %s - API call failed" % (host.name, e, e))


if __name__ == "__main__":
    main()
