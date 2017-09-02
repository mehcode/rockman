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

use std::env;
use tokio::reactor::{Core, Handle};
use reqwest::unstable::async::Client;
use reqwest::Url;
use futures::{future, Future};
use errors::*;

// TODO: Integrate https://crates.io/crates/alpm-sys
// TODO: With alpm-sys, ask if a package is installed (use a threadpool) during a search
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
struct SearchResult {
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
struct SearchResponse {
    results: Vec<SearchResult>,
}

quick_main!(|| -> Result<()> {
    let argv = env::args().collect::<Vec<_>>();
    let term = &argv[1];

    let mut core = Core::new()?;

    let work = search(core.handle(), term.clone()).and_then(|response| {
        for result in response.results {
            print_search_result(&result)?;
        }

        Ok(())
    });

    core.run(work)?;

    Ok(())
});

// TODO(@rust): impl Future
fn search(handle: Handle, term: String) -> Box<Future<Item = SearchResponse, Error = Error>> {
    Box::new(
        future::lazy(move || -> Result<_> {
            let client = Client::new(&handle)?;
            let url = Url::parse_with_params(
                "https://aur.archlinux.org/rpc/",
                &[
                    ("v", "5"),
                    ("type", "search"),
                    ("by", "name-desc"),
                    ("arg", &term),
                ],
            )?;

            Ok(client.get(url)?)
        })
            // Send the request ..
            .and_then(|mut request| request.send().from_err())
            // Parse the request as JSON ..
            // TODO: Handle errors
            .and_then(|mut response| response.json().from_err())
            .and_then(|mut response: SearchResponse| {
                // TODO: --sort votes,popularity,+name (votes)

                response
                    .results
                    .sort_by(|a, b| a.num_votes.cmp(&b.num_votes));

                response.results.reverse();

                Ok(response)
            }),
    )
}

fn print_search_result(result: &SearchResult) -> Result<()> {
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
