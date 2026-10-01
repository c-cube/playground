//! cargo run -p tkhost --example hello
use std::sync::atomic::{AtomicI64, Ordering};

fn main() -> anyhow::Result<()> {
    let ui = tkhost::Ui::new()?;
    static COUNT: AtomicI64 = AtomicI64::new(0);

    ui.set("greeting", "hello from rust");
    ui.handle("add", |args| {
        let n: i64 = args.first().map(|s| s.parse()).transpose()?.unwrap_or(1);
        Ok(COUNT.fetch_add(n, Ordering::Relaxed) + n)
    });
    ui.handle("upper", |args| Ok(args.iter().map(|s| s.to_uppercase()).collect::<Vec<_>>()));
    ui.handle("fail", |_| -> anyhow::Result<()> { anyhow::bail!("this one always fails") });
    ui.on("log", |args| eprintln!("tcl says: {:?}", tkhost::parse_list(&args[0])));
    ui.on_tcl_change(|k, v| eprintln!("rs::ui({k}) = {v:?}"));

    let u = ui.clone();
    std::thread::spawn(move || {
        for i in 0.. {
            u.set("ticks", i);
            u.emit("tick", serde_json::json!({"i": i, "words": ["a b", "{x", "$y\n"]}));
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    ui.run(&[tkhost::tcl_file!("examples/hello.tcl")])
}
