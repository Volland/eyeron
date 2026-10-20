use eyeron::cli::{self, CliOptions, Source};
use eyeron::error::{EyeronError, Result};
use eyeron::reasoner::PreparedReasoner;
use eyeron::{Document, is_rdf_message_log, parse_n3_with_source};
use std::env;
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use ureq::ResponseExt;

fn main() {
    if let Err(err) = run() {
        eprintln!("eyeron: {}", err);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() {
        print!("{}", cli::help());
        return Ok(());
    }
    let opt = cli::parse_args(args, &mut |warning| eprintln!("{warning}"))?;
    if opt.help {
        print!("{}", cli::help());
        return Ok(());
    }
    if opt.version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    cli::validate_options(&opt)?;
    if opt.stream { eprintln!("{}", cli::STREAM_WARNING); }
    if opt.stream_messages { return run_stream_messages(&opt); }
    let sources = read_sources(&opt.files)?.into_iter().map(|(name, text)| {
        let base_iri = if name == "<stdin>" { None }
            else if is_http_url(&name) { Some(name.clone()) }
            else { path_to_file_iri(&name).ok() };
        Source { name, text, base_iri }
    }).collect::<Vec<_>>();
    let exit_code = cli::run_sources(&opt, &sources, read_text_source, &mut emit)?;
    if exit_code != 0 { std::process::exit(exit_code); }
    Ok(())
}

fn emit(output: &str) -> Result<()> {
    io::stdout().write_all(output.as_bytes())?;
    io::stdout().flush()?;
    Ok(())
}

fn run_stream_messages(opt: &CliOptions) -> Result<()> {
    cli::validate_stream_options(opt)?;

    let mut program = Document::new();
    let mut message_sources = Vec::new();
    for source in &opt.files {
        if is_http_url(source) {
            message_sources.push(source.clone());
            continue;
        }
        let text = if source == "-" {
            let mut text = String::new();
            io::stdin().read_to_string(&mut text)?;
            text
        } else {
            fs::read_to_string(source)?
        };
        if is_rdf_message_log(&text) {
            message_sources.push(source.clone());
        } else {
            let base = opt.base_iri.as_deref().or(None);
            let parsed = parse_n3_with_source(&text, base, Some(source))
                .map_err(|err| EyeronError::new(err.with_source_location(&text, source)))?;
            program.merge(parsed);
        }
    }
    if message_sources.is_empty() {
        return Err(EyeronError::new(
            "--stream-messages did not find any RDF Message Log input",
        ));
    }

    let reasoner_options = cli::cli_reasoner_options(opt, false);
    // Built once for the whole stream: PreparedReasoner keeps the agenda index
    // it builds from `program`'s rules and reuses it for every message,
    // instead of `reason()` recomputing that index from scratch per message.
    //
    // Measured against the real alma-rdf-messages.n3 rule set padded out to
    // 1,615 static rules and run over 1,000 synthetic messages: plain
    // `reason()` (rebuilding the agenda index from `doc.rules` on every
    // message) took ~8.4s. Switching to `PreparedReasoner` alone, while it
    // still cloned the whole static rule set and agenda index into every
    // call, made this *worse* (~9.9s) -- pure per-call cloning overhead that
    // the rebuild-from-scratch path didn't pay. `PreparedReasoner::reason`
    // was then changed to hold its rule set and agenda index as `Cow`, so a
    // call only clones them if a rule reifies itself as data mid-run and has
    // to be appended (rare); the common case borrows them for the whole
    // call. That brings this same benchmark back to ~8.3-8.9s -- essentially
    // recovering the regression, not a net win over plain `reason()`, because
    // this workload's real cost is the backward search through the marc:
    // helper rules per message, not the rule-set clone. With a small rule
    // set (examples/alma-rdf-messages.n3's own ~400 rules, 100 messages)
    // neither path is measurably different (~0.5-0.7s either way).
    let prepared = PreparedReasoner::new(program);
    for source in message_sources {
        let base = opt.base_iri.clone().or_else(|| {
            if is_http_url(&source) {
                Some(source.clone())
            } else {
                path_to_file_iri(&source).ok()
            }
        });
        if is_http_url(&source) {
            let response = ureq::get(&source)
                .call()
                .map_err(|err| EyeronError::new(format!("failed to fetch {source}: {err}")))?;
            let final_url = response.get_uri().to_string();
            cli::stream_message_reader(
                BufReader::new(response.into_body().into_reader()),
                &final_url,
                base.as_deref(),
                &prepared,
                &reasoner_options,
                &mut emit,
            )?;
        } else if source == "-" {
            return Err(EyeronError::new(
                "stdin RDF Message Logs cannot follow another stdin read",
            ));
        } else {
            let file = fs::File::open(&source)?;
            cli::stream_message_reader(
                BufReader::new(file),
                &source,
                base.as_deref(),
                &prepared,
                &reasoner_options,
                &mut emit,
            )?;
        }
    }
    Ok(())
}

fn read_text_source(source: &str) -> Result<String> {
    if source == "-" {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        Ok(s)
    } else if is_http_url(source) {
        let response = ureq::get(source).call().map_err(|err| EyeronError::new(format!("failed to fetch {source}: {err}")))?;
        response.into_body().read_to_string().map_err(|err| EyeronError::new(format!("failed to read response from {source}: {err}")))
    } else {
        Ok(fs::read_to_string(source)?)
    }
}

fn read_sources(files: &[String]) -> Result<Vec<(String, String)>> {
    if files.is_empty() {
        let mut s = String::new();
        io::stdin().read_to_string(&mut s)?;
        return Ok(vec![("<stdin>".to_string(), s)]);
    }

    let mut out = Vec::new();
    for f in files {
        if f == "-" {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            out.push(("<stdin>".to_string(), s));
        } else if is_http_url(f) {
            let response = ureq::get(f)
                .call()
                .map_err(|err| EyeronError::new(format!("failed to fetch {f}: {err}")))?;
            let final_url = response.get_uri().to_string();
            let text = response.into_body().read_to_string().map_err(|err| {
                EyeronError::new(format!("failed to read response from {f}: {err}"))
            })?;
            out.push((final_url, text));
        } else {
            out.push((f.clone(), fs::read_to_string(f)?));
        }
    }
    Ok(out)
}

fn is_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

fn path_to_file_iri(path: &str) -> std::result::Result<String, ()> {
    let abs = fs::canonicalize(Path::new(path)).map_err(|_| ())?;
    let s = abs.to_string_lossy().replace('\\', "/");
    Ok(format!(
        "file://{}{}",
        if s.starts_with('/') { "" } else { "/" },
        percent_encode_path(&s)
    ))
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b))
            }
            other => out.push_str(&format!("%{:02X}", other)),
        }
    }
    out
}
