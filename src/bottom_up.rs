use anyhow::{anyhow, Result};
use ide::{AnalysisHost, FileId, LineCol, LineIndex};
use ide_db::defs::Definition;
use rustyline::Editor;
use std::collections::HashMap;
use std::{env, path::PathBuf};
use syntax::{AstNode, TokenAtOffset};

struct Analysis {
    prj_dir: PathBuf,
    host: AnalysisHost,
    fileids: HashMap<String, FileId>,
}
// src/main.rs:16:6
impl Analysis {
    pub fn new(prj_dir: impl Into<PathBuf>) -> Result<Self> {
        let prj_dir = prj_dir.into();
        let (host, vfs) = rust_analyzer::cli::load_cargo(prj_dir.as_ref(), true, false)?;
        let fileids = vfs
            .iter()
            .map(|(a, b)| (b.to_string(), a))
            .collect::<HashMap<_, _>>();
        let _ = host.analysis().prime_caches(|_| {});
        Ok(Self {
            prj_dir,
            host,
            fileids,
        })
    }
    pub fn resolve(&self, file: &str, line: u32, col: u32) -> Result<Definition> {
        let fid = if let Some(fid) = self
            .fileids
            .get(self.prj_dir.join(file).to_string_lossy().as_ref())
        {
            fid
        } else {
            return Err(anyhow!("file not found"));
        };
        let db = self.host.raw_database();
        let sem = hir::Semantics::new(db);
        let file = sem.parse(*fid);
        let offset = LineIndex::new(&file.to_string()).offset(LineCol {
            line,
            col_utf16: col,
        });
        let node = match file.syntax().token_at_offset(offset) {
            TokenAtOffset::None => return Err(anyhow!("ast node not found under cursor")),
            TokenAtOffset::Single(node) => node,
            TokenAtOffset::Between(_, r) if r.kind() == syntax::SyntaxKind::IDENT => r,
            TokenAtOffset::Between(l, _) => l,
        };
        let def = if let Some(name) = syntax::ast::Name::cast(node.parent()) {
            ide_db::defs::NameClass::classify(&sem, &name).and_then(|d| d.defined(db))
        } else if let Some(name) = syntax::ast::NameRef::cast(node.parent()) {
            ide_db::defs::NameRefClass::classify(&sem, &name).and_then(|d| Some(d.referenced(db)))
        } else {
            return Err(anyhow!("name not found under cursor"));
        };

        if let Some(def) = def {
            Ok(def)
        } else {
            return Err(anyhow!("definition not found"));
        }
    }
}

fn main() {
    let args = env::args().skip(1).take(1).next();
    let prj_dir = args.expect("project dir must be specified.");

    println!("Prepare to scan {}.", prj_dir);

    let a = Analysis::new(prj_dir).unwrap();

    let mut rl = Editor::<()>::new();

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let args = line.split(":").collect::<Vec<_>>();
                if args.len() == 3 {
                    let line = args[1].parse::<u32>();
                    let col = args[2].parse::<u32>();
                    if let (Ok(line), Ok(col)) = (line, col) {
                        // line number & column number start from 0
                        match a.resolve(&args[0], line - 1, col - 1) {
                            Ok(def) => println!("{:#?}\n", def),
                            Err(e) => println!("err:{:?}", e),
                        }
                        continue;
                    }
                }
                println!("bad format.\nexample:src/a.rs:12:5\n");
            }

            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }
}
