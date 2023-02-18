use std::{env, fs, path::Path, process::Command};

fn ok(cmd: &mut Command) {
    cmd.status()
        .expect("status")
        .success()
        .then_some(())
        .expect("success");
}

fn main() {
    println!("cargo:rerun-if-changed=static");
    let stat = env::current_dir().unwrap().join("static");
    let out = Path::new(&env::var("OUT_DIR").unwrap()).join("static");

    for d in ["css", "js"] {
        fs::create_dir_all(out.join(d)).expect("mkdir");
    }

    for f in ["favicon.png"] {
        fs::copy(stat.join(f), out.join(f)).expect("copy");
    }

    ok(Command::new("sassc")
        .current_dir(&out)
        .arg("--style=compressed")
        .arg("--omit-map-comment")
        .arg(stat.join("css/ircjournal.sass"))
        .arg(out.join("css/ircjournal.css")));

    ok(Command::new("tsc")
        .arg("--project")
        .arg("tsconfig.json")
        .arg("--outDir")
        .arg(out.join("js")));
}
