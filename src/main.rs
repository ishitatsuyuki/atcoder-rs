#[macro_use]
extern crate clap;
extern crate tokio_core;
extern crate preferences;
extern crate rprompt;
extern crate rpassword;
extern crate atcoder;

use std::fs::File;
use std::io::Read;
use preferences::{AppInfo, Preferences};
use rprompt::prompt_reply_stderr;
use rpassword::prompt_password_stderr;
use tokio_core::reactor::Core;
use atcoder::{create_client, join, login, logout, submit, submissions, Authentication};

const APP_INFO: AppInfo = AppInfo {
    name: "atcoder",
    author: "Tatsuyuki Ishi",
};

fn main() {
    let matches = clap_app! (
        @app (app_from_crate!())
        (@subcommand login => )
        (@subcommand logout => )
        (@subcommand join => (@arg contest: +required))
        (@subcommand submit => (@arg contest: +required)
                               (@arg task: +required)
                               (@arg lang: +required)
                               (@arg file: +required))
        (@subcommand status => (@arg contest: +required))
    ).get_matches();

    let mut core = Core::new().unwrap();
    let client = create_client(&core.handle()).unwrap();

    if let Some(_matches) = matches.subcommand_matches("login") {
        // TODO: get credentials as parameter
        let username = prompt_reply_stderr("Username: ").unwrap();
        let password = prompt_password_stderr("Password: ").unwrap();
        let (auth, message) = core.run(login(&username, &password, &client)).unwrap();
        auth.save(&APP_INFO, "auth").unwrap();
        if let Some(message) = message {
            println!("Login successful: {}", message)
        } else {
            println!("Login successful");
        };
    } else {
        let auth = Authentication::load(&APP_INFO, "auth").unwrap();
        if let Some(_) = matches.subcommand_matches("logout") {
            let message = core.run(logout(auth, &client)).unwrap();
            if let Some(message) = message {
                println!("Logout successful: {}", message)
            } else {
                println!("Logout successful");
            };
        } else if let Some(matches) = matches.subcommand_matches("join") {
            let (message, auth) = core.run(
                join(matches.value_of("contest").unwrap(), auth, &client),
            ).unwrap();
            if let Some(message) = message {
                println!("Join successful: {}", message)
            } else {
                println!("Join successful");
            };
            auth.save(&APP_INFO, "auth").unwrap();
        } else if let Some(matches) = matches.subcommand_matches("submit") {
            let mut file = File::open(matches.value_of("file").unwrap()).unwrap();
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            let (message, auth) = core.run(submit(
                matches.value_of("contest").unwrap(),
                matches.value_of("task").unwrap(),
                matches.value_of("lang").unwrap(),
                contents,
                auth,
                &client,
            )).unwrap();
            if let Some(message) = message {
                println!("Submit successful: {}", message)
            } else {
                println!("Submit successful");
            };
            auth.save(&APP_INFO, "auth").unwrap();
        } else if let Some(matches) = matches.subcommand_matches("status") {
            let (submissions, auth) = core.run(
                submissions(matches.value_of("contest").unwrap(), Some(auth), &client),
            ).unwrap();
            for submission in submissions {
                println!("{} {} {} {}", submission.timestamp, submission.task, submission.lang, submission.status);
            }
            auth.save(&APP_INFO, "auth").unwrap();
        }
    }
}
