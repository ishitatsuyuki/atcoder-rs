#![feature(conservative_impl_trait)]

extern crate cookie;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate percent_encoding;
extern crate reqwest;
extern crate select;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tokio_core;

mod revel_deserialize;

use std::fmt;
use futures::{future, Future};
use tokio_core::reactor::Handle;
use reqwest::unstable::async::Client;
use reqwest::header::{Cookie, SetCookie};
use reqwest::{RedirectPolicy, StatusCode};
use cookie::Cookie as CookieParser;
use select::document::Document;
use select::node::Node;
use select::predicate::{Attr, Element, Name, Text, Predicate};

use revel_deserialize::RevelFlash;

const API_BASE: &str = "https://beta.atcoder.jp";

error_chain! {
    foreign_links {
        ReqError(::reqwest::Error);
        CookieError(::cookie::ParseError);
        NumError(::std::num::ParseIntError);
    }

    errors {
        Unauthorized(m: String) {
            description("Authentication failed")
            display("Authentication failed: {}", m)
        }

        BadStatus(c: StatusCode) {
            description("Unexpected status code")
            display("Unexpected status code: {}", c)
        }

        InvalidResponse(m: String) {
            description("Unexpected response from server")
            display("Unexpected response from server: {}", m)
        }

        NoSuchTask {
            description("No task matched supplied prefix")
        }

        NoSuchLanguage {
            description("No language matched supplied prefix")
        }
    }
}

/// The session returned from the server. This wraps the server-side
/// implementation details to allow the storage to change from signed
/// cookies to a more robust one.
#[derive(Serialize, Deserialize, Debug)]
pub struct Authentication {
    session: String,
}

fn csrf_token(document: &Document) -> Option<String> {
    let mut candidate = document.find(Attr("name", "csrf_token"));
    if let Some(val) = candidate.next().and_then(|node| node.attr("value")) {
        // Sanity check
        for node in candidate {
            if node.attr("value") != Some(val) {
                return None;
            }
        }
        Some(val.to_owned())
    } else {
        None
    }
}

fn get_api(
    endpoint: String,
    auth: Option<Authentication>,
    client: &Client,
) -> impl Future<Item=(Authentication, Vec<u8>), Error=Error> {
    future::lazy({
        let client = client.clone();
        move || -> Result<_> {
            let mut request = client.get(&endpoint)?;
            if let Some(auth) = auth {
                let mut cookie = Cookie::new();
                cookie.append("REVEL_SESSION", auth.session);
                request.header(cookie);
            }
            Ok(request.send().from_err())
        }
    }).flatten()
        .and_then(|response| -> Result<_> {
            ensure!(
                response.status() == StatusCode::Ok,
                ErrorKind::BadStatus(response.status())
            );
            let cookies = response.headers().get::<SetCookie>().cloned().ok_or_else(||
                                                                                        ErrorKind::InvalidResponse("No cookies received".to_owned()),
            )?;
            for raw_cookie in &**cookies {
                let cookie = CookieParser::parse(&**raw_cookie).chain_err(|| {
                    ErrorKind::InvalidResponse("Failed to parse cookie".to_owned())
                })?;
                if cookie.name() == "REVEL_SESSION" {
                    return Ok((
                        Authentication { session: cookie.value().to_owned() },
                        response,
                    ));
                }
            }
            bail!(ErrorKind::InvalidResponse(
                "No \"REVEL_SESSION\" cookie found".to_owned(),
            ));
        })
        .and_then(|(auth, mut response)| {
            future::ok(auth).join(response.body_resolved().from_err())
        })
}

