# Abracabra

A search engine project, that will index only sites without ads and trackers.
Saying it otherwise - search results should be all uBlock Origin clean.

At this stage it's becoming a pipeline to filter through Common Crawl archives
and check a percentage of sites, that do not match criteria taken from AdBlock
filter lists like EasyList.

It's written in Rust and uses the lol_html crate for HTML parsing and partially
for selector matching. It uses the adblock crate for rule parsing. The current
version tries to use it for matching, but it has been proven much too slow.
A 200kB file takes 3 seconds on a modern amd64 CPU with heavy optimisations.
