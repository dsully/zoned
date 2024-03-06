# Update Cloudflare DNS with IPv6 addresses

## Installation

```shell
brew install dsully/tap/zoned
```

Or from source:

```shell
cargo install --git https://github.com/dsully/zoned
```

## Configuration

A configuration file needs to be created in $XDG_CONFIG_HOME/zoned/config.toml

Example:

```toml
token = "<cloudflare api token>"
zoneid = "<cloudflare dns zone id>"
hostname = "<hostname to publish records to>"
```