fn get_post<F: FnOnce(&Document) -> Result<Vec<(&'static str, String)>> + 'static>(
    get: String,
    post: Option<String>,
    form_data: F,
    auth: Option<Authentication>,
    client: &Client,
) -> impl Future<Item=(Option<String>, Authentication), Error=Error> + 'static {
    let post = post.unwrap_or(get.clone());
    get_api(get, auth, client)
        .and_then(move |(auth, body)| {
            let document = Document::from(::std::str::from_utf8(&body).chain_err(|| {
                ErrorKind::InvalidResponse("Cannot decode response".to_owned())
            })?);
            let mut form = form_data(&document)?;
            form.push((
                "csrf_token",
                csrf_token(&document).ok_or_else(|| ErrorKind::InvalidResponse(
                    "Cannot find csrf_token".to_owned(),
                ))?,
            ));
            Ok((auth, form))
        })
        .and_then({
            let client = client.clone();
            move |(auth, form)| {
                let mut cookie = Cookie::new();
                cookie.append("REVEL_SESSION", auth.session);
                let mut request = client.post(&post)?;
                request.header(cookie);
                request.form(&form)?;
                Ok(request.send().from_err())
            }
        })
        .flatten()
        .and_then(|response| {
            ensure!(
                response.status() == StatusCode::Found,
                ErrorKind::BadStatus(response.status())
            );
            let cookies = response.headers().get::<SetCookie>().ok_or_else(||
                                                                               ErrorKind::InvalidResponse("No cookie received".to_owned()),
            )?;
            let mut result = None;
            let mut success = None;
            for raw_cookie in &**cookies {
                let cookie = CookieParser::parse(&**raw_cookie).chain_err(|| {
                    ErrorKind::InvalidResponse("Failed to parse cookie".to_owned())
                })?;
                if cookie.name() == "REVEL_SESSION" {
                    result = Some(Authentication { session: cookie.value().to_owned() });
                }
                if cookie.name() == "REVEL_FLASH" {
                    let flash: RevelFlash = revel_deserialize::from_bytes(
                        cookie.value().as_bytes(),
                    ).chain_err(|| {
                        ErrorKind::InvalidResponse("Failed to decode \"REVEL_FLASH\"".to_owned())
                    })?;
                    if let Some(err) = flash.error {
                        bail!(ErrorKind::Unauthorized(err))
                    } else {
                        success = flash.success;
                    }
                }
            }
            result
                .ok_or_else(||
                                ErrorKind::InvalidResponse("No \"REVEL_SESSION\" cookie found".to_owned())
                                    .into(),
                )
                .map(|auth| (success, auth))
        })
}

pub fn create_client(handle: &Handle) -> Result<Client> {
    //! Build a reqwest client for API usage.
    Client::builder()?
        .redirect(RedirectPolicy::none())
        .build(handle)
        .map_err(Error::from)
}

pub fn login(
    username: &str,
    password: &str,
    client: &Client,
) -> impl Future<Item=(Authentication, Option<String>), Error=Error> {
    //! Login with username and password.
    let form = vec![
        ("username", username.to_owned()),
        ("password", password.to_owned()),
    ];
    get_post(
        format!("{}/login/", API_BASE),
        None,
        move |_| Ok(form),
        None,
        client,
    ).map(|(message, auth)| (auth, message))
}

pub fn logout(
    auth: Authentication,
    client: &Client,
) -> impl Future<Item=Option<String>, Error=Error> {
    //! Logout, making the current `Authentication` no longer usable.
    //! # Server-side implementation details
    //! The server framework, Revel, currently doesn't store sessions in
    //! database, and thus has no ability to invalidate a token other than
    //! timing out. Thus, this cannot be used for safety purposes.
    get_post(
        format!("{}", API_BASE),
        Some(format!("{}/logout/", API_BASE)),
        |_| Ok(vec![]),
        Some(auth),
        client,
    ).map(|(message, _)| message)
}

pub fn join(
    contest: &str,
    auth: Authentication,
    client: &Client,
) -> impl Future<Item=(Option<String>, Authentication), Error=Error> {
    //! Join a contest.
    get_post(
        format!("{}/contests/{}/", API_BASE, contest),
        Some(format!("{}/contests/{}/register/", API_BASE, contest)),
        |_| Ok(vec![]),
        Some(auth),
        client,
    )
}

