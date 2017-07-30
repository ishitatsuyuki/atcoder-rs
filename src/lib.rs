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

use futures::{Future, future};
use tokio_core::reactor::Handle;
use reqwest::unstable::async::Client;
use reqwest::header::{Cookie, SetCookie};
use reqwest::RedirectPolicy;
use reqwest::StatusCode;
use cookie::Cookie as CookieParser;
use percent_encoding::percent_decode;
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
    let node = candidate.next();
    // TODO: handle multiple occurences
    assert_eq!(candidate.count(), 0);
    node.and_then(|node| node.attr("value")).map(str::to_owned)
}

pub fn login(
    username: &str,
    password: &str,
    handle: &Handle,
) -> impl Future<Item = (Authentication, Option<String>), Error = Error> {
    future::lazy({
        let handle = handle.clone();
        move || -> Result<_> {
            let client = Client::builder()?
                .redirect(RedirectPolicy::none())
                .build(&handle)?;
            let mut request = client.get(&format!("{}/login/", API_BASE))?;
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
            let username = username.to_owned();
            let password = password.to_owned();
            move |(auth, csrf_token, client)| {
                let mut cookie = Cookie::new();
                cookie.append("REVEL_SESSION", auth.session);
                let mut request = client.post(&format!("{}/login/", API_BASE))?;
                request.header(cookie);
                request.form(&[
                    ("username", username),
                    ("password", password),
                    ("csrf_token", csrf_token),
                ])?;
                Ok(request.send().from_err())
            }
        })
        .flatten()
        .and_then(|response| {
            let cookies = response
                .headers()
                .get::<SetCookie>()
                .ok_or(ErrorKind::InvalidResponse("No cookie received".to_owned()))?;
            ensure!(
                response.status() == StatusCode::Found,
                ErrorKind::BadStatus(response.status())
            );
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
                    let decoded: Vec<_> = percent_decode(cookie.value().as_bytes()).collect();
                    let flash: RevelFlash = revel_deserialize::from_bytes(&decoded)
                        .chain_err(|| {
                            ErrorKind::InvalidResponse(
                                "Failed to decode \"REVEL_FLASH\"".to_owned(),
                            )
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
                .map(|x| (x, success))
        })

}

#[cfg(test)]
mod tests {
    use std::env;
    use tokio_core::reactor::Core;

    #[test]
    #[ignore]
    fn test_login() {
        let mut core = Core::new().unwrap();
        let handle = core.handle();
        core.run(super::login(
            &env::var("ATCODER_USERNAME").unwrap(),
            &env::var("ATCODER_PASSWORD").unwrap(),
            &handle,
        )).unwrap();
    }
}
