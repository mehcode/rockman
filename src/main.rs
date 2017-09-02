#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate reqwest;
#[allow(unused_extern_crates)]
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate term;
extern crate tokio_core as tokio;

mod errors;

use std::fs::File;
use std::mem;
use std::env;
use std::io::{self, Write};
use tokio::reactor::{Core, Handle};
use reqwest::unstable::async::{Client, Decoder};
use reqwest::Url;
use futures::{future, Future, Stream};
use errors::*;

// TODO: Integrate https://crates.io/crates/alpm-sys
// TODO: With alpm-sys, ask if a package is installed (use a threadpool) during a search
// TODO: Grab a CPU pool already
// TODO: Add clap:
/*

 Commands:
    --download, -d      Download the PKGBUILD tarball and extract it in
                            the current directory (requires exact name)
    --search, -s        Search (what this does now without arguments)
    --info, -i          Show verbose information for a given package (must be exact name)

 */

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
}

#[derive(Debug, Deserialize)]
struct AurResponse {
    results: Vec<AurPackage>,
}

quick_main!(|| -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let term = &argv[1];

    let mut core = Core::new()?;
    let handle = core.handle();

    // let work = search(&handle, term).and_then(|response| {
    //     for result in response.results {
    //         print_search_result(&result)?;
    //     }

    //     Ok(())
    // });

    let work = info(&handle, "atom-editor-beta-bin").and_then(|response| {
        let mut work = vec![];

        for result in response.results {
            // print_info_result(&result)?;
            work.push(download(&handle, result));
        }

        future::join_all(work)
    });

    core.run(work)?;

    Ok(())
});

// TODO(@rust): impl Future
// TODO(@rust): Borrowing is hard across a future, figure out how to not pass in a Vec here
// TODO: kind should be an enum of "info" or "search"
fn aur_query<'a>(
    handle: &'a Handle,
    kind: &'a str,
    parameters: Vec<(&'a str, &'a str)>,
) -> Box<Future<Item = AurResponse, Error = Error> + 'a> {
    Box::new(
        future::lazy(move || -> Result<_> {
            let client = Client::new(handle)?;
            let mut params = vec![
                ("v", "5"),
                ("type", kind),
            ];

            params.extend(&parameters);

            let url = Url::parse_with_params(
                "https://aur.archlinux.org/rpc/",
                &params,
            )?;

            Ok(client.get(url)?)
        })
            // Send the request ..
            .and_then(|mut request| request.send().from_err())
            // Parse the request as JSON ..
            // TODO: Handle errors
            .and_then(|mut response| response.json().from_err()),
    )
}

// TODO(@rust): impl Future
// TODO: Allow N packages
fn info<'a>(
    handle: &'a Handle,
    package: &'a str,
) -> Box<Future<Item = AurResponse, Error = Error> + 'a> {
    aur_query(handle, "info", vec![("arg[]", package)])
}

// TODO(@rust): impl Future
// TODO: Allow selecting search fields: name-desc, name, maintainer
fn search<'a>(
    handle: &'a Handle,
    term: &'a str,
) -> Box<Future<Item = AurResponse, Error = Error> + 'a> {
    Box::new(
        aur_query(handle, "search", vec![("by", "name-desc"), ("arg", term)]).map(
            |mut response: AurResponse| {
                // TODO: --sort votes,popularity,+name (votes)

                response
                    .results
                    .sort_by(|a, b| a.num_votes.cmp(&b.num_votes));

                response.results.reverse();

                response
            },
        ),
    )
}

// TODO(@rust): impl Future
// TODO: Allow N packages (should run concurrently with async)
fn download<'a>(
    handle: &'a Handle,
    package: AurPackage,
) -> Box<Future<Item = (), Error = Error> + 'a> {
    let url_path = package.url_path.clone();

    Box::new(
        future::lazy(move || -> Result<_> {
            let client = Client::new(handle)?;
            let url = format!(
                "https://aur.archlinux.org{}",
                url_path,
            );

            Ok(client.get(&url)?)
        })
            // Send the request ..
            .and_then(|mut request| request.send().from_err())
            // Write out the response to a file
            // TODO: Handle errors
            .and_then(move |mut response| {
                // TODO(@reqwest): I own the response, I should be able to _take_ the body?
                let body = mem::replace(response.body_mut(), Decoder::empty());
                // TODO: Infer the file format and name from the url_path
                // TODO: Use a CPU pool for all this fs stuff
                Ok((body, File::create(&format!("{}.tar.gz", package.name))?))
            }).and_then(|(body, mut f)| {
                body.from_err().for_each(move |chunk| -> Result<()> {
                    // TODO: Use a CPU pool for all this fs stuff
                    f.write_all(&chunk)?;

                    Ok(())
                })
            }),
    )
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
    t: &mut Box<term::Terminal<Output = io::Stdout> + Send>,
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
    let mut t = term::stdout().chain_err(|| "failed to acquire terminal")?;

    print_info_field(&mut t, "Name", &result.name)?;
    print_info_field(&mut t, "Version", &result.version)?;
    print_info_field(&mut t, "Description", &result.description)?;
    print_info_field(&mut t, "URL", &result.url)?;
    print_info_field(&mut t, "Votes", &format!("{}", result.num_votes))?;

    // TODO: More fields

    writeln!(t, "")?;

    Ok(())
}
