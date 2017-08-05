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
use reqwest::unstable::async::{Client, Response};
use reqwest::header::{Cookie, SetCookie};
use reqwest::{RedirectPolicy, StatusCode};
use cookie::Cookie as CookieParser;
use select::document::Document;
use select::predicate::Attr;

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
    }
}

#[derive(Debug)]
pub struct Authentication {
    session: String,
}

fn csrf_token(body: &str) -> Option<String> {
    let document = Document::from(body);
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

fn get_post(
    get: String,
    post: Option<String>,
    mut form_data: Vec<(&'static str, String)>,
    auth: Option<Authentication>,
    handle: &Handle,
) -> impl Future<Item = Response, Error = Error> {
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
        .and_then(|(auth, mut response, client)| {
            return future::ok(auth).join3(
                response.body_resolved().from_err().and_then(|body| {
                    csrf_token(::std::str::from_utf8(&body)
                        .chain_err(|| {
                            ErrorKind::InvalidResponse("Cannot decode response".to_owned())
                        })?).ok_or(
                        ErrorKind::InvalidResponse("Cannot find csrf_token".to_owned()).into(),
                    )
                }),
                Ok(client),
            );
        })
        .and_then({
            move |(auth, csrf_token, client)| {
                let mut cookie = Cookie::new();
                cookie.append("REVEL_SESSION", auth.session);
                let mut request = client.post(&post)?;
                request.header(cookie);
                form_data.push(("csrf_token", csrf_token));
                request.form(&form_data)?;
                Ok(request.send().from_err())
            }
        })
        .flatten()
}

pub fn login(
    username: &str,
    password: &str,
    handle: &Handle,
) -> impl Future<Item = (Authentication, Option<String>), Error = Error> {
    get_post(
        format!("{}/login/", API_BASE),
        None,
        vec![
            ("username", username.to_owned()),
            ("password", password.to_owned()),
        ],
        None,
        handle,
    ).and_then(|response| {
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
                let flash: RevelFlash = revel_deserialize::from_bytes(cookie.value().as_bytes())
                    .chain_err(|| {
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
                ErrorKind::InvalidResponse("No \"REVEL_SESSION\" cookie found".to_owned()).into(),
            )
            .map(|x| (x, success))
    })
}

pub fn logout(
    auth: Authentication,
    handle: &Handle,
) -> impl Future<Item = Option<String>, Error = Error> {
    get_post(
        format!("{}", API_BASE),
        Some(format!("{}/logout/", API_BASE)),
        vec![],
        Some(auth),
        handle,
    ).and_then(|response| {
        ensure!(
            response.status() == StatusCode::Found,
            ErrorKind::BadStatus(response.status())
        );
        if let Some(cookie) = response.headers().get::<SetCookie>().and_then(|cookies| {
            cookies
                .iter()
                .map(|raw| CookieParser::parse(&**raw))
                .find(|cookie| {
                    // FIXME: validate
                    if let Ok(cookie) = cookie.as_ref() {
                        cookie.name() == "REVEL_FLASH"
                    } else {
                        false
                    }
                })
                .map(::std::result::Result::unwrap)
        }) {
            let flash: RevelFlash = revel_deserialize::from_bytes(cookie.value().as_bytes())
                .chain_err(|| {
                    ErrorKind::InvalidResponse("Failed to decode \"REVEL_FLASH\"".to_owned())
                })?;
            if let Some(err) = flash.error {
                bail!(ErrorKind::InvalidResponse(err))
            } else {
                return Ok(flash.success);
            }
        } else {
            Ok(None)
        }
    })
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
        ).unwrap()
    }
}
