//! User-requested Wikipedia and translation lookups. The worker has no book or library path.
use std::io::{self, Read, Write};
use std::ops::Range;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

pub(crate) const MAX_TEXT_BYTES: usize = 6000;
const MAX_RESPONSE: u64 = 384 * 1024;
const MAX_RESULT: usize = 96 * 1024;
const JOB_LIMIT: Duration = Duration::from_secs(45);
const USER_AGENT: &str = concat!(
    "PrimitiveOS-Reader/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/PrimitiveOS/kindle-primitive-reader)"
);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) enum Service {
    #[default]
    Wikipedia,
    Google,
    MyMemory,
}

impl Service {
    pub fn name(self) -> &'static str {
        match self {
            Self::Wikipedia => "Wikipedia",
            Self::Google => "Google Translate",
            Self::MyMemory => "MyMemory",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct Request {
    pub service: Service,
    pub text: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub full_article: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Article {
    pub title: String,
    pub text: String,
}

pub(crate) fn language(code: &str) -> bool {
    let mut parts = code.split('-');
    let first = parts.next().unwrap_or("");
    (2..=3).contains(&first.len())
        && first.bytes().all(|c| c.is_ascii_lowercase())
        && parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|c| c.is_ascii_alphanumeric()))
        && code.len() <= 24
}

impl Request {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.text.trim().is_empty() {
            return Err("Enter a word or select readable text.".into());
        }
        if self.text.len() > MAX_TEXT_BYTES {
            return Err("This page is too long. Select a shorter passage.".into());
        }
        if !language(&self.to) {
            return Err("Enter a target language code, such as en or vi.".into());
        }
        if self.service == Service::Wikipedia {
            if self.text.chars().count() > 150 {
                return Err("Choose a shorter Wikipedia search term.".into());
            }
        } else {
            if self.full_article {
                return Err("Choose a Wikipedia article first.".into());
            }
            if !(language(&self.from) || (self.service == Service::Google && self.from == "auto")) {
                return Err("Choose the source language before using this provider.".into());
            }
            if self.from == self.to {
                return Err("Choose two different translation languages.".into());
            }
        }
        Ok(())
    }
}

fn translation_chunks(text: &str, limit: usize) -> Vec<Range<usize>> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = text.floor_char_boundary((start + limit).min(text.len()));
        if end < text.len() {
            if let Some((i, c)) = text[start..end]
                .char_indices()
                .rfind(|(_, c)| c.is_whitespace())
            {
                end = start + i + c.len_utf8();
            }
        }
        chunks.push(start..end);
        start = end;
    }
    chunks
}

fn endpoint(request: &Request, text: &str) -> Url {
    let base = match request.service {
        Service::Wikipedia => format!(
            "https://{}.wikipedia.org/w/api.php",
            request.to.to_ascii_lowercase()
        ),
        Service::Google => "https://translate.googleapis.com/translate_a/single".into(),
        Service::MyMemory => "https://api.mymemory.translated.net/get".into(),
    };
    let mut url = Url::parse(&base).expect("validated provider host");
    let mut params = url.query_pairs_mut();
    match request.service {
        Service::Wikipedia => {
            params.extend_pairs([
                ("action", "query"),
                ("format", "json"),
                ("formatversion", "2"),
                ("prop", "extracts"),
                ("explaintext", "1"),
                ("redirects", "1"),
            ]);
            if request.full_article {
                params.append_pair("titles", text);
            } else {
                params.extend_pairs([
                    ("generator", "search"),
                    ("gsrsearch", text),
                    ("gsrnamespace", "0"),
                    ("gsrlimit", "3"),
                    ("exintro", "1"),
                    ("exlimit", "3"),
                ]);
            }
        }
        Service::Google => {
            params.extend_pairs([
                ("client", "gtx"),
                ("sl", &request.from),
                ("tl", &request.to),
                ("dt", "t"),
                ("q", text),
            ]);
        }
        Service::MyMemory => {
            params
                .append_pair("langpair", &format!("{}|{}", request.from, request.to))
                .append_pair("q", text);
        }
    }
    drop(params);
    url
}

