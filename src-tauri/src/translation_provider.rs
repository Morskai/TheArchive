//! Minimal, bounded translation transport; errors never contain remote payloads or credentials.

use reqwest::blocking::{Client, Request};
use reqwest::header::{AUTHORIZATION, HeaderValue};
use serde_json::{Value, json};
use std::io::Read;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_TEXT_BYTES: usize = 32 * 1024;

#[derive(Clone)]
pub struct TranslatedText {
    pub text: String,
    pub source_language: Option<String>,
}

impl std::fmt::Debug for TranslatedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TranslatedText")
            .field("text", &"[redacted]")
            .field("source_language", &self.source_language)
            .finish()
    }
}

pub trait TranslateProvider {
    fn translate(&self, text: &str) -> Result<TranslatedText, String>;
}

// Intentionally no Debug or Serialize implementation: this object owns an authentication header.
pub struct HttpTranslationProvider {
    provider: &'static str,
    endpoint: &'static str,
    auth: HeaderValue,
    client: Client,
    region: String,
    app_id: String,
    last_request: std::sync::Mutex<Option<std::time::Instant>>,
}

impl HttpTranslationProvider {
    pub fn new(provider: &str, deepl_plan: &str, key: String) -> Result<Self, String> {
        Self::with_settings(
            &crate::translation::TranslationSettings {
                provider: provider.into(),
                deepl_plan: deepl_plan.into(),
                ..Default::default()
            },
            key,
        )
    }

    pub fn with_settings(
        settings: &crate::translation::TranslationSettings,
        key: String,
    ) -> Result<Self, String> {
        let deepl_plan = settings.deepl_plan.as_str();
        let (provider, endpoint) = match settings.provider.as_str() {
            "deepl" => (
                "deepl",
                match deepl_plan {
                    "free" => "https://api-free.deepl.com/v2/translate",
                    "pro" => "https://api.deepl.com/v2/translate",
                    _ => return Err("translation_invalid_plan".into()),
                },
            ),
            "google" => (
                "google",
                "https://translation.googleapis.com/language/translate/v2",
            ),
            "azure" => (
                "azure",
                "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=zh-Hans",
            ),
            "baidu" => (
                "baidu",
                "https://api.fanyi.baidu.com/api/trans/vip/translate",
            ),
            _ => return Err("translation_invalid_provider".into()),
        };
        if settings.azure_region.len() > 64
            || !settings
                .azure_region
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("translation_invalid_region".into());
        }
        if provider == "baidu"
            && (settings.baidu_app_id.is_empty()
                || settings.baidu_app_id.len() > 64
                || !settings.baidu_app_id.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err("translation_app_id_missing".into());
        }
        let key = key.trim();
        if key.is_empty() {
            return Err("translation_key_missing".into());
        }
        if key.len() > 2560 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
            return Err("translation_invalid_key".into());
        }
        let mut auth = HeaderValue::from_str(&if provider == "deepl" {
            format!("DeepL-Auth-Key {key}")
        } else {
            key.to_owned()
        })
        .map_err(|_| "translation_invalid_key".to_owned())?;
        auth.set_sensitive(true);
        static TLS_PROVIDER: std::sync::Once = std::sync::Once::new();
        TLS_PROVIDER.call_once(|| {
            // An existing process crypto provider is also valid; installing twice is harmlessly rejected.
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let client = Client::builder()
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| "translation_network".to_owned())?;
        Ok(Self {
            provider,
            endpoint,
            auth,
            client,
            region: settings.azure_region.clone(),
            app_id: settings.baidu_app_id.clone(),
            last_request: std::sync::Mutex::new(None),
        })
    }

    fn request(&self, text: &str) -> Result<Request, String> {
        if text.trim().is_empty() {
            return Err("translation_empty_text".into());
        }
        if text.len()
            > if self.provider == "baidu" {
                6000
            } else {
                MAX_TEXT_BYTES
            }
        {
            return Err("translation_text_too_large".into());
        }
        let request = self.client.post(self.endpoint);
        let request = match self.provider {
            "deepl" => request
                .header(AUTHORIZATION, self.auth.clone())
                .json(&json!({"text":[text], "target_lang":"ZH-HANS"})),
            "google" => request
                .header("x-goog-api-key", self.auth.clone())
                .json(&json!({"q":[text], "target":"zh-CN", "format":"text"})),
            "azure" => {
                let request = request.header("Ocp-Apim-Subscription-Key", self.auth.clone());
                let request = if self.region.is_empty() || self.region == "global" {
                    request
                } else {
                    request.header("Ocp-Apim-Subscription-Region", &self.region)
                };
                request.json(&json!([{"Text":text}]))
            }
            "baidu" => {
                use md5::{Digest, Md5};
                static NONCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let salt = format!(
                    "{}{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos(),
                    NONCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                );
                let mut digest = Md5::new();
                digest.update(self.app_id.as_bytes());
                digest.update(text.as_bytes());
                digest.update(salt.as_bytes());
                digest.update(self.auth.as_bytes());
                let sign = format!("{:x}", digest.finalize());
                request.form(&[
                    ("q", text),
                    ("from", "auto"),
                    ("to", "zh"),
                    ("appid", self.app_id.as_str()),
                    ("salt", salt.as_str()),
                    ("sign", sign.as_str()),
                ])
            }
            _ => return Err("translation_invalid_provider".into()),
        };
        request.build().map_err(|_| "translation_request".into())
    }

    /// An explicit user connection test translates this fixed public text, never library data.
    pub fn test_connection(&self) -> Result<(), String> {
        self.translate("Hello").map(|_| ())
    }
}

