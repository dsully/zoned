#!/usr/bin/env python3

import socket

from dataclasses import dataclass, field
from typing import Any

import CloudFlare
import netifaces
import requests
import rich

from getmac import get_mac_address
from netaddr.ip import IPAddress, IPNetwork
from netaddr.eui import EUI
from pyroute2 import IPRoute

GW = "10.0.0.1"


@dataclass
class Host:
    name: str
    ip: str = field(default="")
    type: str = field(default="")


def local_ipv4() -> str:
    route: dict[str, Any] = next(iter(IPRoute().route('get', dst=GW)))

    return str(dict(route['attrs'])['RTA_PREFSRC'])


def local_addresses() -> list[Host]:
    prefix = 0

    # # Get the IPv6 prefix of the global address.
    for addresses in netifaces.ifaddresses(IF)[netifaces.AF_INET6]:
        addr = IPAddress(addresses["addr"])  # type: ignore
        network = IPNetwork(addresses["netmask"]).prefixlen  # type: ignore

        if addr.is_link_local():
            rich.print("[yellow]Skipping link-local address.[/yellow]")
            continue

        prefix = IPAddress(IPNetwork(f"{addr}/{network}").network)

        break

    def host(name: str) -> str:
        return str(socket.gethostbyname(f"{name}.sully.org"))

    def ip(name: str) -> str:
        return str(EUI(get_mac_address(hostname=name) or "").ipv6(prefix))


    rich.print("Getting public IP addresses...")

    hosts = [
        Host("gateway.sully.org", type="A", ip=requests.get("https://ipinfo.io/json").json()["ip"]),
        Host("gateway.sully.org", type="AAAA", ip=requests.get("https://v6.ipinfo.io/json").json()["ip"]),
        Host("dsully-md1.sully.org", type="A", ip=host("dsully-md1")),
        Host("dsully-mn2.sully.org", type="A", ip=host("dsully-mn2")),
        Host("gpu.sully.org", type="A", ip=host("gpu")),
        Host("jarvis.sully.org", type="A", ip=host("jarvis")),
        Host("server.sully.org", type="A", ip=host("server")),
        Host("dsully-md1.sully.org", type="AAAA", ip=ip("dsully-md1")),
        Host("dsully-mn2.sully.org", type="AAAA", ip=ip("dsully-mn2")),
        Host("gpu.sully.org", type="AAAA", ip=ip("gpu")),
        Host("jarvis.sully.org", type="AAAA", ip=ip("jarvis")),
        Host("server.sully.org", type="AAAA", ip=str(EUI(get_mac_address(interface=IF) or "").ipv6(prefix))),
    ]

    return hosts


def main():
    cf = CloudFlare.CloudFlare()

    rich.print("Updating CloudFlare DNS records...")

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
                rich.print(f"[blue]Unchanged[/blue]: {host.name} -> {host.ip}")
                continue

            try:
                cf.zones.dns_records.put(
                    zone_id,
                    match["id"],
                    data={"name": host.name, "type": host.type, "content": host.ip, "proxied": match["proxied"]},
                )

                rich.print(f"[green]Updated[/green]: {host.name} {old_ip} -> {host.ip}")
            except Exception as e:
                rich.print(f"/zones.dns_records.put {host.name} - {e} - [red]API call failed![/red]")

        else:
            try:
                cf.zones.dns_records.post(zone_id, data={"name": host.name, "type": host.type, "content": host.ip})
                rich.print(f"[green]Added[/green]: {host.name} -> {host.ip}")
            except Exception as e:
                rich.print(f"/zones.dns_records.post {host.name} - {e} - [red]API call failed![/red]")


if __name__ == "__main__":
    main()