fn wikipedia(json: &Value) -> Result<Vec<Article>, String> {
    if let Some(code) = json.pointer("/error/code").and_then(Value::as_str) {
        return Err(match code {
            "maxlag" | "ratelimited" => "Wikipedia is busy. Try again later.",
            _ => "Wikipedia could not complete the search.",
        }
        .into());
    }
    let mut pages: Vec<_> = json
        .pointer("/query/pages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .collect();
    pages.sort_by_key(|p| p["index"].as_u64().unwrap_or(0));
    let mut articles = Vec::new();
    for page in pages.into_iter().take(3) {
        if let (Some(title), Some(text)) = (page["title"].as_str(), page["extract"].as_str())
            && !text.trim().is_empty()
        {
            articles.push(Article {
                title: title.to_owned(),
                text: text.to_owned(),
            });
        }
    }
    if articles.is_empty() {
        return Err("Wikipedia has no article for this search in that language.".into());
    }
    Ok(articles)
}

fn translation(json: &Value, service: Service) -> Result<String, String> {
    let text = if service == Service::Google {
        json.pointer("/0")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|s| s.get(0)?.as_str())
            .collect::<String>()
    } else {
        if json["quotaFinished"].as_bool() == Some(true)
            || json["responseStatus"].as_u64() == Some(429)
        {
            return Err("MyMemory's translation quota is in use. Try later.".into());
        }
        let status = &json["responseStatus"];
        if status.as_u64() != Some(200) && status.as_str() != Some("200") {
            return Err("MyMemory could not translate this language pair.".into());
        }
        json.pointer("/responseData/translatedText")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    if text.trim().is_empty() {
        return Err(format!("{} returned no translation.", service.name()));
    }
    Ok(text)
}

fn resolve(
    request: &Request,
    mut fetch: impl FnMut(Url) -> Result<Value, String>,
) -> Result<Vec<Article>, String> {
    request.validate()?;
    let articles = match request.service {
        Service::Wikipedia => wikipedia(&fetch(endpoint(request, &request.text))?)?,
        service => {
            let limit = if service == Service::MyMemory {
                500
            } else {
                1400
            };
            let mut result = String::new();
            for range in translation_chunks(&request.text, limit) {
                let chunk = &request.text[range];
                let content = chunk.trim();
                if content.is_empty() {
                    result.push_str(chunk);
                    continue;
                }
                let received = translation(&fetch(endpoint(request, content))?, service)?;
                let prefix = chunk.len() - chunk.trim_start().len();
                let end = chunk.trim_end().len();
                result.push_str(&chunk[..prefix]);
                result.push_str(received.trim());
                result.push_str(&chunk[end..]);
                if result.len() > MAX_RESULT {
                    return Err("The translation exceeds the display limit.".into());
                }
            }
            vec![Article {
                title: service.name().into(),
                text: result,
            }]
        }
    };
    if articles
        .iter()
        .map(|a| a.title.len() + a.text.len())
        .sum::<usize>()
        > MAX_RESULT
    {
        return Err(
            "The article is too long for this reader. Open its introduction instead.".into(),
        );
    }
    Ok(articles)
}

fn live_fetch(url: Url, agent: &ureq::Agent) -> Result<Value, String> {
    let mut response = agent.get(url.as_str()).call().map_err(|_| {
        "Could not reach the provider over HTTPS. Check Wi-Fi and the device clock.".to_owned()
    })?;
    match response.status().as_u16() {
        200 => (),
        429 => return Err("The provider is rate limiting requests. Try later or choose another translation provider.".into()),
        other => return Err(format!("The provider returned HTTP {other}.")),
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE)
        .read_to_vec()
        .map_err(|_| "The provider response is too large or incomplete.".to_owned())?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "The provider returned an unreadable response.".into())
}

/// Worker protocol on stdin/stdout keeps user text out of process arguments and diagnostic logs.
pub fn worker() -> crate::Result<()> {
    let request: Request =
        serde_json::from_reader(io::stdin().lock().take(MAX_TEXT_BYTES as u64 * 6 + 512))?;
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(15)))
            .build(),
    );
    let reply = resolve(&request, |url| live_fetch(url, &agent));
    serde_json::to_writer(io::stdout().lock(), &reply)?;
    Ok(())
}

pub(crate) struct Job {
    child: Child,
    receiver: Receiver<Result<Vec<Article>, String>>,
    pub poll_at: Instant,
    end: Instant,
}

impl Job {
    pub fn start(request: &Request) -> crate::Result<Self> {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--lookup-worker");
        Self::spawn(command, request)
    }

