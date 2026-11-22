//! The Switch build's stand-in for if-addrs (packaging/switch/cargo-patch.toml). newlib on
//! Horizon OS has no `getifaddrs`, and a Switch has one address at a time: the one libnx's
//! `gethostid` returns (nifm's current IPv4 address, in network byte order; the loopback address
//! when there is no connection).

use std::net::{IpAddr, Ipv4Addr};

/// One address of one interface, as far as the game reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Interface {
    pub name: String,
    addr: IpAddr,
}

impl Interface {
    pub fn ip(&self) -> IpAddr {
        self.addr
    }
    pub fn is_loopback(&self) -> bool {
        self.addr.is_loopback()
    }
}

#[cfg(target_os = "horizon")]
fn current() -> Option<Ipv4Addr> {
    unsafe extern "C" {
        fn gethostid() -> core::ffi::c_long;
    }
    // libnx answers with nifm's `in_addr.s_addr` (the address's bytes as they lie in memory), or
    // with the constant INADDR_LOOPBACK (a host-order number) when nifm has none. Unverified on
    // a console: a wrong reading only changes which address Play with Friends offers.
    let id = unsafe { gethostid() } as u32;
    if id == 0x7f00_0001 {
        return Some(Ipv4Addr::LOCALHOST);
    }
    let ip = Ipv4Addr::from(id.to_ne_bytes());
    (!ip.is_unspecified()).then_some(ip)
}

#[cfg(not(target_os = "horizon"))]
fn current() -> Option<Ipv4Addr> {
    None
}

pub fn get_if_addrs() -> std::io::Result<Vec<Interface>> {
    Ok(current()
        .into_iter()
        .map(|ip| Interface {
            name: if ip.is_loopback() { "lo" } else { "wlan0" }.to_owned(),
            addr: IpAddr::V4(ip),
        })
        .collect())
}
