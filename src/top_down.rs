use hir::HasSource;
use hir::HasVisibility;
use ide::RootDatabase;
use std::ops::Deref;
use std::{env, path::Path};
use syntax::ast::NameOwner;

fn print_public_function(func: hir::Function, db: &RootDatabase, assoc_name: Option<hir::Name>) {
    let mut prefix: Vec<String> = Default::default();
    if let Some(name) = assoc_name {
        prefix.push(name.to_string());
    }
    let f = func.source(db).value;
    let name = f.name().unwrap();
    let params = f.param_list().unwrap();
    let mut module = Some(func.module(db));
    while let Some(m) = module {
        if let Some(name) = m.name(db) {
            prefix.push(name.to_string());
        }

        module = m.parent(db);
    }
    prefix.reverse();
    println!("pub {}::{}{}", prefix.join("::"), name, params);
}

fn main() {
    let args = env::args().skip(1).take(1).next();
    let prj_dir = args.expect("project dir must be specified.");

    let root_crate_name = Path::new(&prj_dir)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    println!("Prepare to scan {}.", prj_dir);

    // use `load_cargo` to scan codebase.
    let (host, _vfs) = rust_analyzer::cli::load_cargo(prj_dir.as_ref(), true, false).unwrap();

    // get a reference of `RootDatabase`
    let db: &ide::RootDatabase = host.raw_database();

    let _ = host.analysis().prime_caches(|_| {});

    let krate = hir::Crate::all(db)
        .drain(..)
        .find(|krate| {
            krate
                .display_name(db)
                .filter(|name| name.deref() == &root_crate_name)
                .is_some()
        })
        .unwrap();

    println!("Found root crate {}", krate.display_name(db).unwrap());

    let mut modules: Vec<hir::Module> = Default::default();
    modules.push(krate.root_module(db));

    while let Some(module) = modules.pop() {
        for decl in module.declarations(db) {
            if decl.definition_visibility(db) != Some(hir::Visibility::Public) {
                continue;
            }
            match decl {
                hir::ModuleDef::Function(func) => print_public_function(func, db, None),
                _ => (),
            }
        }
        for def in module.impl_defs(db) {
            if let Some(hir::Adt::Struct(s)) = def.target_ty(db).as_adt() {
                for item in def.items(db) {
                    if item.visibility(db) != hir::Visibility::Public {
                        continue;
                    }
                    match item {
                        hir::AssocItem::Function(func) => {
                            print_public_function(func, db, Some(s.name(db)))
                        }
                        _ => (),
                    }
                }
            }
        }
        modules.extend(module.children(db));
    }
}
