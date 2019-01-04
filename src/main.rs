#[macro_use]
extern crate clap;
#[macro_use]
extern crate error_chain;
extern crate flate2;
extern crate futures;
extern crate reqwest;
#[allow(unused_extern_crates)]
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate tar;
extern crate term;
extern crate tokio_core as tokio;

mod errors;
mod cli;

use std::path::Path;
use tar::Archive;
use flate2::read::GzDecoder;
use std::io::{self, Write};
use tokio::reactor::Core;
use reqwest::async::Client;
use reqwest::Url;
use futures::{future, Future, Stream};
use errors::*;

// TODO: Integrate https://crates.io/crates/alpm-sys
// TODO: With alpm-sys, ask if a package is installed (use a threadpool) during a search
// TODO: Grab a CPU pool already
// TODO: Show error if we don't match on info and download
// TODO: Use term-size to wrap search result descriptions
// TODO: Use a better terminal color lib (this one is so verbose)

// https://wiki.archlinux.org/index.php/AurJson#info

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct AurPackage {
    #[serde(rename = "ID")] id: u32,
    name: String,
    #[serde(rename = "PackageBaseID")] package_base_id: u32,
    package_base: String,
    version: String,
    description: String,
    #[serde(rename = "URL")] url: String,
    num_votes: u32,
    popularity: f64,
    out_of_date: Option<u32>,
    maintainer: Option<String>,
    first_submitted: u32,
    last_modified: u32,
    #[serde(rename = "URLPath")] url_path: String,
    depends: Vec<String>,
    make_depends: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    results: Vec<AurPackage>,
}

quick_main!(|| -> Result<()> {
    let matches = cli::build().get_matches();

    let mut core = Core::new()?;

    match matches.subcommand() {
        ("search", Some(matches)) => {
            // NOTE: Clap checks that term is present
            let term = matches.value_of("term").unwrap();
            let work = search(term).and_then(|response| {
                for result in response.results {
                    print_search_result(&result)?;
                }

                Ok(())
            });

            core.run(work)?;
        }

        ("info", Some(matches)) => {
            // NOTE: Clap checks that package is present
            let package = matches.values_of("package").unwrap();
            let work = info(package).and_then(|response| {
                for result in response.results {
                    print_info_result(&result)?;
                }

                Ok(())
            });

            core.run(work)?;
        }

        ("download", Some(matches)) => {
            // NOTE: Clap checks that package is present
            let package = matches.values_of("package").unwrap();
            let work = info(package).and_then(|response| {
                let mut work = vec![];

                for result in response.results {
                    work.push(download(result, "."));
                }

                future::join_all(work)
            });

            core.run(work)?;
        }

        _ => {}
    }

    Ok(())
});

// TODO: kind should be an enum of "info" or "search"
fn aur_query<'a, P>(
    kind: &'a str,
    parameters: P,
) -> impl Future<Item = AurResponse, Error = Error> + 'a
    where P: IntoIterator<Item=(&'a str, &'a str)> + 'a
{
    future::lazy(move || -> Result<_> {
        let client = Client::new();
        let mut params = vec![
            ("v", "5"),
            ("type", kind),
        ];

        params.extend(parameters);

        let url = Url::parse_with_params(
            "https://aur.archlinux.org/rpc/",
            &params,
        )?;

        Ok(client.get(url))
    })
        // Send the request ..
        .and_then(|request| request.send().from_err())
        // Parse the request as JSON ..
        // TODO: Handle errors
        .and_then(|mut response| response.json().from_err())
}

fn info<'a, I>(packages: I) -> impl Future<Item = AurResponse, Error = Error> + 'a
    where I: IntoIterator<Item = &'a str> + 'a
{
    aur_query("info", std::iter::repeat("arg[]").zip(packages))
}

// TODO: Allow selecting search fields: name-desc, name, maintainer
fn search<'a>(term: &'a str) -> impl Future<Item = AurResponse, Error = Error> + 'a {
    aur_query("search", vec![("by", "name-desc"), ("arg", term)]).map(
        |mut response: AurResponse| {
            // TODO: --sort votes,popularity,+name (votes)

            response
                .results
                .sort_by(|a, b| b.num_votes.cmp(&a.num_votes));

            response
        },
    )
}

fn download<'a, P: AsRef<Path> + 'a>(
    package: AurPackage,
    dst: P,
) -> impl Future<Item = (), Error = Error> + 'a {
    future::lazy(move || -> Result<_> {
        let client = Client::new();
        let url = format!(
            "https://aur.archlinux.org{}",
            package.url_path,
        );

        Ok(client.get(&url))
    })
        // Send the request ..
        .and_then(|request| request.send().from_err())
        // Write out the response to a file
        // TODO: Handle errors
        .and_then(move |response| {
            response.into_body().from_err().concat2().and_then(move |bytes| -> Result<()> {
                // TODO: This should all be in a threadloop

                let decoder = GzDecoder::new(bytes.as_ref())?;
                let mut archive = Archive::new(decoder);

                archive.unpack(dst)?;

                Ok(())
            })
        })
}

fn print_search_result(result: &AurPackage) -> Result<()> {
    let mut t = term::stdout().chain_err(|| "failed to acquire terminal")?;

    t.fg(term::color::BRIGHT_WHITE)?;
    t.attr(term::Attr::Bold)?;

    write!(t, "{}", result.name)?;

    t.fg(term::color::GREEN)?;

    write!(t, " {}", result.version)?;

    t.fg(term::color::BLUE)?;

    write!(t, " ({}, {:.2})", result.num_votes, result.popularity)?;

    // t.fg(term::color::CYAN)?;

    // TODO: Pre-query this information with pacman
    // write!(t, " [installed]")?;

    t.reset()?;

    writeln!(t, "")?;

    // TODO: Write out entire description line-by-line, using terminal width
    let mut desc = result.description.clone();
    desc.truncate(120);

    writeln!(t, "    {}...", desc)?;

    Ok(())
}

fn print_info_field(
    t: &mut term::Terminal<Output = io::Stdout>,
    key: &str,
    value: &str,
) -> Result<()> {
    t.fg(term::color::BRIGHT_WHITE)?;
    t.attr(term::Attr::Bold)?;

    write!(t, "{:15} :", key)?;

    t.reset()?;

    writeln!(t, " {}", value)?;

    Ok(())
}

fn print_info_result(result: &AurPackage) -> Result<()> {
    let ti = term::terminfo::TermInfo::from_env()?;
    let mut t = term::terminfo::TerminfoTerminal::new_with_terminfo(io::stdout(), ti);

    print_info_field(&mut t, "Name", &result.name)?;
    print_info_field(&mut t, "Version", &result.version)?;
    print_info_field(&mut t, "Description", &result.description)?;
    print_info_field(&mut t, "URL", &result.url)?;
    print_info_field(&mut t, "Votes", &format!("{}", result.num_votes))?;
    print_info_field(&mut t, "Depends", &format!("{}", result.depends.join(", ")))?;
    if let Some(ref mkdep) = result.make_depends {
        print_info_field(&mut t, "Make Depends", &format!("{}", mkdep.join(", ")))?;
    }
    // TODO: More fields

    writeln!(t, "")?;

    Ok(())
}
