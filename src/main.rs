use std::{collections::HashSet, fmt::Display, fs, ops::Add, path::PathBuf};

use rustc_ast::{
    visit::{walk_block, walk_crate, walk_fn, walk_item, FnKind, Visitor},
    Attribute, Block, BlockCheckMode, Item, ItemKind, ModKind, NodeId, Unsafe,
    UnsafeSource::UserProvided,
};
use rustc_session::parse::ParseSess;
use rustc_span::{edition::Edition, Span};

use clap::Clap;

#[derive(Clap)]
struct Args {
    input: PathBuf,
}

fn main() {
    let args: Args = Args::parse();

    rustc_span::with_session_globals(Edition::Edition2018, || {
        let sess = ParseSess::with_silent_emitter();

        let span_lines = |span: &Span| {
            let start = sess.source_map().lookup_char_pos(span.lo()).line - 1;
            let end = sess.source_map().lookup_char_pos(span.hi()).line - 1;
            (start..=end).collect::<HashSet<_>>()
        };

        let st = files(args.input, "rs").iter().fold(Stat::default(), |st, f| {
            let collector = collect_unsafes(&f, &sess);
            let test_lines = collector.tests.iter().flat_map(span_lines).collect::<HashSet<_>>();

            let source = sess.source_map().load_file(f.as_path()).unwrap();
            let is_code = |line: &usize| {
                if test_lines.contains(line) {
                    return false;
                }
                let line = source.get_line(*line).unwrap();
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with("//")
            };
            let lines = (0..source.count_lines()).filter(is_code).collect::<HashSet<_>>();
            let total_lines = |v: &Vec<Span>| {
                v.iter().flat_map(span_lines).filter(|l| lines.contains(l)).collect::<HashSet<_>>()
            };

            let mut mods = vec![];
            let mut mod_lines = HashSet::new();
            for m in collector.mods.iter().rev() {
                let mut ls = span_lines(m);
                ls.retain(|l| lines.contains(l) && !mod_lines.contains(l));
                for l in &ls {
                    mod_lines.insert(*l);
                }
                mods.push(ls);
            }
            mods.push(lines.iter().filter(|l| !mod_lines.contains(l)).map(|l| *l).collect());

            let u_bl_lines = total_lines(&collector.unsafe_blocks);
            let u_im_lines = total_lines(&collector.unsafe_impls);
            let u_lines = u_bl_lines.union(&u_im_lines).map(|l| *l).collect::<HashSet<_>>();

            let partition = |v: &Vec<Span>| {
                v.iter().partition::<Vec<Span>, _>(|s| span_lines(s).is_disjoint(&u_bl_lines))
            };

            let (ss_fns, su_fns) = partition(&collector.safe_fns);
            let (us_fns, uu_fns) = partition(&collector.unsafe_fns);

            let (ss_trs, su_trs) = partition(&collector.safe_traits);
            let (us_trs, uu_trs) = partition(&collector.unsafe_traits);

            let (s_mds, u_mds) = mods.iter().partition::<Vec<_>, _>(|s| s.is_disjoint(&u_lines));

            st + Stat {
                lines: lines.len(),

                u_bl_len: u_bl_lines.len(),
                u_im_len: u_im_lines.len(),
                u_len: u_lines.len(),

                fn_num: SU {
                    ss: ss_fns.len(),
                    su: su_fns.len(),
                    us: us_fns.len(),
                    uu: uu_fns.len(),
                },
                fn_len: SU {
                    ss: total_lines(&ss_fns).len(),
                    su: total_lines(&su_fns).len(),
                    us: total_lines(&us_fns).len(),
                    uu: total_lines(&uu_fns).len(),
                },

                tr_num: SU {
                    ss: ss_trs.len(),
                    su: su_trs.len(),
                    us: us_trs.len(),
                    uu: uu_trs.len(),
                },
                tr_len: SU {
                    ss: total_lines(&ss_trs).len(),
                    su: total_lines(&su_trs).len(),
                    us: total_lines(&us_trs).len(),
                    uu: total_lines(&uu_trs).len(),
                },

                md_num: SU { ss: s_mds.len(), su: u_mds.len(), us: 0, uu: 0 },
                md_len: SU {
                    ss: s_mds.iter().flat_map(|s| s.iter()).collect::<HashSet<_>>().len(),
                    su: u_mds.iter().flat_map(|s| s.iter()).collect::<HashSet<_>>().len(),
                    us: 0,
                    uu: 0,
                },
            }
        });

        println!("{}", st);
    })
}

