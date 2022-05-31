extern crate adblock;
extern crate chardetng;
extern crate lol_html;

use adblock::{
    blocker::{Blocker, BlockerOptions},
    filters::{cosmetic::CosmeticFilterMask, network::FilterPart},
    lists::{parse_filter, FilterFormat, ParsedFilter, ParseOptions},
    optimizer::optimize,
    request::Request,
    url_parser,
};
use chardetng::EncodingDetector;
use cssparser::{Parser as CssParser, ParserInput, ToCss};
use lol_html::{element, AsciiCompatibleEncoding, HtmlRewriter, Settings};

use selectors::parser::{
    Component, NonTSPseudoClass, Parser, PseudoElement, SelectorImpl, SelectorList,
    SelectorParseErrorKind,
};

use std::cmp;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::time;

#[derive(Debug, Clone, PartialEq)]
pub struct SelectorImplDescriptor;

impl SelectorImpl for SelectorImplDescriptor {
    type AttrValue = String;
    type Identifier = String;
    type ClassName = String;
    type PartName = String;
    type LocalName = String;
    type NamespacePrefix = String;
    type NamespaceUrl = String;
    type BorrowedNamespaceUrl = String;
    type BorrowedLocalName = String;

    type NonTSPseudoClass = NonTSPseudoClassStub;
    type PseudoElement = PseudoElementStub;

    type ExtraMatchingData = ();
}

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub enum PseudoElementStub {}

impl ToCss for PseudoElementStub {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

impl PseudoElement for PseudoElementStub {
    type Impl = SelectorImplDescriptor;
}

#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub enum NonTSPseudoClassStub {}

impl NonTSPseudoClass for NonTSPseudoClassStub {
    type Impl = SelectorImplDescriptor;

    fn is_active_or_hover(&self) -> bool {
        match *self {}
    }

    fn is_user_action_state(&self) -> bool {
        match *self {}
    }

    fn has_zero_specificity(&self) -> bool {
        match *self {}
    }
}

impl ToCss for NonTSPseudoClassStub {
    fn to_css<W: fmt::Write>(&self, _dest: &mut W) -> fmt::Result {
        match *self {}
    }
}

#[derive(Default)]
struct SelectorsParser;

impl SelectorsParser {}

impl<'i> Parser<'i> for SelectorsParser {
    type Impl = SelectorImplDescriptor;
    type Error = SelectorParseErrorKind<'i>;
}

#[derive(Debug, Clone)]
struct AdMatchError;

impl fmt::Display for AdMatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ad matched")
    }
}

impl std::error::Error for AdMatchError {}

type CosmeticFilters = HashMap<String, Vec<u64>>;
type CosmeticFiltersEntry<'a> = std::collections::hash_map::Entry<'a, String, Vec<u64>>;

fn filters_entry<'a>(
    selector_str: &'a String,
    cosmetic: &'a mut CosmeticFilters,
    block_id: &'a mut CosmeticFilters,
    block_class: &'a mut CosmeticFilters,
) -> CosmeticFiltersEntry<'a> {
    let mut input = ParserInput::new(selector_str);
    let mut css_parser = CssParser::new(&mut input);
    let result = SelectorList::parse(&SelectorsParser::default(), &mut css_parser);
    let mut selector_list = result.unwrap();
    if selector_list.0.len() != 1 {
        return cosmetic.entry(selector_str.to_owned());
    }
    let selector = selector_list.0.pop().unwrap();
    if selector.len() != 1 {
        return cosmetic.entry(selector_str.to_owned());
    }
    let component = selector.iter_raw_match_order().next().unwrap();
    match component {
        Component::ID(n) => {
            return block_id.entry(n.to_owned());
        }
        Component::Class(n) => {
            return block_class.entry(n.to_owned());
        }
        _ => {
            return cosmetic.entry(selector_str.to_owned());
        }
    };
}