    pub(crate) fn spawn(mut command: Command, request: &Request) -> crate::Result<Self> {
        request.validate()?;
        let payload = serde_json::to_vec(request)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut input = child.stdin.take().expect("piped stdin");
        let output = child.stdout.take().expect("piped stdout");
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("lookup-result".into())
            .spawn(move || {
                let reply = (|| {
                    input.write_all(&payload)?;
                    drop(input);
                    serde_json::from_reader(output.take(MAX_RESULT as u64 * 6 + 4096))
                        .map_err(io::Error::other)
                })()
                .unwrap_or_else(|_| Err("The online lookup ended before a result arrived.".into()));
                let _ = sender.send(reply);
            });
        if let Err(error) = thread {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.into());
        }
        let now = Instant::now();
        Ok(Self {
            child,
            receiver,
            poll_at: now + Duration::from_millis(120),
            end: now + JOB_LIMIT,
        })
    }

    pub fn poll(&mut self, now: Instant) -> Option<Result<Vec<Article>, String>> {
        self.poll_at = now + Duration::from_millis(120);
        match self.receiver.try_recv() {
            Ok(value) => Some(value),
            Err(TryRecvError::Disconnected) => Some(Err("The lookup worker stopped.".into())),
            Err(TryRecvError::Empty) if now >= self.end => {
                Some(Err("The online lookup timed out.".into()))
            }
            Err(TryRecvError::Empty) => None,
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(service: Service, text: &str) -> Request {
        Request {
            service,
            text: text.into(),
            from: "en".into(),
            to: "vi".into(),
            full_article: false,
        }
    }

    #[test]
    fn wikipedia_encodes_the_exact_query_and_fetches_full_articles_by_title() {
        let mut r = request(Service::Wikipedia, "Rock & roll + C#");
        let response = json!({"query":{"pages":[{"title":"Second","index":2,"extract":"Two."},{"title":"First","index":1,"extract":"One."}]}});
        let articles = resolve(&r, |url| {
            assert_eq!(url.host_str(), Some("vi.wikipedia.org"));
            assert!(
                url.query_pairs()
                    .any(|(k, v)| k == "gsrsearch" && v == r.text.as_str())
            );
            Ok(response.clone())
        })
        .unwrap();
        assert_eq!(articles[0].title, "First");
        r.full_article = true;
        let url = endpoint(&r, "First");
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "titles" && v == "First")
        );
        assert!(!url.query_pairs().any(|(k, _)| k == "exintro"));
        r.to = "en.wikipedia.org.example".into();
        assert!(resolve(&r, |_| panic!("invalid host must not be contacted")).is_err());
        assert!(wikipedia(&json!({"query":{"pages":[]}})).is_err());
    }

    #[test]
    fn translation_preserves_unicode_and_chunk_boundaries_and_never_switches_providers() {
        let text = format!(
            "Hello, reader.\n{} {}",
            "眠".repeat(220),
            "bến sông".repeat(60)
        );
        let r = request(Service::MyMemory, &text);
        let mut calls = 0;
        let result = resolve(&r, |url| {
            calls += 1;
            assert_eq!(url.host_str(), Some("api.mymemory.translated.net"));
            let q = url
                .query_pairs()
                .find(|(k, _)| k == "q")
                .unwrap()
                .1
                .into_owned();
            assert!(q.len() <= 500);
            Ok(json!({"responseStatus":200,"responseData":{"translatedText":q}}))
        })
        .unwrap();
        assert!(calls > 1);
        assert_eq!(result[0].text, text);
        let r = request(Service::Google, "Two sentences.");
        assert_eq!(
            resolve(&r, |_| Ok(json!([[["Deux ", null], ["phrases.", null]]]))).unwrap()[0].text,
            "Deux phrases."
        );
        let failure = resolve(&r, |url| {
            assert_eq!(url.host_str(), Some("translate.googleapis.com"));
            Err("Rate limited".into())
        })
        .unwrap_err();
        assert_eq!(failure, "Rate limited");
        let mut invalid = request(Service::MyMemory, "A");
        invalid.from = "auto".into();
        assert!(resolve(&invalid, |_| panic!("needs source language")).is_err());
    }

    #[test]
    fn cancelling_reaps_the_lookup_process_and_the_ui_can_timeout() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "exec sleep 30"]);
        let mut job = Job::spawn(cmd, &request(Service::Wikipedia, "Reader")).unwrap();
        let start = Instant::now();
        assert!(job.poll(start).is_none());
        assert!(
            job.poll(start + JOB_LIMIT + Duration::from_secs(1))
                .unwrap()
                .is_err()
        );
        drop(job);
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
