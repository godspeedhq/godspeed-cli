//! `godspeed publish`: package, authenticate, reserve, upload, finalise - each step reported only once its
//! outcome is established.
//!
//! The one step that publishes is finalisation. A finalise that gets no answer may have succeeded, so it is
//! never retried blindly and never reported as a failure: the CLI reads the registry's public index and says
//! what it finds (CLAUDE.md, "A timed-out publication is not a failure"). Running `publish` again is always
//! safe: a reservation for the same manifest resumes, uploaded files are skipped, and a version already
//! published with this exact manifest is reported as published.

use super::{build, package};
use crate::diag::{Diagnostic, Kind, Result, Stage};
use crate::platform::Platform;
use crate::project::Project;
use crate::registry::{self, Answer, Client, Credentials};
use godspeed_package::{self as pkg, Sha256Digest, Version};
use std::path::Path;

/// What the public index says about this version.
enum Found {
    Ours(String),
    Different(String),
    Absent,
}

fn look_up(client: &Client, name: &str, version: Version, ours: &Sha256Digest) -> Answer<Found> {
    match client.index(name) {
        Answer::Done(None) => Answer::Done(Found::Absent),
        Answer::Done(Some(text)) => match pkg::parse_index(&text) {
            Ok(owned) => {
                let releases = owned.releases();
                let index = owned.index(&releases);
                Answer::Done(match index.release(version) {
                    Some(r) if r.manifest == *ours => Found::Ours(r.published.to_string()),
                    Some(r) => Found::Different(r.manifest.to_string()),
                    None => Found::Absent,
                })
            }
            Err(e) => Answer::Unknown(format!("the registry's index for {name} could not be read: {e}")),
        },
        Answer::Refused(r) => Answer::Unknown(format!("the index could not be read: {}", r.message)),
        Answer::Unknown(why) => Answer::Unknown(why),
    }
}

fn published(name: &str, version: Version, when: &str, already: bool) {
    if already {
        println!("{name}@{version} is already published, with this exact manifest ({when}). Nothing to do.");
    } else {
        println!("Published {name}@{version} ({when})");
    }
    println!("Install, on a Godspeed machine:");
    println!("  godspeed install {name}");
    println!("The registry verified every file and your signature. Its trust anchor (signed root and index metadata)");
    println!("is not built yet, so a client cannot yet check that your key is the one registered for your namespace.");
}

