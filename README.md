# Cloudflare DNS updater using global IPv6 addresses

None of the other DNS updaters I've found actually update the IPv6
address for the host that this app runs on, in addition to the (potentially) private IPv4 address.

So this does that.

It can be cross compiled to run on Ubiquiti Dream Machine routers as well.

Build it with [just](https://github.com/casey/just):

```bash
just build
```
