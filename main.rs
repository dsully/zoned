use anyhow::{Context, Result};
use cloudflare::endpoints::dns;
use cloudflare::endpoints::dns::{DnsContent, DnsRecord};
use cloudflare::framework::{
    auth::Credentials, response::ApiSuccess, Environment, HttpApiClient, HttpApiClientConfig,
};
use default_net::get_default_interface;
use ip_network::Ipv6Network;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::process::Command;
use std::str::FromStr;

const V4_URL: &str = "https://ipinfo.io/json";

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub token: String,
    pub zoneid: String,
    pub hostname: String,
    pub external: Option<String>,
}

fn config_file() -> Result<Config> {
    let xdg_dir =
        xdg::BaseDirectories::with_prefix("dns-updater").context("Failed get config directory")?;

    let filename = xdg_dir
        .place_config_file("config.toml")
        .context("failed get path")?;

    let config: Config = config::Config::builder()
        .add_source(config::File::from(filename.as_ref()))
        .build()?
        .try_deserialize()
        .with_context(|| format!("Couldn't parse {:?}", filename))?;

    Ok(config)
}

fn is_home_network(external_host: String) -> anyhow::Result<bool> {
    let response =
        reqwest::blocking::get(V4_URL).with_context(|| "Failed to get IPv4 Address from API!")?;

    let parsed: serde_json::Value = response.json()?;
    let ip = parsed["ip"]
        .as_str()
        .with_context(|| "Failed to parse IPv4 Address from API!")?;

    let external_ip = Ipv4Addr::from_str(ip).context("failed to parse IPv4 address from JSON")?;

    let mut sockets = format!("{}:443", external_host)
        .to_socket_addrs()
        .with_context(|| "Failed to lookup external IPv4 Address!")?;

    match sockets.find(|s| s.ip().is_ipv4()) {
        Some(socket) => Ok(external_ip == socket.ip()),
        None => Ok(false),
    }
}

pub fn local_ips() -> Vec<IpAddr> {
    // Future: Extract out the ipv6 flags and pull only the secured/global address.
    // println!("\tIPv6: {:?}", default_interface.ipv6);
    let interface = match get_default_interface() {
        Ok(interface) => interface,
        Err(e) => {
            println!("Failed to get default interface: {}", e);
            return vec![];
        }
    };

    let mut ips: Vec<IpAddr> = vec![];

    let output = Command::new("ifconfig")
        .arg(interface.name)
        .output()
        .expect("Failed to execute `ifconfig`");

    let stdout = String::from_utf8(output.stdout)
        .context("Couldn't decode output from 'ifconfig'")
        .unwrap();

    let re6 = Regex::new(r#"inet6\s+([\da-fA-F:]+)\s+prefixlen\s+\d+\s+(.*)"#).unwrap();
    for cap in re6.captures_iter(&stdout) {
        if let Some(flags) = cap.get(2) {
            if flags.as_str().contains("secured") || flags.as_str().contains("global") {
                if let Some(host) = cap.get(1) {
                    if let Ok(addr) = host.as_str().parse::<Ipv6Addr>() {
                        if Ipv6Network::from(addr).is_global()
                            && host.as_str().starts_with("2001:5a8:")
                        {
                            ips.push(IpAddr::V6(addr));
                        }
                    }
                }
            }
        }
    }

    for ip in interface.ipv4 {
        ips.push(IpAddr::V4(ip.addr));
    }

    ips
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
) -> Result<()> {
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

fn main() -> Result<()> {
    let config: Config = config_file()?;

    if config.external.is_some() && !is_home_network(config.external.unwrap())? {
        println!("Not on home network. Skipping DNS update.");
        return Ok(());
    }

    let credentials = Credentials::UserAuthToken {
        token: config.token.clone(),
    };

    let client = HttpApiClient::new(
        credentials,
        HttpApiClientConfig::default(),
        Environment::Production,
    )
    .context("Unable to initialize client")?;

    for ip in local_ips() {
        update_zone(&config.zoneid, &config.hostname, &client, ip)
            .with_context(|| "Failed to update DNS Record")?;
    }

    Ok(())
}
