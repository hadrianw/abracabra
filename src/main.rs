extern crate adblock;
extern crate chardetng;
extern crate lol_html;

use std::fs::File;
use std::io::Read;
use std::io::BufReader;
use std::io::BufRead;
use std::cmp;
use chardetng::EncodingDetector;
use adblock::blocker::{Blocker, BlockerOptions};
use adblock::lists::{parse_filter, ParsedFilter, FilterFormat};
use adblock::optimizer::optimize;
use adblock::request::Request;
use adblock::url_parser;
use lol_html::{element, HtmlRewriter, Settings};

fn main() {
    let rules_file = File::open("easylist.txt").unwrap();
    let rules_reader = BufReader::new(rules_file);
    let rules_lines = rules_reader.lines();
    let mut cosmetic = Vec::new();
    let mut network = Vec::new();

    for line in rules_lines {
        let linestr = line.unwrap();
        let filter = match parse_filter(linestr.as_str(), true, FilterFormat::Standard) {
            Ok(a) => a,
            Err(x) => {println!("{:?}: {}", x, linestr); continue},
        };
        match filter {
            ParsedFilter::Cosmetic(c) => cosmetic.push(c),
            ParsedFilter::Network(n) => network.push(n),
        };
    };
    println!("c {} n {}", cosmetic.len(), network.len());
    let network = optimize(network);
    println!("c {} n {}", cosmetic.len(), network.len());
    let h = cosmetic.iter().filter(|&c| c.hostnames.is_some()).count();
    let nh = cosmetic.iter().filter(|&c| c.not_hostnames.is_some()).count();
    let some = cosmetic.iter().filter(|&c| c.hostnames.is_some() && c.not_hostnames.is_some()).count();
    let no_h = cosmetic.iter().filter(|&c| c.hostnames.is_none()).count();
    let no_nh = cosmetic.iter().filter(|&c| c.not_hostnames.is_none()).count();
    let none = cosmetic.iter().filter(|&c| c.hostnames.is_none() && c.not_hostnames.is_none()).count();
    println!("h {} nh {} all {}", h, nh, some);
    println!("no h {} nh {} all {}", no_h, no_nh, none);

    let blocker = Blocker::new(network, &BlockerOptions{enable_optimizations: true});

    let mut buf = [0u8; 8192];
    let mut file = File::open("example.html").unwrap();
    let mut size = file.read(&mut buf).unwrap();

    let mut det = EncodingDetector::new();
    det.feed(&buf[..cmp::min(1024, size)], false);
    let enc = det.guess(Some(b"com"), false);

    let source_hostname = "example.com";
    let mut rewriter = HtmlRewriter::try_new(
        Settings {
            element_content_handlers: vec![element!("img[src]", |el|{
                let url = el.get_attribute("src").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "image", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("img[srcset]", |el|{
                let url = el.get_attribute("srcset").unwrap();
                //  parse and loop over srcset
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "imageset", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("script[src]", |el|{
                let url = el.get_attribute("src").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "script", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("iframe[src]", |el|{
                let url = el.get_attribute("src").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "document", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("embed[src]", |el|{
                let url = el.get_attribute("src").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "object", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("object[data]", |el|{
                let url = el.get_attribute("data").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "object", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("video[src],audio[src],source[src],track[src]", |el|{
                let url = el.get_attribute("src").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "media", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("link[href][rel='stylesheet']", |el|{
                let url = el.get_attribute("href").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "stylesheet", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("link[href][rel='pingback']", |el|{
                let url = el.get_attribute("href").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "pingback", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            }), element!("a[href][ping]", |el|{
                let url = el.get_attribute("href").unwrap();
                if let Some(parsed_url) = url_parser::parse_url(&url) {
                    let result = blocker.check(&Request::from_urls_with_hostname(url.as_str(), parsed_url.hostname(), source_hostname, "pingback", None));
                    if result.matched {
                        println!("url {}", url);
                    }
                };
                Ok(())
            })],
            encoding: enc.name(),
            ..Settings::default()
        },
        |_: &[u8]| {},
    ).unwrap();

    loop {
        let x = rewriter.write(&buf);
        println!("error: {:?}", x.unwrap());
        if size < buf.len() {
            break;
        }
        size = file.read(&mut buf).unwrap();
    }

    let x = rewriter.end();
    println!("error: {:?}", x);
}
