//! `godspeed login`, `whoami`, `logout` and `token new`: a publisher token for a registry.
//!
//! Tokens are made HERE, on the publisher's machine, the way SSH keys are: `godspeed login` makes one, keeps it
//! readable by the user alone, and prints its fingerprint (its SHA-256). The publisher sends the maintainer
//! only that fingerprint, which the maintainer approves for their account. So the token itself never travels
//! to anyone - not to the maintainer, not through a workflow log - and the registry only ever stores the
//! fingerprint.

use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::registry::{self, Answer, Client, Credentials, Entry, TOKEN_ENV};
use godspeed_package::Sha256Digest;
use std::io::{BufRead, IsTerminal};

/// A token's fingerprint: what the maintainer approves, and all the registry stores.
pub fn fingerprint(token: &str) -> String {
    Sha256Digest::of(token.as_bytes()).to_string()
}

/// A new token: `gsp_` and 64 hex digits from the operating system's random source.
fn new_token() -> std::result::Result<String, Diagnostic> {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).map_err(|e| {
        Diagnostic::new(
            Kind::Environment,
            Stage::Authentication,
            "make a publisher token",
            format!("The operating system's random number source failed: {e}"),
        )
    })?;
    Ok(format!(
        "gsp_{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

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
        .expected("a token beginning gsp_, from `godspeed login` or `godspeed token new`"));
    }
    Ok(token)
}

fn approval_instructions(fp: &str) {
    println!("Its fingerprint:");
    println!("  {fp}");
    println!("Send the registry maintainer this fingerprint - never the token itself - and your account name.");
    println!("Once they approve it, `godspeed whoami` shows your account.");
}

pub fn login(flag: Option<&str>, with_token: bool) -> Result<()> {
    let mut creds = Credentials::load()?;
    let url = registry::resolve(flag, &creds)?;

    if with_token {
        // A token made elsewhere (`godspeed token new`, say): checked with the registry before it is stored.
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
        return Ok(());
    }

    // A token already here: say where it stands, and replace it only if it can never work again.
    if let Some(entry) = creds.registries.get(&url).cloned() {
        let fp = fingerprint(&entry.token);
        match Client::new(&url, Some(entry.token)).whoami() {
            Answer::Done(me) => {
                println!("Already logged in to {url} as {}.", me.account);
                println!("  may publish under: {}", namespaces(&me.publishers));
                println!("  token expires:     {}", me.token_expires);
                return Ok(());
            }
            Answer::Refused(r) if r.error == "token-not-approved" => {
                println!("Your token for {url} is waiting for the registry maintainer's approval.");
                approval_instructions(&fp);
                return Ok(());
            }
            Answer::Refused(r) if r.error == "unauthenticated" || r.error == "token-expired" => {
                eprintln!("The stored token no longer works ({}); making a new one.", r.message);
            }
            Answer::Refused(r) => return Err(registry::refused("log in", &url, &r).into()),
            Answer::Unknown(why) => {
                println!("You have a token for {url}; whether it is approved could not be checked ({why}).");
                approval_instructions(&fp);
                return Ok(());
            }
        }
    }

    let token = new_token()?;
    let fp = fingerprint(&token);
    creds.registries.insert(url.clone(), Entry { token });
    creds.default = Some(url.clone());
    let path = creds.save()?;
    println!(
        "Made a publisher token for {url}, stored in {} (readable by you alone).",
        path.display()
    );
    approval_instructions(&fp);
    Ok(())
}

/// A token for CI: printed once, stored nowhere. Its fingerprint goes to the maintainer, the token to a CI
/// secret (`GODSPEED_TOKEN`).
pub fn token_new() -> Result<()> {
    let token = new_token()?;
    println!("{token}");
    eprintln!("Fingerprint: {}", fingerprint(&token));
    eprintln!("The token above is shown once and saved nowhere. Put it in your CI's secrets as {TOKEN_ENV}, and");
    eprintln!("send the registry maintainer the fingerprint - never the token - and your account name.");
    Ok(())
}

pub fn whoami(flag: Option<&str>) -> Result<()> {
    let creds = Credentials::load()?;
    let url = registry::resolve(flag, &creds)?;
    let (token, from) = registry::token(&url, &creds)?;
    let fp = fingerprint(&token);
    match Client::new(&url, Some(token)).whoami() {
        Answer::Done(me) => {
            println!("{} on {url}", me.account);
            println!("  may publish under: {}", namespaces(&me.publishers));
            println!("  token expires:     {}", me.token_expires);
            println!("  token from:        {from}");
            println!("  fingerprint:       {fp}");
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
        // Never approved, or already unusable: there is nothing left to revoke remotely.
        Answer::Refused(r)
            if r.error == "token-not-approved" || r.error == "unauthenticated" || r.error == "token-expired" =>
        {
            println!(
                "Logged out of {url}. The registry did not accept the token anyway: {}",
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
