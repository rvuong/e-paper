//! HTTP calls to the PRIM API, over HTTPS checked against the usual public
//! root certificates. Not unit-tested: the logic lives in `prim`, this module
//! only moves bytes. Checked by hand against the real API.

use std::time::Duration;

use ureq::Agent;

use crate::prim::stop_monitoring_url;

/// A connection to the PRIM API.
pub struct Client {
    agent: Agent,
    api_key: String,
}

/// An HTTP answer, whatever its status.
pub struct Answer {
    pub status: u16,
    pub body: String,
}

impl Client {
    pub fn new(api_key: String) -> Self {
        let agent: Agent = Agent::config_builder()
            // Quota and authorization errors come with a body: keep it.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(20)))
            .build()
            .into();
        Self { agent, api_key }
    }

    /// Next departures at one stop, as returned by the API.
    pub fn stop_monitoring(&self, monitoring_ref: &str) -> Result<Answer, ureq::Error> {
        let mut response = self
            .agent
            .get(stop_monitoring_url(monitoring_ref))
            .header("apikey", self.api_key.as_str())
            .header("Accept", "application/json")
            .call()?;
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_string()?;
        Ok(Answer { status, body })
    }
}
