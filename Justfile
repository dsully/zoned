target := "./target/aarch64-unknown-linux-gnu/release/cloudflare-dns-updater"

set dotenv-load

default: install

install: build
    cp ./target/release/cloudflare-dns-updater $HOME/.cargo/bin/

install-remote: cross
    @scp {{target}} root@$REMOTEHOST:/usr/local/bin/
    @croc --yes send {{target}}

build:
    @cargo build --release

cross:
    @cross build --release

format:
    @cargo fmt --all

format-check:
    @cargo fmt --all -- --check

lint:
    @cargo clippy --all -- -D clippy::dbg-macro -D warnings
