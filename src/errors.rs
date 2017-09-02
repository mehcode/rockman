use std::io;
use reqwest;
use term;

error_chain!{
    foreign_links {
        Io(io::Error);
        Url(reqwest::UrlError);
        Reqwest(reqwest::Error);
        Term(term::Error);
    }
}