impl TranslateProvider for HttpTranslationProvider {
    fn translate(&self, text: &str) -> Result<TranslatedText, String> {
        let request = self.request(text)?;
        // Serial Baidu batches respect the standard account's one-request-per-second limit.
        if self.provider == "baidu" {
            let mut last = self.last_request.lock().map_err(|_| "translation_failed")?;
            if let Some(at) = *last {
                if let Some(delay) = Duration::from_millis(1050).checked_sub(at.elapsed()) {
                    std::thread::sleep(delay);
                }
            }
            *last = Some(std::time::Instant::now());
        }
        let response = self.client.execute(request).map_err(|error| {
            if error.is_timeout() {
                "translation_timeout"
            } else {
                "translation_network"
            }
            .to_owned()
        })?;
        let status = response.status().as_u16();
        if response
            .content_length()
            .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
        {
            return Err("translation_response_too_large".into());
        }
        let bytes = read_bounded(response)?;
        parse_response(self.provider, status, &bytes)
    }
}

fn read_bounded(reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    reader
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::TimedOut {
                "translation_timeout"
            } else {
                "translation_network"
            }
            .to_owned()
        })?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("translation_response_too_large".into());
    }
    Ok(bytes)
}

fn google_reason(value: &Value, expected: &[&str]) -> bool {
    match value {
        Value::Object(fields) => {
            fields
                .get("reason")
                .and_then(Value::as_str)
                .is_some_and(|reason| expected.contains(&reason))
                || fields.values().any(|value| google_reason(value, expected))
        }
        Value::Array(values) => values.iter().any(|value| google_reason(value, expected)),
        _ => false,
    }
}