pub fn run(platform: &Platform, project: &Project, registry_flag: Option<&str>) -> Result<()> {
    // Who and where, before spending time building: an unconfigured registry or a missing token is known now.
    let creds = Credentials::load()?;
    let url = registry::resolve(registry_flag, &creds)?;
    let (token, _) = registry::token(&url, &creds)?;
    let client = Client::new(&url, Some(token));
    let key = crate::signing::load()?;

    // Validate, build every architecture, package, verify (package::run reports each step).
    let arches = build::select(platform, None, true)?;
    let dir = package::run(platform, project, &arches, None)?;
    eprintln!("Signing with {}...", key.public);
    super::sign::sign_dir(&dir, &key)?;
    let manifest_bytes =
        std::fs::read(dir.join(super::verify::MANIFEST_FILE)).map_err(|e| Diagnostic::io("publish", &dir, &e))?;
    let owned = pkg::parse(std::str::from_utf8(&manifest_bytes).unwrap_or_default())
        .expect("package::run verified this manifest");
    let manifest = owned.manifest();
    let name = manifest.name.to_string();
    let version = manifest.version;
    let ours = Sha256Digest::of(&manifest_bytes);
    let doing = format!("publish {name}@{version}");

    eprintln!("Authenticating with {url}...");
    let me = match client.whoami() {
        Answer::Done(me) => me,
        Answer::Refused(r) => return Err(registry::refused(&doing, &url, &r).into()),
        Answer::Unknown(why) => return Err(registry::unreachable(&doing, Stage::Authentication, &why).into()),
    };
    let publisher = manifest.name.publisher();
    if !me.publishers.iter().any(|p| p == publisher) {
        return Err(Diagnostic::new(
            Kind::PermissionDenied,
            Stage::Authentication,
            doing,
            format!("Account {} may not publish under {publisher}/.", me.account),
        )
        .expected(format!(
            "publisher in godspeed.toml to be one of: {}",
            if me.publishers.is_empty() {
                "(none yet)".into()
            } else {
                me.publishers.join(", ")
            }
        ))
        .next("Change the publisher, or ask the registry maintainer to add your account to that namespace.")
        .into());
    }

    eprintln!("Reserving {name}@{version}...");
    let reservation = match client.reserve(&name, &manifest_bytes) {
        Answer::Done(r) => r,
        Answer::Refused(r) if r.error == "version-exists" => {
            // A retry of a publish that already went through is a success, not a conflict.
            return match look_up(&client, &name, version, &ours) {
                Answer::Done(Found::Ours(when)) => {
                    published(&name, version, &when, true);
                    Ok(())
                }
                _ => Err(registry::refused(&doing, &url, &r).into()),
            };
        }
        Answer::Refused(r) => return Err(registry::refused(&doing, &url, &r).into()),
        Answer::Unknown(why) => return Err(registry::unreachable(&doing, Stage::Upload, &why).into()),
    };
    eprintln!("Reserved until {}.", reservation.expires);

    for file in &reservation.files {
        if reservation.staged.contains(file) {
            eprintln!("Uploading {file}... already uploaded");
            continue;
        }
        let bytes = std::fs::read(dir.join(file)).map_err(|e| Diagnostic::io("publish", &dir.join(file), &e))?;
        eprintln!("Uploading {file} ({} bytes)...", bytes.len());
        match client.upload(&name, &version.to_string(), &reservation.upload, file, &bytes) {
            Answer::Done(_) => {}
            Answer::Refused(r) => return Err(registry::refused(&doing, &url, &r).into()),
            Answer::Unknown(why) => return Err(registry::unreachable(&doing, Stage::Upload, &why).into()),
        }
    }

    eprintln!("Finalising publication...");
    match client.finalise(&name, &version.to_string(), &reservation.upload) {
        Answer::Done(p) if p.manifest == ours.to_string() => {
            published(&name, version, &p.published, false);
            Ok(())
        }
        Answer::Done(p) => Err(Diagnostic::new(
            Kind::Conflict,
            Stage::Finalisation,
            doing,
            format!(
                "The registry published a manifest with digest {}, not this one ({ours}).",
                p.manifest
            ),
        )
        .next("Report this to the registry maintainer.")
        .into()),
        Answer::Refused(r) => Err(registry::refused(&doing, &url, &r).into()),
        Answer::Unknown(why) => reconcile(&client, &doing, &name, version, &ours, &why, &dir),
    }
}

/// Finalisation got no answer. Find out from the public index; never guess.
fn reconcile(
    client: &Client,
    doing: &str,
    name: &str,
    version: Version,
    ours: &Sha256Digest,
    why: &str,
    dir: &Path,
) -> Result<()> {
    eprintln!("The registry did not answer finalisation ({why}). Reading its index to find out what happened...");
    let unknown = |found: &str| {
        Diagnostic::new(
            Kind::OutcomeUnknown,
            Stage::Finalisation,
            format!("confirm that {name}@{version} was published"),
            format!("Finalisation got no answer ({why}), and {found}. It may still complete."),
        )
        .next(format!(
            "Run `godspeed publish` again (from this project; the package is in {}). It is safe to repeat: it \
             reports the version as published if it was, and finishes the upload if it was not.",
            dir.display()
        ))
    };
    match look_up(client, name, version, ours) {
        Answer::Done(Found::Ours(when)) => {
            published(name, version, &when, false);
            Ok(())
        }
        Answer::Done(Found::Different(digest)) => Err(Diagnostic::new(
            Kind::Conflict,
            Stage::Finalisation,
            doing.to_string(),
            format!("{name}@{version} is published, but with a different manifest ({digest}), not this one ({ours})."),
        )
        .next("Increment the version and publish again. Published versions never change.")
        .into()),
        Answer::Done(Found::Absent) => Err(unknown("the index does not list it yet").into()),
        Answer::Refused(_) | Answer::Unknown(_) => Err(unknown("the index could not be read either").into()),
    }
}
