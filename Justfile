target := "./target/aarch64-unknown-linux-gnu/release/cloudflare-dns-updater"

set dotenv-load

default: install

install: build
    cp ./target/release/cloudflare-dns-updater $HOME/.cargo/bin/

build:
    @cargo build --release

cross:
    @cross build --release

install-remote: cross
    @scp {{target}} root@$REMOTEHOST:/usr/local/bin/
    @croc --yes send {{target}}
