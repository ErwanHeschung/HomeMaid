//! Pairing and discovery for the app, shared by the core and the agent:
//! - `/pair` shows a QR code with `homemaid://pair?kind&id&host&port&token`, on loopback only
//!   since it holds the token: open it on the PC itself;
//! - an mDNS announcement (`_homemaid-<kind>._tcp`, TXT `id`) lets the app find the host again
//!   when its IP changes.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};

use mdns_sd::{ServiceDaemon, ServiceInfo};

pub fn hostname() -> String {
    sysinfo::System::host_name().unwrap_or_else(|| "homemaid".into())
}

/// The address the phone should dial: the bound one, or else the source of the default route
/// (a UDP "connect" picks the route without sending anything).
pub fn lan_ip(bind: SocketAddr) -> IpAddr {
    if !bind.ip().is_unspecified() {
        return bind.ip();
    }
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:9")?;
            socket.local_addr()
        })
        .map_or(Ipv4Addr::LOCALHOST.into(), |addr| addr.ip())
}

pub fn uri(kind: &str, id: &str, host: IpAddr, port: u16, token: &str) -> String {
    format!("homemaid://pair?kind={kind}&id={}&host={host}&port={port}&token={}", encode(id), encode(token))
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

pub fn is_local_peer(peer: SocketAddr) -> bool {
    peer.ip().to_canonical().is_loopback()
}

/// The `/pair` page. It holds the token: serve it only to [`is_local_peer`].
pub fn page(kind: &str, bind: SocketAddr, token: &str) -> String {
    let qr = qr_svg(kind, bind, token);
    let what = if kind == "core" { "la maison" } else { "ce PC" };
    PAGE.replace("{what}", what).replace("{qr}", &qr)
}

const PAGE: &str = r#"<!doctype html>
<html lang="fr">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>HomeMaid · Appairage</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; padding: 16px; box-sizing: border-box;
         background: #0e0f12; color: #e8e9ec; font: 16px/1.4 system-ui, sans-serif; text-align: center; }
  .qr { background: #fff; padding: 16px; border-radius: 16px; display: inline-block; }
  .qr svg { display: block; max-width: 100%; height: auto; }
  p { color: #8b8f98; max-width: 32em; }
</style>
<main>
  <h1>Appairer {what}</h1>
  <div class="qr">{qr}</div>
  <p>Dans l'app HomeMaid : Réglages → Appairer, puis scanne ce code.<br>Il contient le token d'accès : ne le partage pas.</p>
</main>
</html>
"#;

/// The pairing QR code as SVG. It holds the token: show it on the PC itself only.
pub fn qr_svg(kind: &str, bind: SocketAddr, token: &str) -> String {
    let uri = uri(kind, &hostname(), lan_ip(bind), bind.port(), token);
    qrcode::QrCode::new(uri.as_bytes())
        .map(|qr| qr.render::<qrcode::render::svg::Color>().min_dimensions(280, 280).build())
        .unwrap_or_else(|e| format!("<p>QR code impossible : {e}</p>"))
}

/// Keeps the service announced until dropped.
pub struct Announcement(Option<(ServiceDaemon, String)>);

pub fn announce(kind: &str, port: u16) -> Announcement {
    let id = hostname();
    let registered = ServiceDaemon::new().and_then(|daemon| {
        let properties = [("id", id.as_str()), ("kind", kind)];
        let info = ServiceInfo::new(&format!("_homemaid-{kind}._tcp.local."), &id, &format!("{id}.local."), (), port, &properties[..])?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_string();
        daemon.register(info)?;
        Ok((daemon, fullname))
    });
    match registered {
        Ok((daemon, fullname)) => {
            tracing::info!("announced on mDNS as {fullname}");
            Announcement(Some((daemon, fullname)))
        }
        // Discovery is a convenience: the paired address still works without it.
        Err(e) => {
            tracing::warn!("mDNS announcement failed: {e}");
            Announcement(None)
        }
    }
}

impl Drop for Announcement {
    fn drop(&mut self) {
        if let Some((daemon, fullname)) = self.0.take() {
            let _ = daemon.unregister(&fullname);
            let _ = daemon.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_uri_is_escaped() {
        let uri = uri("agent", "MY PC", "192.168.1.10".parse().unwrap(), 8124, "a&b=c");
        assert_eq!(uri, "homemaid://pair?kind=agent&id=MY%20PC&host=192.168.1.10&port=8124&token=a%26b%3Dc");
    }

    #[test]
    fn pair_page_is_for_the_pc_itself() {
        assert!(is_local_peer("127.0.0.1:5000".parse().unwrap()));
        assert!(is_local_peer("[::1]:5000".parse().unwrap()));
        assert!(is_local_peer("[::ffff:127.0.0.1]:5000".parse().unwrap()));
        assert!(!is_local_peer("192.168.1.20:5000".parse().unwrap()));
    }

    #[test]
    fn page_contains_a_qr_code() {
        let page = page("core", "127.0.0.1:8123".parse().unwrap(), "t");
        assert!(page.contains("<svg") && page.contains("Appairer la maison"));
    }
}