pub fn submit(
    contest: &str,
    task: &str,
    lang: &str,
    source: String,
    auth: Authentication,
    client: &Client,
) -> impl Future<Item=(Option<String>, Authentication), Error=Error> {
    //! Submit a resolution.
    //! The `task` and `lang` parameters are patterns, and are matched against
    //! the start of the options.
    get_post(
        format!("{}/contests/{}/submit/", API_BASE, contest),
        None,
        {
            let task = task.to_lowercase();
            let lang = lang.to_lowercase();
            move |doc| {
                let mut tasks = doc.find(Attr("id", "select-task").descendant(Name("option")));
                let task_id = tasks
                    .find(|t| t.inner_html().to_lowercase().starts_with(&task))
                    .and_then(|n| n.attr("value"))
                    .ok_or_else(|| ErrorKind::NoSuchTask)?;
                let select_lang = format!("select-lang-{}", task_id);
                let mut langs = doc.find(Attr("id", &*select_lang).descendant(Name("option")));
                let lang_id = langs
                    .find(|t| t.inner_html().to_lowercase().starts_with(&lang))
                    .and_then(|n| n.attr("value"))
                    .ok_or_else(|| ErrorKind::NoSuchLanguage)?;
                Ok(vec![
                    ("data.TaskScreenName", task_id.to_owned()),
                    ("data.LanguageId", lang_id.to_owned()),
                    ("sourceCode", source),
                ])
            }
        },
        Some(auth),
        client,
    )
}

pub enum SubmissionResult {
    Pass,
    Fail,
    Timeout,
    RuntimeError,
    CompileError,
}

pub enum SubmissionStatus {
    Pending,
    InProgress {
        current: usize,
        total: usize,
        status: SubmissionResult,
    },
    Done(SubmissionResult)
}

impl fmt::Display for SubmissionResult {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use SubmissionResult::*;
        f.write_str(match *self {
            Pass => "Pass",
            Fail => "Fail",
            Timeout => "Timeout",
            RuntimeError => "Runtime error",
            CompileError => "Compile error",
        })
    }
}

impl fmt::Display for SubmissionStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use SubmissionStatus::*;
        match *self {
            Pending => f.write_str("Pending"),
            InProgress { current, total, ref status } => write!(f, "{}/{} {}", current, total, status),
            Done(ref status) => status.fmt(f),
        }
    }
}

pub struct Submission {
    pub id: String,
    pub timestamp: String,
    pub task: String,
    pub user: String,
    pub lang: String,
    pub score: usize,
    pub code_length: usize,
    pub status: SubmissionStatus,
    /// Execution time in ms
    pub time: Option<usize>,
    /// Peak memory in KB
    pub memory: Option<usize>,
}

