#![feature(conservative_impl_trait)]

#[macro_use]
extern crate error_chain;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate futures;
extern crate tokio_core;
extern crate reqwest;
extern crate cookie;
extern crate percent_encoding;
extern crate select;

mod revel_deserialize;

use futures::{future, Future};
use tokio_core::reactor::Handle;
use reqwest::unstable::async::Client;
use reqwest::header::{Cookie, SetCookie};
use reqwest::{RedirectPolicy, StatusCode};
use cookie::Cookie as CookieParser;
use select::document::Document;
use select::predicate::{Attr, Descendant, Name};

use revel_deserialize::RevelFlash;

const API_BASE: &str = "https://beta.atcoder.jp";

error_chain! {
    foreign_links {
        ReqError(::reqwest::Error);
        CookieError(::cookie::ParseError);
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

#[derive(Debug)]
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

fn get_post<F: FnOnce(&Document) -> Result<Vec<(&'static str, String)>> + 'static>(
    get: String,
    post: Option<String>,
    form_data: F,
    auth: Option<Authentication>,
    handle: &Handle,
) -> impl Future<Item = (Option<String>, Authentication), Error = Error> + 'static {
    let post = post.unwrap_or(get.clone());
    future::lazy({
        let handle = handle.clone();
        move || -> Result<_> {
            let client = Client::builder()?
                .redirect(RedirectPolicy::none())
                .build(&handle)?;
            let mut request = client.get(&get)?;
            if let Some(auth) = auth {
                let mut cookie = Cookie::new();
                cookie.append("REVEL_SESSION", auth.session);
                request.header(cookie);
            }
            Ok(request.send().from_err().join(Ok(client)))
        }
    }).flatten()
        .and_then(|(response, client)| -> Result<_> {
            ensure!(
                response.status() == StatusCode::Ok,
                ErrorKind::BadStatus(response.status())
            );
            let cookies = response
                .headers()
                .get::<SetCookie>()
                .cloned()
                .ok_or(ErrorKind::InvalidResponse("No cookies received".to_owned()))?;
            for raw_cookie in &**cookies {
                let cookie = CookieParser::parse(&**raw_cookie)
                    .chain_err(|| {
                        ErrorKind::InvalidResponse("Failed to parse cookie".to_owned())
                    })?;
                if cookie.name() == "REVEL_SESSION" {
                    return Ok((
                        Authentication {
                            session: cookie.value().to_owned(),
                        },
                        response,
                        client,
                    ));
                }
            }
            bail!(ErrorKind::InvalidResponse(
                "No \"REVEL_SESSION\" cookie found".to_owned()
            ));
        })
        .and_then(move |(auth, mut response, client)| {
            return future::ok(auth).join3(
                response.body_resolved().from_err().and_then(move |body| {
                    let document = Document::from(::std::str::from_utf8(&body)
                        .chain_err(|| {
                            ErrorKind::InvalidResponse("Cannot decode response".to_owned())
                        })?);
                    let mut form = form_data(&document)?;
                    form.push((
                        "csrf_token",
                        csrf_token(&document)
                            .ok_or(ErrorKind::InvalidResponse(
                                "Cannot find csrf_token".to_owned(),
                            ))?,
                    ));
                    Ok(form)
                }),
                Ok(client),
            );
        })
        .and_then({
            move |(auth, form, client)| {
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
            let cookies = response
                .headers()
                .get::<SetCookie>()
                .ok_or(ErrorKind::InvalidResponse("No cookie received".to_owned()))?;
            let mut result = None;
            let mut success = None;
            for raw_cookie in &**cookies {
                let cookie = CookieParser::parse(&**raw_cookie)
                    .chain_err(|| {
                        ErrorKind::InvalidResponse("Failed to parse cookie".to_owned())
                    })?;
                if cookie.name() == "REVEL_SESSION" {
                    result = Some(Authentication {
                        session: cookie.value().to_owned(),
                    });
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
                .ok_or(
                    ErrorKind::InvalidResponse("No \"REVEL_SESSION\" cookie found".to_owned())
                        .into(),
                )
                .map(|auth| (success, auth))
        })
}

pub fn login(
    username: &str,
    password: &str,
    handle: &Handle,
) -> impl Future<Item = (Authentication, Option<String>), Error = Error> {
    let form = vec![
        ("username", username.to_owned()),
        ("password", password.to_owned()),
    ];
    get_post(
        format!("{}/login/", API_BASE),
        None,
        move |_| Ok(form),
        None,
        handle,
    ).map(|(message, auth)| (auth, message))
}

pub fn logout(
    auth: Authentication,
    handle: &Handle,
) -> impl Future<Item = Option<String>, Error = Error> {
    get_post(
        format!("{}", API_BASE),
        Some(format!("{}/logout/", API_BASE)),
        |_| Ok(vec![]),
        Some(auth),
        handle,
    ).map(|(message, _)| message)
}

pub fn join(
    contest: &str,
    auth: Authentication,
    handle: &Handle,
) -> impl Future<Item = (Option<String>, Authentication), Error = Error> {
    get_post(
        format!("{}/contests/{}/", API_BASE, contest),
        Some(format!("{}/contests/{}/register/", API_BASE, contest)),
        |_| Ok(vec![]),
        Some(auth),
        handle,
    )
}

pub fn submit(
    contest: &str,
    task: &str,
    lang: &str,
    source: String,
    auth: Authentication,
    handle: &Handle,
) -> impl Future<Item = (Option<String>, Authentication), Error = Error> {
    get_post(
        format!("{}/contests/{}/submit/", API_BASE, contest),
        None,
        {
            let task = task.to_lowercase();
            let lang = lang.to_lowercase();
            move |doc| {
                let mut tasks = doc.find(Descendant(Attr("id", "select-task"), Name("option")));
                let task_id = tasks
                    .find(|t| t.inner_html().to_lowercase().starts_with(&task))
                    .and_then(|n| n.attr("value"))
                    .ok_or(ErrorKind::NoSuchTask)?;
                let select_lang = format!("select-lang-{}", task_id);
                let mut langs = doc.find(Descendant(Attr("id", &*select_lang), Name("option")));
                let lang_id = langs
                    .find(|t| t.inner_html().to_lowercase().starts_with(&lang))
                    .and_then(|n| n.attr("value"))
                    .ok_or(ErrorKind::NoSuchLanguage)?;
                Ok(vec![
                    ("data.TaskScreenName", task_id.to_owned()),
                    ("data.LanguageId", lang_id.to_owned()),
                    ("sourceCode", source),
                ])
            }
        },
        Some(auth),
        handle,
    )
}

#[cfg(test)]
mod tests {
    use std::env;
    use tokio_core::reactor::Core;
    use futures::Future;

    #[test]
    #[ignore]
    fn test_login_logout() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(
            super::login(
                &env::var("ATCODER_USERNAME").unwrap(),
                &env::var("ATCODER_PASSWORD").unwrap(),
                &handle,
            ).and_then(|(auth, _)| super::logout(auth, &handle)),
        ).unwrap();
    }

    #[test]
    #[ignore]
    fn test_join() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(
            super::login(
                &env::var("ATCODER_USERNAME").unwrap(),
                &env::var("ATCODER_PASSWORD").unwrap(),
                &handle,
            ).and_then(|(auth, _)| {
                super::join(&env::var("ATCODER_CONTEST_JOIN").unwrap(), auth, &handle)
            })
                .and_then(|(_, auth)| super::logout(auth, &handle)),
        ).unwrap();
    }

    #[test]
    #[ignore]
    fn test_submit() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(
            super::login(
                &env::var("ATCODER_USERNAME").unwrap(),
                &env::var("ATCODER_PASSWORD").unwrap(),
                &handle,
            ).and_then(|(auth, _)| {
                super::submit(
                    "practice",
                    "a",
                    "rust",
                    "use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    let mut buf = String::new();
    input.read_line(&mut buf).unwrap();
    let a: usize = buf.trim().parse().unwrap();
    buf.clear();
    input.read_line(&mut buf).unwrap();
    let (b, c): (usize, usize) = {
        let mut split = buf.split_whitespace().map(|s| s.parse().unwrap());
        (split.next().unwrap(), split.next().unwrap())
    };
    buf.clear();
    input.read_line(&mut buf).unwrap();
    let s = buf.trim().to_owned();
    writeln!(output, \"{} {}\", a + b + c, s).unwrap();
}"
                        .to_owned(),
                    auth,
                    &handle,
                )
            })
                .and_then(|(_, auth)| super::logout(auth, &handle)),
        ).unwrap();
    }
}