fn parse_response(provider: &str, status: u16, bytes: &[u8]) -> Result<TranslatedText, String> {
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err("translation_response_too_large".into());
    }
    if !(200..300).contains(&status) {
        // Read only machine error reasons; never propagate third-party message/error text.
        let google_error = if provider == "google" {
            serde_json::from_slice::<Value>(bytes).ok()
        } else {
            None
        };
        let has_reason = |reasons: &[&str]| {
            google_error
                .as_ref()
                .is_some_and(|value| google_reason(value, reasons))
        };
        let code = if status == 456
            || has_reason(&[
                "dailyLimitExceeded",
                "quotaExceeded",
                "DAILY_LIMIT_EXCEEDED",
                "QUOTA_EXCEEDED",
            ]) {
            "translation_quota"
        } else if status == 429
            || has_reason(&[
                "userRateLimitExceeded",
                "rateLimitExceeded",
                "RATE_LIMIT_EXCEEDED",
            ])
        {
            "translation_rate_limit"
        } else if status == 401
            || status == 403
            || has_reason(&[
                "API_KEY_INVALID",
                "keyInvalid",
                "API_KEY_SERVICE_BLOCKED",
                "API_KEY_HTTP_REFERRER_BLOCKED",
                "API_KEY_IP_ADDRESS_BLOCKED",
            ])
        {
            "translation_auth"
        } else if status >= 500 {
            "translation_server"
        } else if (400..500).contains(&status) {
            "translation_request"
        } else {
            "translation_status"
        };
        return Err(code.into());
    }
    let invalid = || "translation_invalid_response".to_owned();
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if provider == "baidu" {
        if let Some(code) = value.get("error_code").and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        }) {
            return Err(match code.as_str() {
                "52001" => "translation_timeout",
                "52002" => "translation_server",
                "52003" | "54001" | "58000" | "58002" | "90107" => "translation_auth",
                "54003" => "translation_rate_limit",
                "54004" => "translation_quota",
                _ => "translation_request",
            }
            .into());
        }
    }
    let azure = if provider == "azure" {
        let items = value
            .as_array()
            .filter(|items| items.len() == 1)
            .ok_or_else(invalid)?;
        Some(&items[0])
    } else {
        None
    };
    let translations = match provider {
        "deepl" => value.get("translations"),
        "google" => value.get("data").and_then(|data| data.get("translations")),
        "azure" => azure.and_then(|v| v.get("translations")),
        "baidu" => value.get("trans_result"),
        _ => return Err("translation_invalid_provider".into()),
    }
    .and_then(Value::as_array)
    .ok_or_else(invalid)?;
    if translations.len() != 1 {
        return Err(invalid());
    }
    let (text_key, language_key) = if provider == "deepl" {
        ("text", "detected_source_language")
    } else if provider == "baidu" {
        ("dst", "from")
    } else if provider == "azure" {
        ("text", "language")
    } else {
        ("translatedText", "detectedSourceLanguage")
    };
    let text = translations[0]
        .get(text_key)
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
        .trim();
    if text.is_empty() || text.len() > MAX_RESPONSE_BYTES {
        return Err(invalid());
    }
    let language_value = match provider {
        "azure" => azure
            .and_then(|v| v.get("detectedLanguage"))
            .and_then(|v| v.get("language")),
        "baidu" => value.get("from"),
        _ => translations[0].get(language_key),
    };
    let source_language = language_value
        .and_then(Value::as_str)
        .filter(|language| {
            language.len() <= 16
                && !language.is_empty()
                && language
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic() || byte == b'-')
        })
        .map(str::to_owned);
    Ok(TranslatedText {
        text: text.to_owned(),
        source_language,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // A wrong endpoint, authentication location, or extra metadata must fail these tests.
    #[test]
    fn deepl_requests_only_text_and_simplified_target_with_sensitive_auth() {
        for (plan, url) in [
            ("free", "https://api-free.deepl.com/v2/translate"),
            ("pro", "https://api.deepl.com/v2/translate"),
        ] {
            let provider =
                HttpTranslationProvider::new("deepl", plan, "fake-key:fx".into()).unwrap();
            let request = provider.request("A fictional title").unwrap();
            assert_eq!(request.method(), reqwest::Method::POST);
            assert_eq!(request.url().as_str(), url);
            assert_eq!(
                request.headers()["authorization"],
                "DeepL-Auth-Key fake-key:fx"
            );
            assert!(request.headers()["authorization"].is_sensitive());
            let body: serde_json::Value =
                serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
            assert_eq!(
                body,
                serde_json::json!({"text":["A fictional title"],"target_lang":"ZH-HANS"})
            );
        }
    }

    #[test]
    fn google_key_is_sensitive_header_and_never_in_url_or_body() {
        let provider =
            HttpTranslationProvider::new("google", "free", "fake-google-key".into()).unwrap();
        let request = provider.request("A fictional title").unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://translation.googleapis.com/language/translate/v2"
        );
        assert_eq!(request.headers()["x-goog-api-key"], "fake-google-key");
        assert!(request.headers()["x-goog-api-key"].is_sensitive());
        let body: serde_json::Value =
            serde_json::from_slice(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"q":["A fictional title"],"target":"zh-CN","format":"text"})
        );
        assert!(!format!("{request:?}").contains("fake-google-key"));
    }

    #[test]
    fn parses_both_official_single_translation_responses() {
        let deepl = parse_response("deepl", 200, br#"{"translations":[{"detected_source_language":"EN","text":"translated title","billed_characters":14,"model_type_used":"quality_optimized"}]}"#).unwrap();
        assert_eq!(deepl.text, "translated title");
        assert_eq!(deepl.source_language.as_deref(), Some("EN"));
        let google = parse_response("google", 200, br#"{"data":{"translations":[{"translatedText":"translated title","detectedSourceLanguage":"en"}]}}"#).unwrap();
        assert_eq!(google.text, "translated title");
        assert_eq!(google.source_language.as_deref(), Some("en"));
    }

    #[test]
    fn provider_failures_are_fixed_codes_without_remote_message_or_secret() {
        let body = br#"{"error":{"message":"fake-secret echoed with private title/path","errors":[{"reason":"forbidden"}]}}"#;
        for (status, code) in [
            (400, "translation_request"),
            (401, "translation_auth"),
            (403, "translation_auth"),
            (429, "translation_rate_limit"),
            (456, "translation_quota"),
            (500, "translation_server"),
            (302, "translation_status"),
        ] {
            assert_eq!(parse_response("google", status, body).unwrap_err(), code);
        }
        assert_eq!(
            parse_response(
                "google",
                403,
                br#"{"error":{"errors":[{"reason":"dailyLimitExceeded"}]}}"#
            )
            .unwrap_err(),
            "translation_quota"
        );
        assert_eq!(
            parse_response(
                "google",
                403,
                br#"{"error":{"errors":[{"reason":"userRateLimitExceeded"}]}}"#
            )
            .unwrap_err(),
            "translation_rate_limit"
        );
        assert_eq!(
            parse_response(
                "google",
                400,
                br#"{"error":{"details":[{"reason":"API_KEY_INVALID"}]}}"#
            )
            .unwrap_err(),
            "translation_auth"
        );
    }

    #[test]
    fn malformed_empty_or_multiple_results_cannot_be_saved_as_a_title() {
        for body in [
            b"not JSON".as_slice(),
            br#"{"translations":[]}"#,
            br#"{"translations":[{"text":" "}]}"#,
            br#"{"translations":[{"text":"one"},{"text":"two"}]}"#,
            br#"{"translations":[{"text":12}]}"#,
        ] {
            assert_eq!(
                parse_response("deepl", 200, body).unwrap_err(),
                "translation_invalid_response"
            );
        }
        assert_eq!(parse_response("google",200,br#"{"data":{"translations":[{"translatedText":"", "detectedSourceLanguage":"en"}]}}"#).unwrap_err(), "translation_invalid_response");
    }

    #[test]
    fn bounded_response_reader_rejects_large_body_and_io_error() {
        assert_eq!(
            read_bounded(std::io::Cursor::new(vec![b'x'; MAX_RESPONSE_BYTES + 1])).unwrap_err(),
            "translation_response_too_large"
        );
        struct Failed;
        impl std::io::Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fake secret"))
            }
        }
        assert_eq!(read_bounded(Failed).unwrap_err(), "translation_network");
    }

    #[test]
    fn invalid_settings_and_text_fail_locally_without_echoing_key() {
        assert_eq!(
            HttpTranslationProvider::new("https://bad.example", "free", "fake-key".into())
                .err()
                .unwrap(),
            "translation_invalid_provider"
        );
        assert_eq!(
            HttpTranslationProvider::new("deepl", "other", "fake-key".into())
                .err()
                .unwrap(),
            "translation_invalid_plan"
        );
        assert_eq!(
            HttpTranslationProvider::new("deepl", "free", " ".into())
                .err()
                .unwrap(),
            "translation_key_missing"
        );
        assert_eq!(
            HttpTranslationProvider::new("google", "free", "fake\nkey".into())
                .err()
                .unwrap(),
            "translation_invalid_key"
        );
        let provider = HttpTranslationProvider::new("google", "free", "fake-key".into()).unwrap();
        assert_eq!(
            provider.request(" ").err().unwrap(),
            "translation_empty_text"
        );
        assert_eq!(
            provider
                .request(&"x".repeat(MAX_TEXT_BYTES + 1))
                .err()
                .unwrap(),
            "translation_text_too_large"
        );
    }
}

#[cfg(test)]
mod additional_provider_tests {
    use super::*;
    #[test]
    fn azure_and_baidu_official_responses_are_accepted() {
        let azure = parse_response("azure",200,br#"[{"detectedLanguage":{"language":"ja"},"translations":[{"text":"translated title","to":"zh-Hans"}]}]"#).unwrap();
        assert_eq!(azure.text, "translated title");
        assert_eq!(azure.source_language.as_deref(), Some("ja"));
        let baidu = parse_response(
            "baidu",
            200,
            br#"{"from":"jp","to":"zh","trans_result":[{"src":"title","dst":"translated title"}]}"#,
        )
        .unwrap();
        assert_eq!(baidu.text, "translated title");
        assert_eq!(baidu.source_language.as_deref(), Some("jp"));
    }
    #[test]
    fn baidu_errors_never_echo_remote_details() {
        for (code, expected) in [
            ("54001", "translation_auth"),
            ("54003", "translation_rate_limit"),
            ("54004", "translation_quota"),
            ("52001", "translation_timeout"),
        ] {
            let body =
                format!(r#"{{"error_code":"{code}","error_msg":"fake-secret private-title"}}"#);
            assert_eq!(
                parse_response("baidu", 200, body.as_bytes()).unwrap_err(),
                expected
            );
        }
    }
}

#[cfg(test)]
mod additional_request_tests {
    use super::*;
    use crate::translation::TranslationSettings;
    #[test]
    fn azure_auth_and_region_are_headers_and_request_contains_only_title() {
        for region in ["", "global", "eastasia"] {
            let settings = TranslationSettings {
                provider: "azure".into(),
                azure_region: region.into(),
                ..Default::default()
            };
            let provider =
                HttpTranslationProvider::with_settings(&settings, "synthetic-azure-key".into())
                    .unwrap();
            let request = provider.request("fictional title").unwrap();
            assert_eq!(
                request.url().as_str(),
                "https://api.cognitive.microsofttranslator.com/translate?api-version=3.0&to=zh-Hans"
            );
            assert_eq!(
                request.headers()["ocp-apim-subscription-key"],
                "synthetic-azure-key"
            );
            assert!(request.headers()["ocp-apim-subscription-key"].is_sensitive());
            assert_eq!(
                request
                    .headers()
                    .get("ocp-apim-subscription-region")
                    .map(|v| v.to_str().unwrap()),
                if region == "eastasia" {
                    Some(region)
                } else {
                    None
                }
            );
            assert_eq!(
                serde_json::from_slice::<Value>(request.body().unwrap().as_bytes().unwrap())
                    .unwrap(),
                json!([{"Text":"fictional title"}])
            );
            assert!(!format!("{request:?}").contains("synthetic-azure-key"));
        }
    }
    #[test]
    fn baidu_signs_utf8_post_without_transmitting_secret_or_paths() {
        let settings = TranslationSettings {
            provider: "baidu".into(),
            baidu_app_id: "123456789".into(),
            ..Default::default()
        };
        let provider =
            HttpTranslationProvider::with_settings(&settings, "synthetic-baidu-key".into())
                .unwrap();
        let request = provider.request("fictional 日文 & title").unwrap();
        assert_eq!(
            request.url().as_str(),
            "https://api.fanyi.baidu.com/api/trans/vip/translate"
        );
        assert_eq!(
            request.headers()["content-type"],
            "application/x-www-form-urlencoded"
        );
        let body = std::str::from_utf8(request.body().unwrap().as_bytes().unwrap()).unwrap();
        assert!(!body.contains("synthetic-baidu-key"));
        let url = reqwest::Url::parse(&format!("https://synthetic.invalid/?{body}")).unwrap();
        let fields: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(fields.len(), 6);
        assert_eq!(fields["q"], "fictional 日文 & title");
        assert_eq!(fields["from"], "auto");
        assert_eq!(fields["to"], "zh");
        assert_eq!(fields["appid"], "123456789");
        assert_eq!(fields["sign"].len(), 32);
        assert_ne!(
            fields["salt"],
            reqwest::Url::parse(&format!(
                "https://synthetic.invalid/?{}",
                std::str::from_utf8(
                    provider
                        .request("other")
                        .unwrap()
                        .body()
                        .unwrap()
                        .as_bytes()
                        .unwrap()
                )
                .unwrap()
            ))
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "salt")
            .unwrap()
            .1
        );
        assert_eq!(
            provider.request(&"x".repeat(6001)).err().unwrap(),
            "translation_text_too_large"
        );
        assert_eq!(
            HttpTranslationProvider::new("baidu", "free", "key".into())
                .err()
                .unwrap(),
            "translation_app_id_missing"
        );
    }
}
