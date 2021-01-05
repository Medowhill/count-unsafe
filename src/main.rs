use std::path::PathBuf;

use rustc_ast::{Block, BlockCheckMode, Extern, Item, ItemKind, Unsafe, UnsafeSource::UserProvided, visit::{walk_block, walk_crate, walk_item, Visitor}};
use rustc_session::parse::ParseSess;
use rustc_span::{edition::Edition, Loc, Span};

use anyhow::Result;
use clap::Clap;

#[derive(Clap)]
struct Args {
    #[clap(short, long)]
    verbose: bool,
    input: Vec<PathBuf>,
}

fn main() -> Result<()> {
    let args: Args = Args::parse();
    let mut unsafes = Vec::new();

    rustc_span::with_session_globals(Edition::Edition2018, || -> Result<()> {
        let parse_sess = ParseSess::with_silent_emitter();

        for entry in args.input {
            unsafes.append(&mut collect_unsafes(&entry, &parse_sess));
        }

        println!("file,begin,end,sloc,type");

        for ublock in unsafes {
            let start = parse_sess.source_map().lookup_char_pos(ublock.span.lo());
            let end = parse_sess.source_map().lookup_char_pos(ublock.span.hi());
            let kind = ublock.kind;

            let filename = &start.file.name;

            // count sloc (excluding empty lines)
            let mut sloc = 0;
            for line_no in start.line..=end.line {
                let line = start.file.get_line(line_no - 1).unwrap();

                if args.verbose {
                    println!("{}", line);
                }

                if !line.trim().is_empty() && !line.trim().starts_with("//") {
                    // TODO: treat multi-line comments
                    sloc += 1;
                }
            }

            println!("{},{},{},{},{:?}", filename, start.line, end.line, sloc, kind);
        }

        Ok(())
    })
}

#[derive(Debug)]
struct UnsafeBlock {
    start: Loc,
    end: Loc,
    func: Option<String>,
}

#[derive(Debug)]
enum UnsafeKind {
    Trait,
    Impl,
    Fn,
    Block,
    Ffi,
}

#[derive(Debug)]
struct SpannedUnsafeBlock {
    span: Span,
    kind: UnsafeKind,
}

struct UnsafeCollector {
    blocks: Vec<SpannedUnsafeBlock>,
}

impl<'ast> Visitor<'ast> for UnsafeCollector {
    fn visit_item(&mut self, item: &'ast Item) {
        match &item.kind {
            ItemKind::Trait(_, Unsafe::Yes(..), ..) => {
                self.blocks.push(SpannedUnsafeBlock { span: item.span, kind: UnsafeKind::Trait })
            }
            ItemKind::Impl { unsafety: Unsafe::Yes(..), .. } => {
                self.blocks.push(SpannedUnsafeBlock { span: item.span, kind: UnsafeKind::Impl })
            }
            ItemKind::Fn(_, sig, ..) if matches!(sig.header.unsafety, Unsafe::Yes(..)) => {
                self.blocks.push(SpannedUnsafeBlock {
                    span: item.span,
                    kind: match sig.header.ext {
                        Extern::Explicit(..) => UnsafeKind::Ffi,
                        _ => UnsafeKind::Fn,
                    },
                })
            }
            _ => {}
        }

        walk_item(self, item);
    }

    fn visit_block(&mut self, b: &'ast Block) {
        if b.rules == BlockCheckMode::Unsafe(UserProvided) {
            self.blocks.push(SpannedUnsafeBlock { span: b.span, kind: UnsafeKind::Block })
        }

        walk_block(self, b);
    }
}

fn collect_unsafes(file: &PathBuf, sess: &ParseSess) -> Vec<SpannedUnsafeBlock> {
    let mut collector = UnsafeCollector { blocks: Vec::new() };

    let krate = rustc_parse::parse_crate_from_file(&file, &sess).unwrap();

    walk_crate(&mut collector, &krate);

    collector.blocks
}
