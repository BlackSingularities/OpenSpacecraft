//! Wspólny klient HTTP (redzikowski.com, CelesTrak).

use std::sync::OnceLock;
use std::time::Duration;

pub fn agent() -> ureq::Agent {
    static A: OnceLock<ureq::Agent> = OnceLock::new();
    A.get_or_init(|| {
        ureq::AgentBuilder::new()
            .user_agent(concat!("OpenSpacecraft/", env!("CARGO_PKG_VERSION")))
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(120))
            .build()
    })
    .clone()
}
