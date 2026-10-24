//! The extraction process (ADR-095): reads one deposited file's text, the
//! file on its standard input, its reading on its standard output. Started
//! by the API for each file (`services/documents/extractor.rs`), never run
//! by hand.

fn main() -> std::process::ExitCode {
    mimorya_extract::child::run()
}
