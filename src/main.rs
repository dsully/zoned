use std::fs;
use std::net::IpAddr;
use std::str::FromStr;

use anyhow::Context;
use cloudflare::endpoints::dns;
use cloudflare::endpoints::dns::{DnsContent, DnsRecord};
use cloudflare::framework::{
    auth::Credentials, response::ApiSuccess, Environment, HttpApiClient, HttpApiClientConfig,
};
use local_ip_address::local_ip;
use serde::{Deserialize, Serialize};

const V4_URL: &str = "https://ipinfo.io/json";
const V6_URL: &str = "https://v6.ipinfo.io/json";

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub token: String,
    pub zoneid: String,
    pub hostname: String,
    pub public: Option<String>,
}

fn config_file() -> anyhow::Result<Config> {
    let xdg_dir =
        xdg::BaseDirectories::with_prefix("dns-updater").context("Failed get config directory")?;

    let filename = xdg_dir
        .place_config_file("config.toml")
        .context("failed get path")?;

    let contents =
        fs::read_to_string(&filename).with_context(|| format!("Couldn't read {:?}", filename))?;

    let config: Config =
        toml::from_str(&contents).with_context(|| format!("Couldn't parse {:?}", filename))?;

    Ok(config)
}

fn local_ip_address() -> anyhow::Result<IpAddr> {
    let ip = local_ip()?.to_string();

    IpAddr::from_str(&ip).context("failed to parse IPv4 address")
}

fn remote_ip_address(url: &str) -> anyhow::Result<IpAddr> {
    let response =
        reqwest::blocking::get(url).with_context(|| "Failed to get IPv6 Address from API!");

    let parsed: serde_json::Value = response.unwrap().json()?;
    let ip = parsed["ip"]
        .as_str()
        .with_context(|| "Failed to get IPv6 Address from API!")?;

    IpAddr::from_str(ip).context("failed to parse IPv6 address")
}

fn ip_from_record(record: &DnsRecord) -> IpAddr {
    match record.content {
        DnsContent::A { content } => IpAddr::V4(content),
        DnsContent::AAAA { content } => IpAddr::V6(content),
        _ => panic!("Unsupported record type: {:?}", record),
    }
}

fn update_zone(
    zoneid: &String,
    hostname: &String,
    client: &HttpApiClient,
    detected_ip_addr: IpAddr,
) -> anyhow::Result<()> {
    let detected_dns_content = match detected_ip_addr {
        IpAddr::V4(ip) => DnsContent::A { content: ip },
        IpAddr::V6(ip) => DnsContent::AAAA { content: ip },
    };

    // Fetch the current DNS record matching the record type and given name.
    let current_dns_record = {
        client
            .request(&dns::ListDnsRecords {
                zone_identifier: zoneid,
                params: dns::ListDnsRecordsParams {
                    name: Some(hostname.to_string()),
                    record_type: Some(detected_dns_content.clone()),
                    ..Default::default()
                },
            })
            .map(|response: ApiSuccess<Vec<DnsRecord>>| {
                response.result.into_iter().find(|record| {
                    matches!(
                        record.content,
                        DnsContent::A { .. } | DnsContent::AAAA { .. }
                    )
                })
            })?
    };

    // println!("Record {:?}", current_dns_record);

    // If the record exists
    if let Some(current_dns_record) = current_dns_record {
        let current_ip_addr = ip_from_record(&current_dns_record);
        if detected_ip_addr == current_ip_addr {
            println!(
                "No change required. {} is already set to {}",
                hostname, current_ip_addr
            );
        } else {
            println!(
                "Updating {} from {} to {}",
                hostname, current_ip_addr, detected_ip_addr
            );

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
                .with_context(|| "")?;
        }
    } else {
        println!(
            "No record for {} exists. Creating as {}",
            hostname, detected_ip_addr
        );

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
            .with_context(|| "")?;
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let config: Config = config_file()?;

    let credentials = Credentials::UserAuthToken {
        token: config.token.clone(),
    };

    let client = HttpApiClient::new(
        credentials,
        HttpApiClientConfig::default(),
        Environment::Production,
    )
    .context("Unable to initialize client")?;

    if let Some(public) = config.public {
        update_zone(
            &config.zoneid,
            &public,
            &client,
            remote_ip_address(V4_URL)?,
        )
        .with_context(|| "Failed to update public IPv4 Record")?;
    };

    update_zone(
        &config.zoneid,
        &config.hostname,
        &client,
        local_ip_address()?,
    )
    .with_context(|| "Failed to update IPv4 Record")?;

    update_zone(
        &config.zoneid,
        &config.hostname,
        &client,
        remote_ip_address(V6_URL)?,
    )
    .with_context(|| "Failed to update IPv6 Record")?;

    Ok(())
}