// TODO: filter and all submissions
pub fn submissions(
    contest: &str,
    auth: Option<Authentication>,
    client: &Client,
) -> impl Future<Item=(Vec<Submission>, Authentication), Error=Error> {
    get_api(
        format!("{}/contests/{}/submissions/me/", API_BASE, contest),
        auth,
        client,
    ).and_then(|(auth, body)| {
        let document = Document::from(::std::str::from_utf8(&body).chain_err(|| {
            ErrorKind::InvalidResponse("Cannot decode response".to_owned())
        })?);
        let result_tbody = document
            .find(Name("table").descendant(Name("tbody")))
            .next()
            .ok_or_else(|| ErrorKind::InvalidResponse(
                "No result table found".to_owned(),
            ))?;
        let results = result_tbody.children().filter(|e| e.is(Element)).map(|row| {
            fn next_text<'a, T: Iterator<Item=Node<'a>>>(iter: &mut T) -> Result<&'a str> {
                Ok(iter.next()
                    .ok_or_else(|| ErrorKind::InvalidResponse(
                        "Table layout mismatch".to_owned(),
                    ))?
                    .find(Text)
                    .map(|e| e.as_text().unwrap().trim())
                    .filter(|s| !s.is_empty())
                    .next()
                    .ok_or_else(|| ErrorKind::InvalidResponse(
                        "Table layout mismatch".to_owned(),
                    ))?)
            }
            let mut col_iter = row.children().filter(|e| e.is(Name("td")));
            let timestamp = next_text(&mut col_iter)?.to_owned();
            // TODO: chrono parse
            let task = next_text(&mut col_iter)?.to_owned();
            // TODO: internal id
            let user = next_text(&mut col_iter)?.to_owned();
            let lang = next_text(&mut col_iter)?.to_owned();
            let score = next_text(&mut col_iter)?.parse()?;
            let code_length_str = next_text(&mut col_iter)?;
            let byte_pattern = " Byte";
            if !code_length_str.ends_with(byte_pattern) {
                return Err(
                    ErrorKind::InvalidResponse("Code size pattern mismatch".to_owned()).into(),
                );
            }
            let code_length = code_length_str[..code_length_str.len() - byte_pattern.len()].parse()?;

            fn parse_result(text: &str) -> Option<SubmissionResult> {
                use SubmissionResult::*;
                match text {
                    "AC" => Some(Pass),
                    "WA" => Some(Fail),
                    "TLE" => Some(Timeout),
                    "RE" => Some(RuntimeError),
                    "CE" => Some(CompileError),
                    _ => None
                }
            }
            fn parse_status(text: &str) -> Option<SubmissionStatus> {
                use SubmissionStatus::*;
                if let Some(x) = parse_result(text) {
                    Some(Done(x))
                } else if text == "WJ" {
                    Some(Pending)
                } else {
                    let slash = text.find('/')?;
                    let current = text[..slash].parse().ok()?;
                    if let Some(space) = text.find(' ') {
                        if space < slash {
                            None
                        } else {
                            let total = text[slash + 1..space].parse().ok()?;
                            let status = parse_result(&text[space + 1..])?;
                            Some(InProgress { current, total, status })
                        }
                    } else {
                        let total = text[slash + 1..].parse().ok()?;
                        Some(InProgress { current, total, status: SubmissionResult::Pass })
                    }
                }
            }
            let status_node = col_iter.next().ok_or_else(|| {
                ErrorKind::InvalidResponse("Table layout mismatch".to_owned())
            })?;
            let status = parse_status(next_text(&mut status_node.children())?)
                .ok_or_else(|| ErrorKind::InvalidResponse("Status parse failure".to_owned()))?;

            let (time, memory) =
                if status_node.attr("colspan") != Some("3") {
                    let time_str = next_text(&mut col_iter)?;
                    let ms_pattern = " ms";
                    if !time_str.ends_with(ms_pattern) {
                        return Err(
                            ErrorKind::InvalidResponse("Execution time pattern mismatch".to_owned()).into(),
                        );
                    }
                    let time = time_str[..time_str.len() - ms_pattern.len()].parse()?;

                    let memory_str = next_text(&mut col_iter)?;
                    let kb_pattern = " KB";
                    if !memory_str.ends_with(kb_pattern) {
                        return Err(
                            ErrorKind::InvalidResponse("Memory usage pattern mismatch".to_owned()).into(),
                        );
                    }
                    let memory = memory_str[..memory_str.len() - kb_pattern.len()].parse()?;
                    (Some(time), Some(memory))
                } else { (None, None) };

            let id_href = col_iter.next().ok_or_else(|| ErrorKind::InvalidResponse(
                "Table layout mismatch".to_owned(),
            ))?
                .find(Name("a")).next().ok_or_else(|| ErrorKind::InvalidResponse(
                "Table layout mismatch".to_owned(),
            ))?
                .attr("href").ok_or_else(|| ErrorKind::InvalidResponse(
                "Table layout mismatch".to_owned(),
            ))?;
            let id = id_href[id_href.rfind('/').ok_or_else(|| ErrorKind::InvalidResponse(
                "Table layout mismatch".to_owned(),
            ))? + 1..].to_owned();

            Ok(Submission { id, timestamp, task, user, lang, score, code_length, status, time, memory })
        }).collect::<Result<Vec<Submission>>>()?;
        Ok((results, auth))
    })
}

#[cfg(test)]
mod tests {}
