//! `godspeed login`, `whoami` and `logout`: a publisher token for a registry.
//!
//! Phase 1 tokens are issued by the registry maintainer by hand. `login` checks a token with the registry
//! before storing it, so a stored token is one the registry accepted at that moment.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::registry::{self, Answer, Client, Credentials, Entry, TOKEN_ENV};
use std::io::{BufRead, IsTerminal};

fn read_token() -> std::result::Result<String, Diagnostic> {
    let stdin = std::io::stdin();
    let token = if stdin.is_terminal() {
        rpassword::prompt_password("Publisher token (input hidden): ")
    } else {
        // Piped, for CI or scripts: the first line of standard input.
        let mut line = String::new();
        stdin.lock().read_line(&mut line).map(|_| line)
    }
    .map_err(|e| {
        Diagnostic::new(
            Kind::Io,
            Stage::Authentication,
            "read the token",
            format!("Standard input: {e}"),
        )
    })?;
    let token = token.trim().to_string();
    if !token.starts_with("gsp_") || token.len() < 8 {
        return Err(Diagnostic::new(
            Kind::AuthenticationFailure,
            Stage::Authentication,
            "log in",
            "That is not a Godspeed publisher token.",
        )
        .expected("a token beginning gsp_, as issued by the registry maintainer"));
    }
    Ok(token)
}

pub fn login(flag: Option<&str>) -> Result<()> {
    let mut creds = Credentials::load()?;
    let url = registry::resolve(flag, &creds)?;
    let token = read_token()?;
    eprintln!("Checking the token with {url}...");
    let me = match Client::new(&url, Some(token.clone())).whoami() {
        Answer::Done(me) => me,
        Answer::Refused(r) => return Err(registry::refused("log in", &url, &r).into()),
        Answer::Unknown(why) => return Err(registry::unreachable("log in", Stage::Authentication, &why).into()),
    };
    creds.registries.insert(url.clone(), Entry { token });
    creds.default = Some(url.clone());
    let path = creds.save()?;
    println!("Logged in to {url} as {}.", me.account);
    println!("  may publish under: {}", namespaces(&me.publishers));
    println!("  token expires:     {}", me.token_expires);
    println!("  stored in:         {} (readable by you alone)", path.display());
    Ok(())
}

pub fn whoami(flag: Option<&str>) -> Result<()> {
    let creds = Credentials::load()?;
    let url = registry::resolve(flag, &creds)?;
    let (token, from) = registry::token(&url, &creds)?;
    match Client::new(&url, Some(token)).whoami() {
        Answer::Done(me) => {
            println!("{} on {url}", me.account);
            println!("  may publish under: {}", namespaces(&me.publishers));
            println!("  token expires:     {}", me.token_expires);
            println!("  token from:        {from}");
            Ok(())
        }
        Answer::Refused(r) => Err(registry::refused("show who you are", &url, &r).into()),
        Answer::Unknown(why) => Err(registry::unreachable("show who you are", Stage::Authentication, &why).into()),
    }
}

pub fn logout(flag: Option<&str>) -> Result<()> {
    let mut creds = Credentials::load()?;
    let url = registry::resolve(flag, &creds)?;
    let (token, from) = registry::token(&url, &creds)?;
    let revoked = Client::new(&url, Some(token)).logout();

    // The local copy goes whatever the registry said: the user asked for it gone.
    let had = creds.registries.remove(&url).is_some();
    if creds.default.as_deref() == Some(url.as_str()) {
        creds.default = None;
    }
    if had {
        creds.save()?;
    }
    let env_note = (from == TOKEN_ENV).then(|| format!("{TOKEN_ENV} is still set in this environment; unset it."));

    match revoked {
        Answer::Done(_) => {
            println!(
                "Logged out of {url}: the token is revoked{}.",
                if had { " and removed from this machine" } else { "" }
            );
            if let Some(n) = env_note {
                println!("{n}");
            }
            Ok(())
        }
        // Already unusable (revoked or expired): logging out has nothing left to do remotely.
        Answer::Refused(r) if r.error == "unauthenticated" || r.error == "token-expired" => {
            println!(
                "Logged out of {url}. The registry no longer accepted the token: {}",
                r.message
            );
            Ok(())
        }
        Answer::Refused(r) => Err(registry::refused("revoke the token", &url, &r).into()),
        Answer::Unknown(why) => Err(Diagnostic::new(
            Kind::RegistryUnavailable,
            Stage::Authentication,
            "revoke the token",
            format!(
                "{why}. The token was removed from this machine but is NOT known to be revoked: \
                 anyone holding a copy may still be able to use it."
            ),
        )
        .next(
            "Run `godspeed logout` again when the registry is reachable, or ask the registry maintainer to revoke it.",
        )
        .into()),
    }
}

fn namespaces(publishers: &[String]) -> String {
    if publishers.is_empty() {
        "nothing yet (ask the registry maintainer)".into()
    } else {
        publishers
            .iter()
            .map(|p| format!("{p}/"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}
