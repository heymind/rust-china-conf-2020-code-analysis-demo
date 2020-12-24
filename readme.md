Code analysis with rust-analyzer
====

自顶向下
------


### 开始

`rust-analyzer` 项目包含了许多 crates ，分别实现了不同的功能，例如：

- `rust-analyzer` crate 作为主要入口，除了承担 LSP 功能外，其中的 `cli` 子模块还提供了一些方便的辅助函数用于扫描目标代码仓库，初始化分析的状态，一会儿就会用到。
- `ide` crate 提供了 IDE 功能的编程接口 （ 如查找函数定义、输入补全等）。
- `parser` crate 负责词法分析，输入代码，输出 Token 流。
- `syntax` crate 负责语法分析，将 Token 流转换为 AST 表示。
- `hir`, `hir_def`, `hir_ty` crate 具有语义分析的功能，内建类型系统，可以进行名字解析等等。
- `project_model` crate 用于解析 Cargo 工程的目录结构，寻找依赖等。

之后我们先实现一个简单的，可用的 Demo，再去具体理解 `rust-analyzer` 的层次结构。

### 配置开发环境

```sh
cargo init play_with_ra --bin #创建项目
```

由于 `rust-analyzer` 相关 crates 并没有发布到 crates.io registry 上，因此需要直接依赖 `rust-analyzer` 的 GitHub 仓库。

```toml
[dependencies]
rust-analyzer = { git = "https://github.com/rust-analyzer/rust-analyzer.git" }
ide = { git = "https://github.com/rust-analyzer/rust-analyzer.git" }
hir = { git = "https://github.com/rust-analyzer/rust-analyzer.git" }
```

### 公开函数扫描器

这里将会实现一个小工具，其作用可以
1. 扫描该工程目录下所有公开函数 (pub fn)
2. 输出他们的全限定名称 ( fully-qualified name )

如果仅实现 #1 其实直接用 `grep` 命令都能实现，但如若 #2 功能就需要理解语义了。


#### Hello World

从命令行参数中去读目标目录，并输出。如果未提供，则报错。

```rs
use std::env;
fn main() {
    let args = env::args().skip(1).take(1).next();
    let prj_dir = args.expect("project dir must be specified.");
    println!("Prepare to scan {}.", prj_dir);
}

```

运行效果。

```sh
⋊> ~/r/n/play_with_ra on master ⨯ cargo run -- ../play_with_ra/
    Finished dev [unoptimized + debuginfo] target(s) in 0.12s
     Running `target/debug/play_with_ra ../play_with_ra/`
Prepare to scan ../play_with_ra/.
```

#### 加载 Cargo 项目

```rs
 // use `load_cargo` to scan codebase.
let (host, vfs) = rust_analyzer::cli::load_cargo(prj_dir.as_ref(), true, false).unwrap();

// get a reference of `RootDatabase`
let db: &ide::RootDatabase = host.raw_database();

for krate in hir::Crate::all(db) {
    println!("Found crate {}", krate.display_name(db).unwrap());
}
```


`load_cargo` 是前文中提到的用于扫描 Cargo 项目的辅助函数，它返回两个值，类型分别为 (AnalysisHost)[https://rust-analyzer.github.io/rust-analyzer/ide/struct.AnalysisHost.html] 和 `Vfs`。


> AnalysisHost stores the current state of the world.


文档上表明，`AnalysisHost` 持有了当前有关代码分析的所有状态。之后我们通过 `raw_database` 方法拿到了 (RootDatabase)[https://rust-analyzer.github.io/rust-analyzer/ide/struct.RootDatabase.html] 的引用。`RootDatabase` 聚合了 `AstDatabase`, `DefDatabase``, HirDatabase` 等，其存放了从语法分析到语义分析之间的信息，以及一些其他的信息。


之后我们调用了 (hir::Crate::all)[https://rust-analyzer.github.io/rust-analyzer/hir/struct.Crate.html#method.all] 方法，从数据库中获取了当前所有的 crates 的列表。可以看到这个函数签名要求的参数为 `&dyn HirDatabase`，而我们传入了 `&RootDatabase` ，印证了 `RootDatabase` 是所有数据信息的集合。


运行上述代码后，预计会打印数十个 crate 的名称，这些 crates 包括了项目的本身，及其直接和间接的依赖。

不过我们现在只关心一个 crate，即我们要扫描的 crate。

```rs
let root_crate_name = Path::new(&prj_dir)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
let krate = hir::Crate::all(db)
        .drain(..)
        .find(|krate| {
            krate
                .display_name(db)
                .filter(|name| name.deref() == &root_crate_name)
                .is_some()
        })
        .unwrap();
```

#### 遍历所有公开函数

为了加速后续代码的运行速度，在实际代码分析过程之前，可以先调用 `prime_caches` 函数对涉及到的所有 crates 进行一次初始缓存。

```rs
host.analysis().prime_caches(|x| println!("{:?}", x));
```

先来回顾一下 Rust 语言中 Module 层级。
```rs

pub mod example {
    pub mod inner {
        fn some_func();
    }
    pub mod inner2 {
        fn some_func2();
    }
}
```

对于 `some_func` 函数而言，其全限定名称应该是 `example::inner::some_func` 。其遍历过程是一个广度优先遍历，使用栈结构。

```rs
// a stack to store modules
let mut modules: Vec<hir::Module> = Default::default();

// push the root module into the stack
modules.push(krate.root_module(db));

// iter over modules until empty stack
while let Some(module) = modules.pop() {
    // iter over all declarations of current module
    for decl in module.declarations(db) {
        // skip the declaration which is not public
        if decl.definition_visibility(db) != Some(hir::Visibility::Public) {
            continue;
        }
        // print the function name without qualified
        match decl {
            hir::ModuleDef::Function(func) => println!("{:?}", func.name(db)),
            _ => (),
        }
    }
    
    modules.extend(module.children(db));
}

```

此外，考虑这种情况。

```rs
pub mod inner {
    pub struct Example {}
    impl Example {
        pub fn example_method(&self){}
    }
}
```

对于上面这种 struct 上附着的方法，可以这样处理。
```rs
for def in module.impl_defs(db) {
        // iter over all impl declarations of current module
        if let Some(hir::Adt::Struct(s)) = def.target_ty(db).as_adt() {
            for item in def.items(db) {
                // skip the declaration which is not public
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
```

#### print_public_function
我们预期要输出所有公开可访问的函数的签名，对于普通的函数和 struct 结构附着的方法这两种情况，可以定义一个统一的输出函数 `print_public_function`。

```rs
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
```


自底向上
------

// todo


演示
-----
```
cargo run --bin top_down -- <path to crate's root>
cargo run --bin bottom_up --  <absolute path of crate's root>
```