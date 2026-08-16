use std::env;

use serde::{Deserialize, Serialize};

const GIPHY_API_KEY_ENV: &str = "SYMBOLIS_GIPHY_API_KEY";
const KLIPY_API_KEY_ENV: &str = "SYMBOLIS_KLIPY_API_KEY";
const DEFAULT_CLIENT_KEY: &str = "symbolis";

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum GifProvider {
    #[default]
    Local,
    Giphy,
    #[serde(alias = "Tenor")]
    Klipy,
}

impl GifProvider {
    pub(crate) const CHOICES: [GifProvider; 3] =
        [GifProvider::Local, GifProvider::Giphy, GifProvider::Klipy];

    pub(crate) fn label(self) -> &'static str {
        match self {
            GifProvider::Local => "Local library",
            GifProvider::Giphy => "GIPHY",
            GifProvider::Klipy => "Klipy",
        }
    }

    pub(crate) fn description(self) -> &'static str {
        match self {
            GifProvider::Local => "Search saved GIFs and stickers without network requests",
            GifProvider::Giphy => "Search GIPHY when SYMBOLIS_GIPHY_API_KEY is set",
            GifProvider::Klipy => "Search Klipy when SYMBOLIS_KLIPY_API_KEY is set",
        }
    }

    pub(crate) fn attribution(self) -> Option<&'static str> {
        match self {
            GifProvider::Local => None,
            GifProvider::Giphy => Some("Powered by GIPHY"),
            GifProvider::Klipy => Some("Powered by KLIPY"),
        }
    }

    pub(crate) fn api_key_env(self) -> Option<&'static str> {
        match self {
            GifProvider::Local => None,
            GifProvider::Giphy => Some(GIPHY_API_KEY_ENV),
            GifProvider::Klipy => Some(KLIPY_API_KEY_ENV),
        }
    }

    pub(crate) fn status(self) -> ProviderStatus {
        let Some(env_name) = self.api_key_env() else {
            return ProviderStatus::Ready("offline".to_owned());
        };

        match env::var(env_name) {
            Ok(value) if !value.trim().is_empty() => {
                ProviderStatus::Ready(format!("{env_name} set"))
            }
            _ => ProviderStatus::MissingApiKey(env_name),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProviderStatus {
    Ready(String),
    MissingApiKey(&'static str),
}

impl ProviderStatus {
    pub(crate) fn label(&self) -> String {
        match self {
            ProviderStatus::Ready(value) => value.clone(),
            ProviderStatus::MissingApiKey(env_name) => format!("missing {env_name}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum GifSearchKind {
    Gif,
    Sticker,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct GifSearchRequest {
    pub(crate) query: String,
    pub(crate) limit: u8,
    pub(crate) offset: u32,
    pub(crate) kind: GifSearchKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ProviderUrlError {
    LocalProvider,
    MissingApiKey(&'static str),
    EmptyQuery,
}

impl std::fmt::Display for ProviderUrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderUrlError::LocalProvider => {
                write!(f, "local library does not use provider URLs")
            }
            ProviderUrlError::MissingApiKey(env_name) => write!(f, "missing {env_name}"),
            ProviderUrlError::EmptyQuery => write!(f, "search query is empty"),
        }
    }
}

impl std::error::Error for ProviderUrlError {}

#[allow(dead_code)]
pub(crate) fn build_search_url(
    provider: GifProvider,
    request: &GifSearchRequest,
) -> Result<String, ProviderUrlError> {
    let query = request.query.trim();
    if query.is_empty() {
        return Err(ProviderUrlError::EmptyQuery);
    }

    match provider {
        GifProvider::Local => Err(ProviderUrlError::LocalProvider),
        GifProvider::Giphy => build_giphy_search_url(request, query),
        GifProvider::Klipy => build_klipy_search_url(request, query),
    }
}

fn build_giphy_search_url(
    request: &GifSearchRequest,
    query: &str,
) -> Result<String, ProviderUrlError> {
    let api_key = provider_api_key(GifProvider::Giphy)?;
    let endpoint = match request.kind {
        GifSearchKind::Gif => "https://api.giphy.com/v1/gifs/search",
        GifSearchKind::Sticker => "https://api.giphy.com/v1/stickers/search",
    };

    Ok(format!(
        "{endpoint}?api_key={api_key}&q={query}&limit={limit}&offset={offset}&rating=pg-13&bundle=messaging_non_clips",
        api_key = encode_query_component(&api_key),
        query = encode_query_component(query),
        limit = request.limit.clamp(1, 50),
        offset = request.offset,
    ))
}

fn build_klipy_search_url(
    request: &GifSearchRequest,
    query: &str,
) -> Result<String, ProviderUrlError> {
    let api_key = provider_api_key(GifProvider::Klipy)?;
    let mut url = format!(
        "https://api.klipy.com/v2/search?key={key}&client_key={client_key}&q={query}&limit={limit}&media_filter=tinygif,gif,mp4&contentfilter=medium",
        key = encode_query_component(&api_key),
        client_key = DEFAULT_CLIENT_KEY,
        query = encode_query_component(query),
        limit = request.limit.clamp(1, 50),
    );

    if request.kind == GifSearchKind::Sticker {
        url.push_str("&searchfilter=sticker");
    }

    if request.offset > 0 {
        url.push_str("&pos=");
        url.push_str(&request.offset.to_string());
    }

    Ok(url)
}

fn provider_api_key(provider: GifProvider) -> Result<String, ProviderUrlError> {
    let env_name = provider
        .api_key_env()
        .ok_or(ProviderUrlError::LocalProvider)?;
    match env::var(env_name) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ProviderUrlError::MissingApiKey(env_name)),
    }
}

fn encode_query_component(value: &str) -> String {
    let mut encoded = String::new();

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            b' ' => encoded.push('+'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }

    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_provider_has_no_api_key_requirement() {
        assert_eq!(GifProvider::Local.api_key_env(), None);
    }

    #[test]
    fn legacy_tenor_settings_deserialize_as_klipy() {
        assert_eq!(
            serde_json::from_str::<GifProvider>("\"Tenor\"").unwrap(),
            GifProvider::Klipy
        );
    }

    #[test]
    fn klipy_uses_its_own_api_key_env() {
        assert_eq!(GifProvider::Klipy.api_key_env(), Some(KLIPY_API_KEY_ENV));
    }

    #[test]
    fn online_providers_have_attribution() {
        assert_eq!(GifProvider::Giphy.attribution(), Some("Powered by GIPHY"));
        assert_eq!(GifProvider::Klipy.attribution(), Some("Powered by KLIPY"));
    }

    #[test]
    fn encodes_query_component() {
        assert_eq!(encode_query_component("hello world!"), "hello+world%21");
    }
}