fn files(path: PathBuf, ext: &str) -> Vec<PathBuf> {
    if path.is_dir() {
        fs::read_dir(path)
            .unwrap()
            .into_iter()
            .flat_map(|entry| files(entry.unwrap().path(), ext))
            .collect()
    } else if path.extension().and_then(|x| x.to_str()) == Some(ext) {
        vec![path]
    } else {
        vec![]
    }
}

#[derive(Default)]
struct SU {
    ss: usize,
    su: usize,
    us: usize,
    uu: usize,
}

impl Add for SU {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            ss: self.ss + rhs.ss,
            su: self.su + rhs.su,
            us: self.us + rhs.us,
            uu: self.uu + rhs.uu,
        }
    }
}

impl Display for SU {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{} {} {} {}", self.ss, self.su, self.us, self.uu))
    }
}

#[derive(Default)]
struct Stat {
    lines: usize,

    u_bl_len: usize,
    u_im_len: usize,
    u_len: usize,

    fn_num: SU,
    fn_len: SU,

    tr_num: SU,
    tr_len: SU,

    md_num: SU,
    md_len: SU,
}

impl Add for Stat {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            lines: self.lines + rhs.lines,

            u_bl_len: self.u_bl_len + rhs.u_bl_len,
            u_im_len: self.u_im_len + rhs.u_im_len,
            u_len: self.u_len + rhs.u_len,

            fn_num: self.fn_num + rhs.fn_num,
            fn_len: self.fn_len + rhs.fn_len,

            tr_num: self.tr_num + rhs.tr_num,
            tr_len: self.tr_len + rhs.tr_len,

            md_num: self.md_num + rhs.md_num,
            md_len: self.md_len + rhs.md_len,
        }
    }
}

impl Display for Stat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{} {} {} {} {} {} {} {} {} {}",
            self.lines,
            self.u_bl_len,
            self.u_im_len,
            self.u_len,
            self.fn_num,
            self.fn_len,
            self.tr_num,
            self.tr_len,
            self.md_num,
            self.md_len
        ))
    }
}

#[derive(Default)]
struct Collector {
    tests: Vec<Span>,
    mods: Vec<Span>,

    safe_traits: Vec<Span>,
    unsafe_traits: Vec<Span>,

    safe_fns: Vec<Span>,
    unsafe_fns: Vec<Span>,

    unsafe_impls: Vec<Span>,
    unsafe_blocks: Vec<Span>,
}

fn is_test_module(attr: &Attribute) -> bool {
    if let Some(id) = attr.ident() {
        if id.as_str() == "cfg" {
            if let Some(lst) = attr.meta_item_list() {
                return lst
                    .iter()
                    .any(|i| i.ident().map(|i| i.as_str() == "test").unwrap_or(false));
            }
        }
    }
    false
}

fn is_test_func(attr: &Attribute) -> bool {
    attr.ident().map(|i| i.as_str() == "test").unwrap_or(false)
}

impl<'ast> Visitor<'ast> for Collector {
    fn visit_item(&mut self, item: &'ast Item) {
        if item.attrs.iter().any(|a| is_test_module(a) || is_test_func(a)) {
            for a in &item.attrs {
                self.tests.push(a.span);
            }
            self.tests.push(item.span);
            return;
        }

        match &item.kind {
            ItemKind::Mod(_, ModKind::Loaded(_, _, _)) => self.mods.push(item.span),
            ItemKind::Trait(b) => {
                let vec =
                    if b.1 == Unsafe::No { &mut self.safe_traits } else { &mut self.unsafe_traits };
                vec.push(item.span);
            }
            ItemKind::Impl(b) => {
                if b.unsafety != Unsafe::No {
                    self.unsafe_impls.push(item.span);
                }
            }
            _ => {}
        }

        walk_item(self, item);
    }

    fn visit_fn(&mut self, fk: FnKind<'ast>, s: Span, _: NodeId) {
        match fk {
            FnKind::Fn(_, _, sig, _, Some(_)) => {
                let vec = if sig.header.unsafety == Unsafe::No {
                    &mut self.safe_fns
                } else {
                    &mut self.unsafe_fns
                };
                vec.push(s);
            }
            _ => {}
        }

        walk_fn(self, fk, s)
    }

    fn visit_block(&mut self, b: &'ast Block) {
        if b.rules == BlockCheckMode::Unsafe(UserProvided) {
            for s in &b.stmts {
                self.unsafe_blocks.push(s.span);
            }
        }

        walk_block(self, b);
    }
}

fn collect_unsafes(file: &PathBuf, sess: &ParseSess) -> Collector {
    let mut collector = Collector::default();
    if let Ok(krate) = rustc_parse::parse_crate_from_file(&file, &sess) {
        walk_crate(&mut collector, &krate);
    }
    collector
}
