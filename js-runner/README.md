JavaScript runner for Abracabra - the search engine project

Run JavaScript with QuickJS with minimal fake DOM to detect elements matching AdBlock compatible filter lists.

It uses a simple bump allocator to make it faster and limit memory usage.

It will be wrapped in seccomp and run as a separate process to the Abracabra indexer.
