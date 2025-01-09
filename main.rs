#![warn(clippy::all, clippy::pedantic)]

use std::net::IpAddr;

use anyhow::{Context, Result};
use clap::Parser;
use cloudflare::endpoints::dns;
use cloudflare::endpoints::dns::{DnsContent, DnsRecord};
use cloudflare::framework::{
    async_api::Client, auth::Credentials, response::ApiSuccess, Environment, HttpApiClientConfig,
};
use local_ip_address::local_ip;
use serde::Deserialize;
use tracing::{debug, info};
use tracing_subscriber::{filter::filter_fn, prelude::*};

const V6_URL: &str = "https://v6.ipinfo.io/json";

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub zoneid: String,
    pub hostname: String,
    pub ssid: Option<String>,
}

fn config_file() -> Result<Config> {
    let xdg_dir =
        xdg::BaseDirectories::with_prefix("zoned").context("Failed get config directory")?;

    let filename = xdg_dir.place_config_file("config.toml")?;

    let builder = config::Config::builder()
        .add_source(config::File::from(filename))
        .build()
        .context("Unable to load config file!")?;

    builder
        .try_deserialize()
        .context("Unable to parse config file!")
}

fn local_ip_address() -> Result<IpAddr> {
    let ip = local_ip()?.to_string();

    debug!("Found Local IP: {ip}");

    ip.parse().context("failed to parse IPv4 address")
}

async fn remote_ip_address(url: &str) -> Result<IpAddr> {
    debug!("Fetching IPv6 Address from {url}");

    let response = reqwest::get(url).await.unwrap();

    let parsed: serde_json::Value = response.json().await?;

    let ip = parsed["ip"]
        .as_str()
        .context("Failed to get IPv6 Address from API!")?;

    debug!("Found IPv6 Address: {}", ip);

    ip.parse().context("failed to parse IPv6 address")
}

fn ip_from_record(record: &DnsRecord) -> IpAddr {
    match record.content {
        DnsContent::A { content } => IpAddr::V4(content),
        DnsContent::AAAA { content } => IpAddr::V6(content),
        _ => panic!("Unsupported record type: {record:?}"),
    }
}

mod wifi {
    pub fn ssid() -> Option<String> {
        default_interface().and_then(|i| {
            std::process::Command::new("networksetup")
                .args(["-getairportnetwork", &i])
                .output()
                .ok()
                .and_then(|output| {
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout)
                            .split(": ")
                            .nth(1)
                            .map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
        })
    }

    pub fn default_interface() -> Option<String> {
        netdev::get_default_interface().ok().map(|i| i.name)
    }
}

async fn update_zone(
    zoneid: &String,
    hostname: &String,
    client: &Client,
    detected_ip_addr: IpAddr,
) -> Result<()> {
    let detected_dns_content = match detected_ip_addr {
        IpAddr::V4(ip) => DnsContent::A { content: ip },
        IpAddr::V6(ip) => DnsContent::AAAA { content: ip },
    };

    // Fetch the current DNS record matching the record type and given name.
    let current_dns_record = client
        .request(&dns::ListDnsRecords {
            zone_identifier: zoneid,
            params: dns::ListDnsRecordsParams {
                name: Some(hostname.to_string()),
                record_type: Some(detected_dns_content.clone()),
                ..Default::default()
            },
        })
        .await
        .map(|response: ApiSuccess<Vec<DnsRecord>>| {
            response.result.into_iter().find(|record| {
                matches!(
                    record.content,
                    DnsContent::A { .. } | DnsContent::AAAA { .. }
                )
            })
        })?;

    // If the record exists
    if let Some(current_dns_record) = current_dns_record {
        debug!("Current DNS Record {current_dns_record:#?}");

        let current_ip_addr = ip_from_record(&current_dns_record);

        if detected_ip_addr == current_ip_addr {
            info!("No change required. {hostname} is already set to {current_ip_addr}");
        } else {
            info!("Updating {hostname} from {current_ip_addr} to {detected_ip_addr}");

            // Update the DNS record
            client
                .request(&dns::UpdateDnsRecord {
                    zone_identifier: zoneid,
                    identifier: &current_dns_record.id,
                    params: dns::UpdateDnsRecordParams {
                        name: hostname,
                        content: detected_dns_content,
                        proxied: Some(current_dns_record.proxied),
                        ttl: Some(current_dns_record.ttl),
                    },
                })
                .await
                .context("Unable to update the DNS record!")?;
        }
    } else {
        info!("No record for {hostname} exists. Creating as {detected_ip_addr}");

        // Create the DNS record
        client
            .request(&dns::CreateDnsRecord {
                zone_identifier: zoneid,
                params: dns::CreateDnsRecordParams {
                    name: hostname,
                    content: detected_dns_content,
                    proxied: Some(false),
                    ttl: None,
                    priority: None,
                },
            })
            .await
            .context("Unable to create a DNS record!")?;
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Cli {
    #[command(flatten)]
    verbose: clap_verbosity_flag::Verbosity,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Log from this crate only.
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            cli.verbose.log_level_filter().to_string(),
        ))
        .with(
            tracing_subscriber::fmt::layer().with_filter(filter_fn(|metadata| {
                metadata.target().starts_with(env!("CARGO_PKG_NAME"))
            })),
        )
        .init();

    let config: Config = config_file()?;

    if config.ssid.is_some() && config.ssid != wifi::ssid() {
        info!("SSID does not match. Exiting.");
        std::process::exit(1);
    }

    let credentials = Credentials::UserAuthToken {
        token: config.token.clone(),
    };

    let client = Client::new(
        credentials,
        HttpApiClientConfig::default(),
        Environment::Production,
    )
    .context("Unable to initialize client")?;

    update_zone(
        &config.zoneid,
        &config.hostname,
        &client,
        local_ip_address()?,
    )
    .await
    .context("Failed to update IPv4 Record")?;

    update_zone(
        &config.zoneid,
        &config.hostname,
        &client,
        remote_ip_address(V6_URL).await?,
    )
    .await
    .context("Failed to update IPv6 Record")?;

    Ok(())
}
