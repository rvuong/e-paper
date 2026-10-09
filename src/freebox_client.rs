//! HTTP calls to the Freebox, over HTTPS checked against the Freebox root
//! certificates. Not unit-tested: the logic lives in `freebox`, this module
//! only moves bytes. Checked by hand against the real Freebox.

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use ureq::Agent;
use ureq::tls::{PemItem, RootCerts, TlsConfig};

use crate::freebox::{
    AuthorizationStep, FreeboxError, Host, api_base, authorization_step, parse_hosts,
    parse_response, session_password,
};

const HOST: &str = "https://mafreebox.freebox.fr";
/// Freebox ECC Root CA and Freebox Root CA, from the Freebox OS SDK documentation.
const ROOT_CA: &[u8] = include_bytes!("../assets/freebox/root-ca.pem");

const APP_ID: &str = "io.github.rvuong.e-paper";
const APP_NAME: &str = "e-paper dashboard";

/// A connection to the Freebox API, logged in or not.
pub struct Client {
    agent: Agent,
    /// Full API URL, e.g. `https://mafreebox.freebox.fr/api/v16/`.
    api: String,
    session_token: Option<String>,
}

impl Client {
    /// Finds the API version of the Freebox.
    pub fn connect() -> Result<Self, FreeboxError> {
        let certs = ureq::tls::parse_pem(ROOT_CA)
            .filter_map(|item| match item {
                Ok(PemItem::Certificate(cert)) => Some(cert),
                _ => None,
            })
            .collect();
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::Specific(Arc::new(certs)))
            .build();
        let agent: Agent = Agent::config_builder()
            .tls_config(tls)
            // Freebox errors come with a JSON body: read it instead of failing.
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into();
        let mut client = Self {
            agent,
            api: String::new(),
            session_token: None,
        };
        let version = client.get_text(&format!("{HOST}/api_version"))?;
        client.api = format!("{HOST}{}", api_base(&version)?);
        Ok(client)
    }

    /// Asks for an app token, then waits for the answer on the Freebox screen.
    pub fn authorize(&self, device_name: &str) -> Result<String, FreeboxError> {
        #[derive(Deserialize)]
        struct Request {
            app_token: String,
            track_id: u64,
        }
        #[derive(Deserialize)]
        struct Status {
            status: String,
        }

        let request: Request = self.post(
            "login/authorize/",
            json!({
                "app_id": APP_ID,
                "app_name": APP_NAME,
                "app_version": env!("CARGO_PKG_VERSION"),
                "device_name": device_name,
            }),
        )?;
        println!("Accept the request on the front screen of the Freebox Server...");
        loop {
            let status: Status = self.get(&format!("login/authorize/{}", request.track_id))?;
            match authorization_step(&status.status)? {
                AuthorizationStep::Wait => thread::sleep(Duration::from_secs(1)),
                AuthorizationStep::Granted => return Ok(request.app_token),
            }
        }
    }

    /// Opens a session with the app token.
    pub fn login(&mut self, app_token: &str) -> Result<(), FreeboxError> {
        #[derive(Deserialize)]
        struct Login {
            challenge: String,
        }
        #[derive(Deserialize)]
        struct Session {
            session_token: String,
        }

        let login: Login = self.get("login/")?;
        let session: Session = self.post(
            "login/session/",
            json!({
                "app_id": APP_ID,
                "password": session_password(app_token, &login.challenge),
            }),
        )?;
        self.session_token = Some(session.session_token);
        Ok(())
    }

    /// Devices known to the Freebox on the main LAN interface.
    pub fn lan_hosts(&self) -> Result<Vec<Host>, FreeboxError> {
        parse_hosts(&self.get_text(&format!("{}lan/browser/pub/", self.api))?)
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, FreeboxError> {
        parse_response(&self.get_text(&format!("{}{path}", self.api))?)
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: Value) -> Result<T, FreeboxError> {
        let mut request = self.agent.post(format!("{}{path}", self.api));
        if let Some(token) = &self.session_token {
            request = request.header("X-Fbx-App-Auth", token);
        }
        let mut response = request
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(http_error)?;
        parse_response(&response.body_mut().read_to_string().map_err(http_error)?)
    }

    fn get_text(&self, url: &str) -> Result<String, FreeboxError> {
        let mut request = self.agent.get(url);
        if let Some(token) = &self.session_token {
            request = request.header("X-Fbx-App-Auth", token);
        }
        let mut response = request.call().map_err(http_error)?;
        response.body_mut().read_to_string().map_err(http_error)
    }
}

fn http_error(error: ureq::Error) -> FreeboxError {
    FreeboxError::Http(error.to_string())
}
