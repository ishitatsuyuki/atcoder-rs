extern crate atcoder;
extern crate tokio_core;
extern crate futures;

use std::env;
use tokio_core::reactor::Core;
use futures::Future;

#[test]
#[ignore]
fn test_login_logout() {
    let mut core = Core::new().unwrap();
    let client = atcoder::create_client(&core.handle()).unwrap();
    core.run(
        atcoder::login(
            &env::var("ATCODER_USERNAME").unwrap(),
            &env::var("ATCODER_PASSWORD").unwrap(),
            &client,
        ).and_then(|(auth, _)| atcoder::logout(auth, &client)),
    ).unwrap();
}

#[test]
#[ignore]
fn test_join() {
    let mut core = Core::new().unwrap();
    let client = atcoder::create_client(&core.handle()).unwrap();
    core.run(
        atcoder::login(
            &env::var("ATCODER_USERNAME").unwrap(),
            &env::var("ATCODER_PASSWORD").unwrap(),
            &client,
        ).and_then(|(auth, _)| {
            atcoder::join(&env::var("ATCODER_CONTEST_JOIN").unwrap(), auth, &client)
        })
            .and_then(|(_, auth)| atcoder::logout(auth, &client)),
    ).unwrap();
}

#[test]
#[ignore]
fn test_submit() {
    let mut core = Core::new().unwrap();
    let client = atcoder::create_client(&core.handle()).unwrap();
    core.run(
        atcoder::login(
            &env::var("ATCODER_USERNAME").unwrap(),
            &env::var("ATCODER_PASSWORD").unwrap(),
            &client,
        ).and_then(|(auth, _)| {
            atcoder::submit(
                "practice",
                "a",
                "rust",
                include_str!("submit_data/practice_a.rs").to_owned(),
                auth,
                &client,
            )
        })
            .and_then(|(_, auth)| atcoder::logout(auth, &client)),
    ).unwrap();
}