fn main() {
    let timer = time::Instant::now();

    let rules_file = File::open("easylist.txt").unwrap();
    let rules_reader = BufReader::new(rules_file);
    let rules_lines = rules_reader.lines();

    // matching against hosts:
    // take last part of domain and check, if does not match take one more subdomain
    // so subsub.sub.example.com: check example.com, sub.example.com and finally all of it
    let mut blocked_domains = HashSet::new();

    // map of selectors to possible exceptions
    let mut cosmetic: CosmeticFilters = HashMap::new();
    let mut block_id: CosmeticFilters = HashMap::new();
    let mut block_class: CosmeticFilters = HashMap::new();

    let mut network = Vec::new();

    for line in rules_lines {
        let linestr = line.unwrap();
        let filter = match parse_filter(
            linestr.as_str(), true, ParseOptions{ format: FilterFormat::Standard, ..ParseOptions::default()}
        ) {
            Ok(a) => a,
            Err(_) => continue,
        };
        match filter {
            ParsedFilter::Cosmetic(c) => {
                if c.not_hostnames.is_some() {
                    panic!("cosmetic filters with not_hostnames not supported");
                }
                if c.entities.is_some() || c.not_entities.is_some() {
                    panic!("cosmetic filters with entities or not_entities not supported");
                }
                match c.hostnames {
                    Some(h) => {
                        // is it a cosmetic filter exception?
                        if c.mask.contains(CosmeticFilterMask::UNHIDE) {
                            filters_entry(
                                &c.selector,
                                &mut cosmetic,
                                &mut block_id,
                                &mut block_class,
                            )
                            .or_insert_with(Vec::new)
                            .extend(h);
                        } else {
                            // we are sure there are ads under those domains so don't bother with the selector
                            blocked_domains.extend(h);
                        }
                    }
                    None => {
                        // empty Vec means no exceptions
                        filters_entry(&c.selector, &mut cosmetic, &mut block_id, &mut block_class)
                            .or_insert_with(Vec::new);
                    }
                }
            }
            ParsedFilter::Network(n) => {
                network.push(n);
            }
        }
    }

    let source_hostname = "rockpapershotgun.com";
    // TODO: it has to be a vec of hashes
    let source_hash = adblock::utils::fast_hash(&source_hostname);

    if blocked_domains.contains(&source_hash) {
        println!("blocked domain");
        return;
    }

    let mut buf = [0u8; 8192];
    let mut file = File::open("rock.html").unwrap();
    let mut size = file.read(&mut buf).unwrap();

    let mut det = EncodingDetector::new();
    det.feed(&buf[..cmp::min(1024, size)], false);
    let enc = det.guess(Some(b"com"), false);
    let ascii_comp_enc = AsciiCompatibleEncoding::new(enc).unwrap();
    // TODO: pipe encoding if not ascii compatible

    let blocker = Blocker::new(
        network,
        &BlockerOptions {
            enable_optimizations: true,
        },
    );
    let mut handlers = vec![
        element!("img[src]", |el| {
            let url = el.get_attribute("src").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "image",
                    None,
                ));
                if result.matched {
                    println!("image {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("link[href][rel='preload'][as='script']", |el| {
            let url = el.get_attribute("href").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "script",
                    None,
                ));
                if result.matched {
                    println!("preload-script {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("script[src]", |el| {
            let url = el.get_attribute("src").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "script",
                    None,
                ));
                if result.matched {
                    println!("script {}", url);
                    //return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("img[srcset]", |el| {
            let url = el.get_attribute("srcset").unwrap();
            //  parse and loop over srcset
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "imageset",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("iframe[src]", |el| {
            let url = el.get_attribute("src").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "document",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("embed[src]", |el| {
            let url = el.get_attribute("src").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "object",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("object[data]", |el| {
            let url = el.get_attribute("data").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "object",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("video[src],audio[src],source[src],track[src]", |el| {
            let url = el.get_attribute("src").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "media",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("link[href][rel='stylesheet']", |el| {
            let url = el.get_attribute("href").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "stylesheet",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("link[href][rel='pingback']", |el| {
            let url = el.get_attribute("href").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "pingback",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("a[href][ping]", |el| {
            let url = el.get_attribute("href").unwrap();
            if let Some(parsed_url) = url_parser::parse_url(&url) {
                let result = blocker.check(&Request::from_urls_with_hostname(
                    url.as_str(),
                    parsed_url.hostname(),
                    source_hostname,
                    "pingback",
                    None,
                ));
                if result.matched {
                    println!("url {}", url);
                    return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("[class]", |el| {
            let cls = el.get_attribute("class").unwrap();
            if let Some(exceptions) = block_class.get(&cls) {
                if let Some(idx) = exceptions.iter().position(|&e| e == source_hash) {
                    println!("exceptions {} idx {}", &cls, &idx);
                } else {
                    println!("class {}", &cls);
                    //return Err(AdMatchError.into());
                }
            };
            Ok(())
        }),
        element!("[id]", |el| {
            let id = el.get_attribute("id").unwrap();
            //println!("id {}", &id);
            if let Some(exceptions) = block_id.get(&id) {
                if let Some(idx) = exceptions.iter().position(|&e| e == source_hash) {
                    println!("exceptions {} idx {}", &id, &idx);
                } else {
                    println!("id {}", &id);
                    //return Err(AdMatchError.into());
                };
            };
            Ok(())
        }),
    ];
    for c in &cosmetic {
        let sel = c.0.parse::<lol_html::Selector>();
        if let Err(err) = sel {
            println!("selector: '{}' error: {}", c.0, err);
            continue;
        }
        handlers.push(element!(c.0, |_el| {
            println!("cosmetic match!");
            Ok(())
        }));
    }
    println!(
        "blocked_domains: {}, handlers: {}, cosmetic: {}, block_class: {}, block_id: {}",
        blocked_domains.len(),
        handlers.len(),
        cosmetic.len(),
        block_class.len(),
        block_id.len()
    );

    /*
    for b in &blocked_domains {
        println!("blocked_domain {}", b);
    }
    for b in &block_id {
        println!("block_id {} {}", b.0, b.1.len());
    }
    println!("hash {}", adblock::utils::fast_hash(&source_hostname));
*/
    println!("init: {:?}", timer.elapsed());

    let timer = time::Instant::now();
    let mut rewriter = HtmlRewriter::new(
        Settings {
            element_content_handlers: handlers,
            encoding: ascii_comp_enc,
            ..Settings::default()
        },
        |_: &[u8]| {},
    );
    println!("HtmlRewriter::new: {:?}", timer.elapsed());

    let timer = time::Instant::now();
    loop {
        if let Err(err) = rewriter.write(&buf[..size]) {
            println!("error: {:?}", err);
            println!("it took: {:?}", timer.elapsed());
            break;
        }
        if size < buf.len() {
            break;
        }
        size = file.read(&mut buf).unwrap();
    }

    if let Err(err) = rewriter.end() {
        println!("error: {:?}", err);
        println!("it took: {:?}", timer.elapsed());
    }
    println!("work: {:?}", timer.elapsed());
}
